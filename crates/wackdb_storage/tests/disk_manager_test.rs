use tempfile::tempdir;
use wackdb_common::config::Config;
use wackdb_common::constants::PAGE_SIZE;
use wackdb_common::types::PageId;
use wackdb_storage::DiskManager;

#[test]
fn test_disk_manager_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let config = Config {
        data_dir: dir.path().to_str().unwrap().to_string(),
        ..Config::default()
    };

    let mut dm = DiskManager::new(&config)?;
    let table_id = 1;
    dm.create_file(table_id)?;

    let page_id = PageId::new(table_id, 0);
    let mut data = [0u8; PAGE_SIZE];
    data[0] = 42;
    dm.safe_write_page(page_id, &data)?;

    let read_data = dm.read_page(page_id)?;
    assert_eq!(data, read_data);

    Ok(())
}
