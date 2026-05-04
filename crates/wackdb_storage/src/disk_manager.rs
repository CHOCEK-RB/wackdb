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
            fs::create_dir_all(&data_dir)?;
        }
        Ok(Self {
            data_dir,
            opened_files: HashMap::new(),
        })
    }

    pub fn create_file(&mut self, table_id: u32) -> Result<(), DatabaseError> {
        let path = self.resolve_path(table_id);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.sync_all()?;
        self.opened_files.insert(table_id, file);
        Ok(())
    }

    pub fn read_page(&mut self, page_id: PageId) -> Result<[u8; PAGE_SIZE], DatabaseError> {
        let mut file = self.get_file(page_id.table_id())?;
        let mut buffer = [0u8; PAGE_SIZE];
        file.seek(SeekFrom::Start(
            page_id.page_idx() as u64 * PAGE_SIZE as u64,
        ))?;
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    pub fn safe_write_page(
        &mut self,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), DatabaseError> {
        let path = self.resolve_path(page_id.table_id());
        let temp_path = path.with_extension("tmp");
        let offset = page_id.page_idx() as u64 * PAGE_SIZE as u64;

        if path.exists() {
            fs::copy(&path, &temp_path)?;
        }

        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&temp_path)?;
            tmp.seek(SeekFrom::Start(offset))?;
            tmp.write_all(data)?;
            tmp.sync_all()?;
        }

        fs::rename(&temp_path, &path)?;
        if let Some(p) = path.parent() {
            File::open(p)?.sync_all()?;
        }
        self.opened_files.remove(&page_id.table_id());
        Ok(())
    }

    fn resolve_path(&self, table_id: u32) -> PathBuf {
        self.data_dir.join(format!("{}.db", table_id))
    }

    fn get_file(&mut self, table_id: u32) -> Result<&File, DatabaseError> {
        if !self.opened_files.contains_key(&table_id) {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(self.resolve_path(table_id))?;
            self.opened_files.insert(table_id, file);
        }
        Ok(self.opened_files.get(&table_id).unwrap())
    }
}
