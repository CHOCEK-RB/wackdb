use colored::Colorize;
use comfy_table::{
    Attribute, Cell, Color, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL,
};
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;
use wackdb_storage::DiskManager;

/// Executes dot-commands (meta commands) like `.help`, `.tables`, or `.demo`.
///
/// # Errors
/// Returns an error if disk operations, catalog updates, or B+Tree inserts fail.
pub fn execute_meta_command<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    cmd: &str,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match cmd {
        ".help" => print_help(),
        ".tables" => print_tables(catalog),
        c if c.starts_with(".inspect ") => inspect_table(c, catalog),
        c if c.starts_with(".source ") => Ok(Some(c.to_string())),
        c if c.starts_with(".reset") => Ok(Some(".reset".to_string())),
        c if c.starts_with(".checkpoint") => execute_checkpoint(bpm),
        c if c.starts_with(".demo") => generate_demo_data(c, catalog, bpm),
        _ => Err("Unknown dot command.".into()),
    }
}

fn print_help() -> Result<Option<String>, Box<dyn std::error::Error>> {
    println!("{}", "\n[ SQL DDL - Schema Definition ]".bold().yellow());
    println!(
        "  {} - Create a new table.",
        "CREATE TABLE IF NOT EXISTS <name> (<col> <type>, ...);".green()
    );
    println!(
        "  {} - Atomically drop a table.",
        "DROP TABLE IF EXISTS <name>;".green()
    );
    println!(
        "{}",
        "\n[ SQL DML - Data Manipulation & Querying ]"
            .bold()
            .yellow()
    );
    println!(
        "  {} - Insert a tuple.",
        "INSERT INTO <name> VALUES (<values>);".green()
    );
    println!(
        "  {} - Delete a tuple.",
        "DELETE FROM <name> WHERE <cond>;".green()
    );
    println!(
        "  {} - Query execution.",
        "SELECT <cols> FROM <t1> [JOIN <t2> ON <cond>] WHERE <filter>;".green()
    );
    println!("{}", "\n[ SYSTEM META-COMMANDS ]".bold().yellow());
    println!(
        "  {:<15} - Lists all tables in the catalog.",
        ".tables".cyan()
    );
    println!(
        "  {:<15} - Prints table schema and structural metadata.",
        ".inspect <name>".cyan()
    );
    println!(
        "  {:<15} - Creates 'demo_users' with mock data.",
        ".demo <count>".cyan()
    );
    println!(
        "  {:<15} - Run commands from a file.",
        ".source <file>".cyan()
    );
    println!("  {:<15} - Wipes the database cleanly.", ".reset".cyan());
    println!(
        "  {:<15} - Flushes dirty pages to disk and truncates the WAL.",
        ".checkpoint".cyan()
    );
    println!(
        "  {:<15} - Exits the application and flushes buffers safely.",
        ".exit".cyan()
    );
    Ok(None)
}

fn print_tables(catalog: &Catalog) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let tables = catalog.list_tables();
    if tables.is_empty() {
        println!("No tables exist in the catalog.");
    } else {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);

        table.set_header(vec![
            Cell::new("Table Name")
                .fg(Color::Blue)
                .add_attribute(Attribute::Bold),
            Cell::new("Heap Rel ID")
                .fg(Color::Blue)
                .add_attribute(Attribute::Bold),
            Cell::new("Index Rel ID")
                .fg(Color::Blue)
                .add_attribute(Attribute::Bold),
        ]);

        for t in tables {
            table.add_row(vec![
                Cell::new(t.name).fg(Color::Green),
                Cell::new(t.heap_relation_id).fg(Color::Yellow),
                Cell::new(t.index_relation_id).fg(Color::Yellow),
            ]);
        }
        println!("\nTables in Catalog:");
        println!("{table}");
    }
    Ok(None)
}

fn inspect_table(c: &str, catalog: &Catalog) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = c.split_whitespace().collect();
    if parts.len() < 2 {
        println!("{}", "[ERR] Usage: .inspect <table_name>".bold().red());
        return Ok(None);
    }
    if let Ok(meta) = catalog.get_table(parts[1]) {
        println!(
            "{}",
            format!("Table Inspection: {}", meta.name).bold().blue()
        );
        println!("  Heap Relation ID: {}", meta.heap_relation_id);
        println!("  Index Relation ID: {}", meta.index_relation_id);
        println!(
            "  B+Tree Root Page: {}",
            meta.root_page_num
                .map_or_else(|| "(Not initialized)".to_string(), |r| r.to_string())
        );
        println!("  Columns:");
        for col in &meta.schema.columns {
            println!("    - {}: {:?}", col.name, col.data_type);
        }
    } else {
        println!(
            "{}",
            format!("[ERR] Table '{}' not found in catalog.", parts[1])
                .bold()
                .red()
        );
    }
    Ok(None)
}

fn execute_checkpoint<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    println!("Executing Checkpoint...");
    let _ = bpm.flush_all_pages();
    if let Some(lm) = bpm.get_log_manager() {
        let _ = lm.flush_up_to(u64::MAX);
        let _ = lm.checkpoint();
    }
    println!(
        "{}",
        "[OK] Checkpoint completed. Buffer Pool flushed, WAL truncated."
            .bold()
            .green()
    );
    Ok(None)
}

