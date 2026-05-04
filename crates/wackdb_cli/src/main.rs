use anyhow::Result;
use std::path::Path;
use wackdb_common::config::Config;
use wackdb_common::types::PageId;
use wackdb_page::SlottedPage;
use wackdb_storage::DiskManager;

const FIXED_RECORD_SIZE: usize = 64;

fn create_fixed_record(data: &str) -> [u8; FIXED_RECORD_SIZE] {
    let mut record = [0u8; FIXED_RECORD_SIZE];
    let bytes = data.as_bytes();
    let len = bytes.len().min(FIXED_RECORD_SIZE);
    record[..len].copy_from_slice(&bytes[..len]);
    record
}

fn main() -> Result<()> {
    let config = Config::default();
    let mut dm = DiskManager::new(&config)?;
    let table_id = 42;

    if !Path::new(&config.data_dir).join("42.db").exists() {
        dm.create_file(table_id)?;
    }

    let mut buffer = [0u8; 8192];
    let mut page = SlottedPage::new(&mut buffer);
    page.init();

    let r1 = create_fixed_record("User: Alice | Email: alice@example.com | Age: 28");
    let r2 = create_fixed_record("User: Bob | Email: bob@rust.org | Age: 34");
    let r3 = create_fixed_record("User: Charlie | Email: charlie@wackdb.io | Age: 22");

    page.insert_tuple(&r1)?;
    page.insert_tuple(&r2)?;
    page.insert_tuple(&r3)?;

    let page_id = PageId::new(table_id, 0);
    dm.safe_write_page(page_id, &buffer)?;

    let mut read_buffer = dm.read_page(page_id)?;
    let read_page = SlottedPage::new(&mut read_buffer);

    println!(
        "Verification of Fixed-Length Records ({} bytes):",
        FIXED_RECORD_SIZE
    );
    for i in 0..3 {
        let data = read_page.get_tuple(i).unwrap();
        println!(
            "  Slot {}: {:?}",
            i,
            String::from_utf8_lossy(data).trim_matches('\0')
        );
    }

    Ok(())
}
