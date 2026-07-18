use colored::Colorize;
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::{Catalog, TableMetadata};
use wackdb_query::executor::predicate::evaluate_where_clause;
use wackdb_sql::{Ast, WhereCondition};
use wackdb_storage::{CTID, DiskManager};
use wackdb_tuple::{Schema, Tuple, value::Value};

/// Executes DML statements (INSERT, DELETE)
///
/// # Errors
/// Returns an error if catalog lookup, tuple formatting, B+Tree search, or WAL operations fail.
pub fn execute_dml<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    ast: Ast,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    quiet: bool,
    is_recovery: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = match ast {
        Ast::Insert { table, values } => {
            execute_insert(&table, &values, catalog, bpm, quiet, is_recovery)
        }
        Ast::Delete {
            table,
            where_clause,
        } => execute_delete(&table, &where_clause, catalog, bpm, quiet, is_recovery),
        _ => Err("Not a DML statement.".into()),
    };

    if !is_recovery && result.is_ok() {
        if let Some(lm) = bpm.get_log_manager() {
            let _ = lm.flush_up_to(u64::MAX);
        }
    }

    result
}

fn execute_insert<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    table: &str,
    values: &[String],
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    quiet: bool,
    is_recovery: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let meta = catalog.get_table(table)?;
    let schema = catalog.get_schema(table).map_err(|_| "Schema not found")?;

    let parsed_vals = parse_values(values, &schema)?;
    let tuple = Tuple::from_values(&schema, &parsed_vals)?;

    let mut btree = get_btree(&*bpm, &meta);

    let Some(Value::Integer(pk)) = parsed_vals.first() else {
        return Ok(());
    };

    if wackdb_btree::traits::Index::search(&btree, *pk).is_ok() {
        if !quiet {
            println!("{}", format!("[ERR] UniqueConstraintViolation: Primary Key '{}' already exists in relation '{}'.", pk, table).bold().red());
        }
        return Ok(());
    }

    let ctid = write_tuple_to_heap(&tuple, meta.heap_relation_id, &*bpm)?;

    if !is_recovery {
        log_insert_to_wal(table, &tuple, &*bpm)?;
    }

    wackdb_btree::traits::Index::insert(&mut btree, *pk, ctid)?;

    if let Some(r) = btree.get_root_page_id() {
        catalog.update_root_page(table, Some(r.page_num))?;
    }

    catalog.update_num_records(table, 1)?;

    if !quiet {
        println!(
            "{}",
            format!("[OK] Inserted 1 row into '{}'.", table)
                .bold()
                .green()
        );
    }

    Ok(())
}

fn execute_delete<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    table: &str,
    where_clause: &[WhereCondition],
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    quiet: bool,
    is_recovery: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let meta = catalog.get_table(table)?;
    let schema = catalog.get_schema(table).map_err(|_| "Schema not found")?;

    let total_pages = bpm.get_total_pages(meta.heap_relation_id)?;
    let mut deleted_count = 0;

    for current_page in 0..total_pages {
        deleted_count += process_page_for_delete(
            current_page,
            table,
            where_clause,
            &meta,
            &schema,
            bpm,
            is_recovery,
        )?;
    }

    if deleted_count > 0 {
        catalog.update_num_records(table, -(deleted_count as i32))?;
    }

    if !quiet {
        println!(
            "{}",
            format!("[OK] Deleted {} rows from '{}'.", deleted_count, table)
                .bold()
                .green()
        );
    }

    Ok(())
}

fn process_page_for_delete<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    page_num: u32,
    table: &str,
    where_clause: &[WhereCondition],
    meta: &TableMetadata,
    schema: &Schema,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    is_recovery: bool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let page_id = wackdb_storage::PageId {
        file_id: meta.heap_relation_id,
        page_num,
    };
    let Ok(frame_id) = bpm.fetch_page(page_id) else {
        return Ok(0);
    };

    let mut page_write = bpm.write_page(frame_id);
    let to_delete = find_slots_to_delete(&page_write, where_clause, schema);

    let deleted = delete_slots_from_page(
        &mut page_write,
        &to_delete,
        table,
        meta,
        schema,
        bpm,
        is_recovery,
    )?;

    drop(page_write);
    let _ = bpm.unpin_page(page_id, true);

    Ok(deleted)
}

fn find_slots_to_delete<const PAGE_SIZE: usize>(
    page_write: &parking_lot::RwLockWriteGuard<'_, wackdb_page::SlottedPage<PAGE_SIZE>>,
    where_clause: &[WhereCondition],
    schema: &Schema,
) -> Vec<u16> {
    let mut to_delete = Vec::new();
    let total_slots = wackdb_query::get_total_slots_from_bytes(&page_write.data.0) as u16;

    for slot_idx in 0..total_slots {
        let Some(record) =
            wackdb_query::get_record_from_bytes(&page_write.data.0, slot_idx as usize)
        else {
            continue;
        };
        let tuple = Tuple {
            data: record.1.to_vec(),
        };
        let Ok(vals) = tuple.to_values(schema) else {
            continue;
        };

        if evaluate_where_clause(where_clause, schema, &vals) {
            to_delete.push(slot_idx);
        }
    }
    to_delete
}

