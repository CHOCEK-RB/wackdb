use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use wackdb_common::config::Config;
use wackdb_common::constants::PAGE_SIZE;
use wackdb_common::errors::DatabaseError;
use wackdb_common::types::PageId;

/// DiskManager manages the physical persistence of fixed-size 8KB pages.
///
/// It provides basic I/O operations and guarantees atomicity during page updates
/// by employing a temporary file swap (Shadow Paging) strategy combined with
/// explicit filesystem synchronization.
pub struct DiskManager {
    data_dir: PathBuf,
    opened_files: HashMap<u32, File>,
}

impl DiskManager {
    /// Initializes the DiskManager and ensures the data directory exists.
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

    /// Creates a new database file for the specified table ID.
    pub fn create_file(&mut self, table_id: u32) -> Result<(), DatabaseError> {
        let path = self.resolve_physical_location_path(table_id);
        if path.exists() {
            return Err(DatabaseError::Storage(format!(
                "File for table {} already exists",
                table_id
            )));
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;

        file.sync_all()?;
        self.opened_files.insert(table_id, file);
        Ok(())
    }

    /// Reads an 8KB page from disk.
    pub fn read_page(&mut self, page_id: PageId) -> Result<[u8; PAGE_SIZE], DatabaseError> {
        let table_id = page_id.table_id();
        let offset = page_id.page_idx() as u64 * PAGE_SIZE as u64;

        let mut file = self.get_file_handle(table_id)?;
        let mut buffer = [0u8; PAGE_SIZE];

        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buffer).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                DatabaseError::Storage(format!(
                    "Read past EOF: page {} in table {}",
                    page_id.page_idx(),
                    table_id
                ))
            } else {
                DatabaseError::Io(e.to_string())
            }
        })?;

        Ok(buffer)
    }

    /// Writes a page to disk using in-place overwrite followed by fsync.
    pub fn write_page(
        &mut self,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), DatabaseError> {
        let table_id = page_id.table_id();
        let offset = page_id.page_idx() as u64 * PAGE_SIZE as u64;

        let mut file = self.get_file_handle(table_id)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_all()?;

        Ok(())
    }

    /// Performs an atomic write by writing to a temporary file and renaming it.
    ///
    /// This pattern ensures that a crash during the write operation does not
    /// leave the original file in a corrupted or "torn" state.
    pub fn safe_write_page(
        &mut self,
        page_id: PageId,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), DatabaseError> {
        let table_id = page_id.table_id();
        let offset = page_id.page_idx() as u64 * PAGE_SIZE as u64;
        self.atomic_write(table_id, offset, data)
    }

    /// Appends a new 8KB page to the end of the specified table file.
    pub fn allocate_page(&mut self, table_id: u32) -> Result<PageId, DatabaseError> {
        let file = self.get_file_handle(table_id)?;
        let file_len = file.metadata()?.len();

        if !file_len.is_multiple_of(PAGE_SIZE as u64) {
            return Err(DatabaseError::Storage(format!(
                "Table file {} is not aligned to PAGE_SIZE (len: {})",
                table_id, file_len
            )));
        }

        let page_idx = (file_len / PAGE_SIZE as u64) as u32;
        let page_id = PageId::new(table_id, page_idx);

        let empty_page = [0u8; PAGE_SIZE];
        self.atomic_write(table_id, file_len, &empty_page)?;

        Ok(page_id)
    }

    /// Internal atomic write mechanism implementing Temp-Flush-Rename.
    fn atomic_write(
        &mut self,
        table_id: u32,
        offset: u64,
        data: &[u8],
    ) -> Result<(), DatabaseError> {
        let path = self.resolve_physical_location_path(table_id);
        let temp_path = path.with_extension("tmp");

        if path.exists() {
            fs::copy(&path, &temp_path)?;
        }

        {
            let mut temp_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&temp_path)?;

            temp_file.seek(SeekFrom::Start(offset))?;
            temp_file.write_all(data)?;
            temp_file.sync_all()?;
        }

        fs::rename(&temp_path, &path)?;

        if let Some(parent) = path.parent() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }

        // Drop the cached handle as the filesystem inode has changed.
        self.opened_files.remove(&table_id);

        Ok(())
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
                .truncate(false)
                .open(path)?;
            self.opened_files.insert(table_id, file);
        }
        Ok(self.opened_files.get(&table_id).unwrap())
    }
}
