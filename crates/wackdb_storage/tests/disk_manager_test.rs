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

    let page_id = dm.allocate_page(table_id)?;
    assert_eq!(page_id, PageId(0));

    let mut data = [0u8; PAGE_SIZE];
    data[0] = 42;
    data[PAGE_SIZE - 1] = 24;
    dm.write_page(table_id, page_id, &data)?;

    let read_data = dm.read_page(table_id, page_id)?;
    assert_eq!(data, read_data);

    let page_id2 = dm.allocate_page(table_id)?;
    assert_eq!(page_id2, PageId(1));

    let table2_id = 2;
    let t2_page0 = dm.allocate_page(table2_id)?;
    assert_eq!(t2_page0, PageId(0));

    Ok(())
}