fn delete_slots_from_page<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    page_write: &mut parking_lot::RwLockWriteGuard<'_, wackdb_page::SlottedPage<PAGE_SIZE>>,
    to_delete: &[u16],
    table: &str,
    meta: &TableMetadata,
    schema: &Schema,
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
    is_recovery: bool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut deleted = 0;

    for &slot_idx in to_delete {
        page_write.mark_deleted(slot_idx as usize, 999);
        deleted += 1;

        let tuple = extract_tuple_for_slot(page_write, slot_idx);
        remove_from_index(&tuple, schema, bpm, meta);
        log_delete_to_wal(table, &tuple, page_write, bpm, is_recovery);
    }

    Ok(deleted)
}

fn extract_tuple_for_slot<const PAGE_SIZE: usize>(
    page_write: &parking_lot::RwLockWriteGuard<'_, wackdb_page::SlottedPage<PAGE_SIZE>>,
    slot_idx: u16,
) -> Tuple {
    let offset = page_write.slots()[slot_idx as usize].offset as usize;
    let length = page_write.slots()[slot_idx as usize].length as usize;
    let header_size = std::mem::size_of::<wackdb_page::TupleHeader>();
    let tuple_data = &page_write.data[offset + header_size..offset + length];
    Tuple {
        data: tuple_data.to_vec(),
    }
}

fn remove_from_index<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    tuple: &Tuple,
    schema: &Schema,
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
    meta: &TableMetadata,
) {
    if let Ok(vals) = tuple.to_values(schema) {
        if let Some(Value::Integer(pk)) = vals.first() {
            let mut btree = get_btree(bpm, meta);
            let _ = wackdb_btree::traits::Index::delete(&mut btree, *pk);
        }
    }
}

fn log_delete_to_wal<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    table: &str,
    tuple: &Tuple,
    page_write: &mut parking_lot::RwLockWriteGuard<'_, wackdb_page::SlottedPage<PAGE_SIZE>>,
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
    is_recovery: bool,
) {
    if is_recovery {
        return;
    }

    if let Some(lm) = bpm.get_log_manager() {
        let mut payload = Vec::new();
        payload.push(1u8); // OP: 1 = DELETE
        let tn_bytes = table.as_bytes();
        payload.extend_from_slice(&(tn_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(tn_bytes);
        payload.extend_from_slice(&tuple.data);

        if let Ok(lsn) = lm.append_record(&payload) {
            page_write.set_lsn(lsn);
        }
    }
}

fn parse_values(
    values: &[String],
    schema: &Schema,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut parsed_vals = Vec::new();
    for (v, col) in values.iter().zip(schema.columns.iter()) {
        let parsed = match col.data_type {
            wackdb_tuple::DataType::Integer => Value::Integer(v.parse::<i32>()?),
            wackdb_tuple::DataType::Boolean => Value::Boolean(v.parse::<bool>()?),
            wackdb_tuple::DataType::Varchar => Value::Varchar(v.clone()),
        };
        parsed_vals.push(parsed);
    }
    Ok(parsed_vals)
}

fn write_tuple_to_heap<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    tuple: &Tuple,
    heap_file_id: u32,
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
) -> Result<CTID, Box<dyn std::error::Error>> {
    let (frame_id, page_id) = bpm.new_page(heap_file_id)?;
    let slot_idx = {
        let mut page = bpm.write_page(frame_id);
        page.init();
        page.insert_record(&tuple.data, 1).unwrap_or(0)
    };
    bpm.unpin_page(page_id, true)?;
    Ok(CTID {
        page_id,
        slot_idx: slot_idx as u16,
    })
}

fn get_btree<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    bpm: &'a BufferPoolManager<PAGE_SIZE, D>,
    meta: &TableMetadata,
) -> BTreeIndex<'a, PAGE_SIZE, D> {
    BTreeIndex::new(
        bpm,
        meta.root_page_num.map(|n| wackdb_storage::PageId {
            file_id: meta.index_relation_id,
            page_num: n,
        }),
        meta.index_relation_id,
    )
}

fn log_insert_to_wal<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    table: &str,
    tuple: &Tuple,
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(lm) = bpm.get_log_manager() else {
        return Ok(());
    };
    let mut payload = Vec::new();
    payload.push(0u8); // OP: 0 = INSERT
    let tn_bytes = table.as_bytes();
    payload.extend_from_slice(&(tn_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(tn_bytes);
    payload.extend_from_slice(&tuple.data);

    let _ = lm.append_record(&payload);
    Ok(())
}
