use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use wackdb_common::config::Config;
use wackdb_common::constants::PAGE_SIZE;
use wackdb_common::errors::DatabaseError;
use wackdb_common::types::PageId;

pub struct DiskManager {
    data_dir: PathBuf,
    opened_files: HashMap<u32, File>,
}

impl DiskManager {
    pub fn new(config: &Config) -> Result<Self, DatabaseError> {
        let data_dir = PathBuf::from(&config.data_dir);
        if !data_dir.exists() {
            fs::create_dir_all(&data_dir).map_err(|e| DatabaseError::Io(e.to_string()))?;
        }

        Ok(Self {
            data_dir,
            opened_files: HashMap::new(),
        })
    }

    pub fn read_page(
        &mut self,
        table_id: u32,
        page_id: PageId,
    ) -> Result<[u8; PAGE_SIZE], DatabaseError> {
        let offset = page_id.0 as u64 * PAGE_SIZE as u64;

        let mut file = self.get_file_handle(table_id)?;
        let mut buffer = [0u8; PAGE_SIZE];

        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buffer).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                DatabaseError::Storage(format!(
                    "Read past EOF: page {} in table {}",
                    page_id.0, table_id
                ))
            } else {
                DatabaseError::Io(e.to_string())
            }
        })?;

        Ok(buffer)
    }

    pub fn write_page(
        &mut self,
        table_id: u32,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), DatabaseError> {
        let offset = page_id.0 as u64 * PAGE_SIZE as u64;

        let mut file = self.get_file_handle(table_id)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_all()?;

        Ok(())
    }

    pub fn allocate_page(&mut self, table_id: u32) -> Result<PageId, DatabaseError> {
        let mut file = self.get_file_handle(table_id)?;
        let file_len = file.metadata()?.len();

        if file_len % PAGE_SIZE as u64 != 0 {
            return Err(DatabaseError::Storage(format!(
                "Table file {} is not aligned to PAGE_SIZE (len: {})",
                table_id, file_len
            )));
        }

        let page_idx = (file_len / PAGE_SIZE as u64) as u32;
        let page_id = PageId(page_idx);

        let empty_page = [0u8; PAGE_SIZE];
        file.seek(SeekFrom::End(0))?;
        file.write_all(&empty_page)?;
        file.sync_all()?;

        Ok(page_id)
    }

    fn resolve_physical_location_path(&self, table_id: u32) -> PathBuf {
        self.data_dir.join(format!("{}.db", table_id))
    }

    fn get_file_handle(&mut self, table_id: u32) -> Result<&File, DatabaseError> {
        if !self.opened_files.contains_key(&table_id) {
            let path = self.resolve_physical_location_path(table_id);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            self.opened_files.insert(table_id, file);
        }
        Ok(self.opened_files.get(&table_id).unwrap())
    }
}