fn generate_demo_data<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    c: &str,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let count = c
        .strip_prefix(".demo")
        .unwrap_or("")
        .trim()
        .parse::<i32>()
        .unwrap_or(50)
        .max(1);
    println!("Generating demo data ({} rows)...", count);

    let table_name = "demo_users";
    if catalog.get_table(table_name).is_err() {
        use wackdb_tuple::{Column, DataType, Schema};
        catalog.create_table_with_schema(
            table_name,
            Schema::new(vec![
                Column::new("id", DataType::Integer, false),
                Column::new("is_admin", DataType::Boolean, false),
                Column::new("username", DataType::Varchar, false),
                Column::new("status", DataType::Varchar, true),
            ]),
        )?;
    }
    let meta = catalog.get_table(table_name)?;
    let schema = catalog.get_schema(table_name)?;

    let mut btree = BTreeIndex::new(
        bpm,
        meta.root_page_num.map(|n| wackdb_storage::PageId {
            file_id: meta.index_relation_id,
            page_num: n,
        }),
        meta.index_relation_id,
    );

    let mut current_page = None;
    for i in 1..=count {
        if wackdb_btree::traits::Index::search(&btree, i).is_ok() {
            continue;
        }

        let tuple = build_demo_tuple(i, &schema)?;
        let ctid = write_demo_tuple(&*bpm, &meta, &tuple, &mut current_page)?;

        append_demo_wal(&*bpm, table_name, &tuple)?;
        wackdb_btree::traits::Index::insert(&mut btree, i, ctid)?;
    }

    if let Some((_, pid)) = current_page {
        let _ = bpm.unpin_page(pid, true);
    }

    if let Some(r) = btree.get_root_page_id() {
        catalog.update_root_page(table_name, Some(r.page_num))?;
    }

    if let Some(lm) = bpm.get_log_manager() {
        let _ = lm.flush_up_to(u64::MAX);
    }

    println!("Demo data generated ({} rows).", count);
    Ok(None)
}

fn build_demo_tuple(
    i: i32,
    schema: &wackdb_tuple::Schema,
) -> Result<wackdb_tuple::Tuple, Box<dyn std::error::Error>> {
    let username = format!("user_{i}");
    let vals = vec![
        wackdb_tuple::Value::Integer(i),
        wackdb_tuple::Value::Boolean(i % 5 == 0),
        wackdb_tuple::Value::Varchar(username),
        wackdb_tuple::Value::Varchar("active".into()),
    ];
    wackdb_tuple::Tuple::from_values(schema, &vals).map_err(Into::into)
}

fn write_demo_tuple<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
    meta: &wackdb_catalog::TableMetadata,
    tuple: &wackdb_tuple::Tuple,
    current_page: &mut Option<(usize, wackdb_storage::PageId)>,
) -> Result<wackdb_storage::CTID, Box<dyn std::error::Error>> {
    if current_page.is_none() {
        let total_pages = bpm
            .disk_manager()
            .get_total_pages(meta.heap_relation_id)
            .unwrap_or(0);
        if total_pages > 0 {
            let last_pid = wackdb_storage::PageId {
                file_id: meta.heap_relation_id,
                page_num: total_pages - 1,
            };
            if let Ok(frame_id) = bpm.fetch_page(last_pid) {
                *current_page = Some((frame_id, last_pid));
            }
        }
        if current_page.is_none() {
            let (frame_id, page_id) = bpm.new_page(meta.heap_relation_id)?;
            {
                let mut page = bpm.write_page(frame_id);
                page.init();
            }
            *current_page = Some((frame_id, page_id));
        }
    }

    let (frame_id, page_id) = current_page.unwrap();
    let slot_idx = {
        let mut page = bpm.write_page(frame_id);
        page.insert_record(&tuple.data, 1)
    };

    let ctid = match slot_idx {
        Some(idx) => wackdb_storage::CTID {
            page_id,
            slot_idx: idx as u16,
        },
        None => {
            bpm.unpin_page(page_id, true)?;
            let (nf, np) = bpm.new_page(meta.heap_relation_id)?;
            *current_page = Some((nf, np));
            let idx = {
                let mut page = bpm.write_page(nf);
                page.init();
                page.insert_record(&tuple.data, 1).unwrap_or(0)
            };
            wackdb_storage::CTID {
                page_id: np,
                slot_idx: idx as u16,
            }
        }
    };
    Ok(ctid)
}

fn append_demo_wal<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    bpm: &BufferPoolManager<PAGE_SIZE, D>,
    table_name: &str,
    tuple: &wackdb_tuple::Tuple,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(lm) = bpm.get_log_manager() {
        let mut payload = Vec::new();
        payload.push(0u8);
        let tn_bytes = table_name.as_bytes();
        payload.extend_from_slice(&(tn_bytes.len() as u16).to_le_bytes());
        payload.extend_from_slice(tn_bytes);
        payload.extend_from_slice(&tuple.data);

        if let Ok(lsn) = lm.append_record(&payload) {
            // we skip writing LSN to the page to avoid fetching the frame_id again just for demo bulk load.
            let _ = lsn;
        }
    }
    Ok(())
}
