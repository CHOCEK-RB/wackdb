use crate::state::AppState;
use std::error::Error;
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;

pub fn process_command<D: wackdb_storage::DiskManager<8192>>(
    cmd: &str,
    state: &mut AppState,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<8192, D>,
) -> Result<(), Box<dyn Error>> {
    state.log(format!("> {}", cmd));

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    match parts[0] {
        "\\help" => {
            state.log("Available Commands:");
            state.log("  \\list-tables         - Lists all tables in the catalog.");
            state.log(
                "  \\demo <count>        - Creates 'demo_users' and generates realistic data.",
            );
            state.log("  \\select <name>       - Prints data from a table.");
            state.log("  \\search <key>        - Searches for a specific key in the active table's B+Tree.");
            state.log("  \\delete <key>        - Deletes a key from the active table's B+Tree.");
            state.log("  \\stats               - Displays detailed system statistics.");
            state.log("  \\flush               - Flushes all pages to disk.");
            state.log("  \\quit                - Exits the application.");
        }
        "\\list-tables" => {
            let tables = catalog.list_tables();
            if tables.is_empty() {
                state.log("No tables exist in the catalog.");
            } else {
                state.log("Tables in Catalog:");
                for t in tables {
                    state.log(format!(
                        "  - {} (Heap: {}, Index: {})",
                        t.name, t.heap_relation_id, t.index_relation_id
                    ));
                }
            }
        }
        "\\demo" => {
            let count = if parts.len() > 1 {
                parts[1].parse::<usize>().unwrap_or(500)
            } else {
                500
            };

            state.log(format!(
                "Starting automated WackDB demonstration ({} records)...",
                count
            ));
            let table_name = "demo_users";

            // Create table if it doesn't exist
            if catalog.get_table(table_name).is_err() && catalog.create_table(table_name).is_err() {
                state.log("Error creating demo table".to_string());
                return Ok(());
            }

            let meta = catalog.get_table(table_name)?;
            let heap_id = meta.heap_relation_id;
            let index_id = meta.index_relation_id;

            // Define the Schema for the demo table
            let schema = wackdb_tuple::Schema::new(vec![
                wackdb_tuple::Column::new("id", wackdb_tuple::DataType::Integer, false),
                wackdb_tuple::Column::new("is_admin", wackdb_tuple::DataType::Boolean, false),
                wackdb_tuple::Column::new("username", wackdb_tuple::DataType::Varchar, false),
                wackdb_tuple::Column::new("status", wackdb_tuple::DataType::Varchar, true),
            ]);

            state.active_table = Some(table_name.to_string());
            state.root_id =
                catalog
                    .get_table(table_name)?
                    .root_page_num
                    .map(|num| wackdb_storage::PageId {
                        file_id: index_id,
                        page_num: num,
                    });

            state.log(format!(
                "Table '{}' active (Heap: {}, Index: {}).",
                table_name, heap_id, index_id
            ));
            state.log(format!("Generating {} representative records...", count));

            let mut btree = BTreeIndex::new(bpm, state.root_id, index_id);
            let mut splits = 0;
            let mut current_page = None;
            let mut pages_allocated = 0;

            for i in 0..count {
                // Generate tuple values
                let is_admin = i % 10 == 0;
                let username = format!("user_{i}");
                let status = if i % 3 == 0 {
                    wackdb_tuple::Value::Null
                } else {
                    wackdb_tuple::Value::Varchar("active".to_string())
                };

                let values = vec![
                    wackdb_tuple::Value::Integer(i as i32),
                    wackdb_tuple::Value::Boolean(is_admin),
                    wackdb_tuple::Value::Varchar(username),
                    status,
                ];

                let tuple = match wackdb_tuple::Tuple::from_values(&schema, &values) {
                    Ok(t) => t,
                    Err(e) => {
                        state.log(format!("Error serializing tuple: {}", e));
                        continue;
                    }
                };
                let record_bytes = &tuple.data;

                // Allocate a new SlottedPage if doesn't exist
                if current_page.is_none() {
                    let (frame_id, page_id) = bpm.new_page(heap_id)?;
                    {
                        let mut page = bpm.write_page(frame_id);
                        page.init(); // Initialize empty slotted page
                    }
                    current_page = Some((frame_id, page_id));
                    pages_allocated += 1;
                }

                let (frame_id, page_id) = current_page.unwrap();

                // Try to insert into current page
                let slot_idx = {
                    let mut page = bpm.write_page(frame_id);
                    page.insert_record(record_bytes, 1)
                };

                let ctid = match slot_idx {
                    Some(idx) => wackdb_storage::CTID {
                        page_id,
                        slot_idx: idx as u16,
                    },
                    None => {
                        // Page full, unpin and allocate new
                        bpm.unpin_page(page_id, true)?;
                        let (new_frame_id, new_page_id) = bpm.new_page(heap_id)?;
                        current_page = Some((new_frame_id, new_page_id));
                        pages_allocated += 1;

                        let idx = {
                            let mut page = bpm.write_page(new_frame_id);
                            page.init();
                            page.insert_record(record_bytes, 1).unwrap_or(0)
                        };
                        wackdb_storage::CTID {
                            page_id: new_page_id,
                            slot_idx: idx as u16,
                        }
                    }
                };

                let old_root = btree.get_root_page_id();
                let _ = wackdb_btree::traits::Index::insert(&mut btree, i as i32, ctid);
                let new_root = btree.get_root_page_id();
                if old_root != new_root && old_root.is_some() {
                    splits += 1;
                }
            }

            // Unpin the last data page
            if let Some((_, page_id)) = current_page {
                bpm.unpin_page(page_id, true)?;
            }

            state.root_id = btree.get_root_page_id();
            if let Some(r) = state.root_id {
                catalog.update_root_page(table_name, Some(r.page_num))?;
            }

            state.log("--- Demo Summary ---");
            state.log(format!("Records inserted: {}", count));
            state.log(format!("Data pages allocated: {}", pages_allocated));
            state.log(format!("B+Tree Root Splits: {}", splits));
            state.log(format!("Current Root Page: {:?}", state.root_id));
            state.log("Buffer pool active. Type \\flush to persist.");
        }
        "\\flush" => match bpm.flush_all_pages() {
            Ok(_) => {
                catalog.flush()?;
                state.log("Success: Buffer pool and Catalog flushed successfully.");
            }
            Err(e) => state.log(format!("Error flushing: {:?}", e)),
        },
        "\\select" => {
            if parts.len() < 2 {
                state.log("Usage: \\select <table_name>".to_string());
                return Ok(());
            }
            process_select(state, catalog, bpm, parts[1])?;
        }
        "\\stats" => {
            state.log("System Statistics:");
            state.log(format!("  Buffer Hits: {}", bpm.get_hits()));
            state.log(format!("  Buffer Misses: {}", bpm.get_misses()));
            state.log(format!("  Hit Rate: {:.2}%", bpm.get_hit_rate() * 100.0));
            state.log(format!("  Data Directory: {}", state.data_dir));
            state.log(format!("  Active Table: {:?}", state.active_table));
        }
        "\\search" => {
            if parts.len() < 2 {
                state.log("Usage: \\search <key>".to_string());
                return Ok(());
            }
            if let Ok(key) = parts[1].parse::<i32>() {
                process_search_delete(state, catalog, bpm, key, false)?;
            } else {
                state.log("Error: Key must be an integer.".to_string());
            }
        }
        "\\delete" => {
            if parts.len() < 2 {
                state.log("Usage: \\delete <key>".to_string());
                return Ok(());
            }
            if let Ok(key) = parts[1].parse::<i32>() {
                process_search_delete(state, catalog, bpm, key, true)?;
            } else {
                state.log("Error: Key must be an integer.".to_string());
            }
        }
        _ => {
            state.log(format!("Error: Unknown command '{}'", parts[0]));
            state.log("Type \\help for a list of commands.");
        }
    }

    Ok(())
}

