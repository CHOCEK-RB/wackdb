use anyhow::Result;
use std::path::Path;
use wackdb_common::config::Config;
use wackdb_common::types::PageId;
use wackdb_page::SlottedPage;
use wackdb_storage::DiskManager;

fn main() -> Result<()> {
    let config = Config::default();
    let mut dm = DiskManager::new(&config)?;
    let table_id = 42;

    println!("--- WackDB Milestone Proof ---");

    // 1. Create file
    if !Path::new(&config.data_dir).join("42.db").exists() {
        println!("Creating table file 42.db...");
        dm.create_file(table_id)?;
    }

    // 2. Prepare a page in memory
    let mut buffer = [0u8; 8192];
    let mut page = SlottedPage::new(&mut buffer);
    page.init();

    // 3. Insert tuples
    println!("Inserting tuples...");
    let t1 = b"Row 1: Data Foundations";
    let t2 = b"Row 2: Atomic Persistence";
    let t3 = b"Row 3: Slotted Page Manager";

    let s1 = page.insert_tuple(t1)?;
    let s2 = page.insert_tuple(t2)?;
    let s3 = page.insert_tuple(t3)?;

    println!("  Inserted at slots: {}, {}, {}", s1, s2, s3);

    // 4. Safe Write to disk
    let page_id = PageId::new(table_id, 0);
    println!(
        "Performing safe write (atomic swap) for Page ID: {:?}...",
        page_id
    );
    dm.safe_write_page(page_id, &buffer)?;

    // 5. Read back to verify
    println!("Reading back from disk...");
    let mut read_buffer = dm.read_page(page_id)?;
    let read_page = SlottedPage::new(&mut read_buffer);

    println!("Verification:");
    println!(
        "  Slot 0: {:?}",
        String::from_utf8_lossy(read_page.get_tuple(0).unwrap())
    );
    println!(
        "  Slot 1: {:?}",
        String::from_utf8_lossy(read_page.get_tuple(1).unwrap())
    );
    println!(
        "  Slot 2: {:?}",
        String::from_utf8_lossy(read_page.get_tuple(2).unwrap())
    );

    let file_path = Path::new(&config.data_dir).join("42.db");
    println!(
        "\nPhysical proof: File exists at {:?} (size: {} bytes)",
        file_path,
        file_path.metadata()?.len()
    );

    Ok(())
}