fn process_select<D: wackdb_storage::DiskManager<8192>>(
    state: &mut AppState,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<8192, D>,
    table_name: &str,
) -> Result<(), String> {
    let meta = match catalog.get_table(table_name) {
        Ok(m) => m,
        Err(_) => {
            state.log(format!("Table '{}' not found.", table_name));
            return Ok(());
        }
    };

    if table_name != "demo_users" {
        state.log("Note: Only 'demo_users' schema is currently fully supported for binary deserialization.".to_string());
        return Ok(());
    }

    let schema = wackdb_tuple::Schema::new(vec![
        wackdb_tuple::Column::new("id", wackdb_tuple::DataType::Integer, false),
        wackdb_tuple::Column::new("is_admin", wackdb_tuple::DataType::Boolean, false),
        wackdb_tuple::Column::new("username", wackdb_tuple::DataType::Varchar, false),
        wackdb_tuple::Column::new("status", wackdb_tuple::DataType::Varchar, true),
    ]);

    state.active_table = Some(table_name.to_string());
    state.root_id = meta.root_page_num.map(|num| wackdb_storage::PageId {
        file_id: meta.index_relation_id,
        page_num: num,
    });

    let mut printed = 0;

    state.log(format!("--- Data for table '{}' ---", table_name));

    // Primitive sequential scan: read pages until we hit empty pages.
    // In a real DBMS, we would use the system catalog's extent map or an iterator.
    for page_num in 0..10 {
        // Limit scan for demo purposes
        let page_id = wackdb_storage::PageId {
            file_id: meta.heap_relation_id,
            page_num,
        };

        let frame_id = match bpm.fetch_page(page_id) {
            Ok(id) => id,
            Err(_) => break, // Reached EOF or unallocated space
        };

        let mut empty_page = false;
        {
            let page = bpm.read_page(frame_id);
            if page.header().total_slots == 0 {
                empty_page = true;
            } else {
                for slot_idx in 0..page.header().total_slots as usize {
                    if let Some((header, record_bytes)) = page.get_record(slot_idx) {
                        // Check if the record is logically deleted (xmax != INVALID_TXN_ID)
                        if header.xmax != 0 {
                            continue;
                        }

                        let tuple = wackdb_tuple::Tuple {
                            data: record_bytes.to_vec(),
                        };
                        if let Ok(values) = tuple.to_values(&schema) {
                            // Format the values into a readable row
                            let row_str = values
                                .iter()
                                .map(|v| match v {
                                    wackdb_tuple::Value::Null => "NULL".to_string(),
                                    wackdb_tuple::Value::Integer(i) => i.to_string(),
                                    wackdb_tuple::Value::Boolean(b) => b.to_string(),
                                    wackdb_tuple::Value::Varchar(s) => s.clone(),
                                })
                                .collect::<Vec<String>>()
                                .join(" | ");

                            state.log(format!("Row {printed}: {}", row_str));
                            printed += 1;

                            if printed >= 25 {
                                // Limit output for TUI readability
                                state.log("... (output truncated at 50 rows) ...".to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }

        let _ = bpm.unpin_page(page_id, false);

        if empty_page || printed >= 50 {
            break;
        }
    }

    if printed == 0 {
        state.log("0 rows returned.".to_string());
    } else {
        state.log(format!("--- End of Scan ({} rows) ---", printed));
    }

    Ok(())
}

fn process_search_delete<D: wackdb_storage::DiskManager<8192>>(
    state: &mut AppState,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<8192, D>,
    key: i32,
    is_delete: bool,
) -> Result<(), Box<dyn Error>> {
    let table_name = match &state.active_table {
        Some(name) => name.clone(),
        None => {
            state.log("No active table. Use \\demo or \\select to activate one.".to_string());
            return Ok(());
        }
    };

    let meta = catalog.get_table(&table_name)?;
    let index_id = meta.index_relation_id;
    let root_page_id = meta.root_page_num.map(|num| wackdb_storage::PageId {
        file_id: index_id,
        page_num: num,
    });

    let mut btree = BTreeIndex::new(bpm, root_page_id, index_id);

    if is_delete {
        // Find the CTID before deleting from the index so we can delete from the heap
        let ctid_to_delete = wackdb_btree::traits::Index::search(&btree, key).ok();

        match wackdb_btree::traits::Index::delete(&mut btree, key) {
            Ok(_) => {
                state.log(format!("Successfully deleted key {key} from index."));
                // Also mark as deleted in the heap to keep the Demo consistent
                if let Some(ctid) = ctid_to_delete {
                    #[allow(clippy::collapsible_if)]
                    if let Ok(heap_frame) = bpm.fetch_page(ctid.page_id) {
                        let mut heap_page = bpm.write_page(heap_frame);
                        heap_page.mark_deleted(ctid.slot_idx as usize, 999); // 999 is a dummy xmax for the demo
                        drop(heap_page);
                        let _ = bpm.unpin_page(ctid.page_id, true);
                    }
                }
            }
            Err(e) => state.log(format!("Failed to delete key {key}: {:?}", e)),
        }
        // Update root page in catalog if it changed
        if let Some(r) = btree.get_root_page_id() {
            catalog.update_root_page(&table_name, Some(r.page_num))?;
            state.root_id = Some(r);
        }
    } else {
        match wackdb_btree::traits::Index::search(&btree, key) {
            Ok(ctid) => state.log(format!("Key {key} found at CTID: {:?}", ctid)),
            Err(_) => state.log(format!("Key {key} not found.")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;
    use wackdb_storage::disk_manager::BasicDiskManager;

    fn setup_test_env() -> (
        tempfile::TempDir,
        AppState,
        Catalog,
        BufferPoolManager<8192, BasicDiskManager<8192>>,
    ) {
        let dir = tempdir().unwrap();
        let data_dir = dir.path().join("test_data").to_str().unwrap().to_string();
        let disk_manager = BasicDiskManager::<8192>::new(Path::new(&data_dir)).unwrap();
        let bpm = BufferPoolManager::new(50, disk_manager);

        let catalog = Catalog::open(&data_dir).unwrap();
        let state = AppState::new(&data_dir);

        (dir, state, catalog, bpm)
    }

    #[test]
    fn test_process_command_invalid() {
        let (_dir, mut state, mut catalog, mut bpm) = setup_test_env();

        process_command("invalid_cmd", &mut state, &mut catalog, &mut bpm).unwrap();
        assert!(state.logs.iter().any(|l| l.contains("Unknown command")));
    }
}
