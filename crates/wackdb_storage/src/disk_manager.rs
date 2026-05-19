use crate::error::StorageError;
use crate::types::PageId;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Represents the physical layer that manages reads and writes to disk pages.
pub trait DiskManager<const PAGE_SIZE: usize>: Send + Sync {
    /// # Errors
    /// Returns error on I/O issues
    fn read_page(&self, page_id: PageId, data: &mut [u8; PAGE_SIZE]) -> Result<(), StorageError>;
    /// # Errors
    /// Returns error on I/O issues
    fn write_page(&self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), StorageError>;
    /// # Errors
    /// Returns error on I/O issues
    fn allocate_page(&self, file_id: u32) -> Result<PageId, StorageError>;
    /// Deallocates a physical page. Space reuse should be handled via a free space map.
    fn deallocate_page(&self, page_id: PageId);
}

/// Contains the file descriptor and tracked page count for an open database file.
pub struct FileHandle {
    /// Thread-safe file descriptor.
    pub fd: Mutex<File>,
    /// Thread-safe tracker of the total pages currently in the file.
    pub total_pages: AtomicU32,
}

/// A basic file-system backed disk manager.
pub struct BasicDiskManager<const PAGE_SIZE: usize> {
    data_dir: PathBuf,
    file_handles: Mutex<HashMap<u32, FileHandle>>,
}

impl<const PAGE_SIZE: usize> BasicDiskManager<PAGE_SIZE> {
    /// # Errors
    /// Returns error on I/O issues
    #[allow(clippy::cast_possible_truncation)]
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self, StorageError> {
        std::fs::create_dir_all(data_dir.as_ref())?;
        Ok(Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            file_handles: Mutex::new(HashMap::new()),
        })
    }

    fn ensure_file_open(&self, file_id: u32) -> Result<(), StorageError> {
        let mut handles = self.file_handles.lock();
        if let std::collections::hash_map::Entry::Vacant(e) = handles.entry(file_id) {
            let file_path = self.data_dir.join(file_id.to_string());
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&file_path)?;

            let metadata = file.metadata()?;
            #[allow(clippy::cast_possible_truncation)]
            let total_pages = (metadata.len() / (PAGE_SIZE as u64)) as u32;

            e.insert(FileHandle {
                fd: Mutex::new(file),
                total_pages: AtomicU32::new(total_pages),
            });
        }
        Ok(())
    }
}

impl<const PAGE_SIZE: usize> DiskManager<PAGE_SIZE> for BasicDiskManager<PAGE_SIZE> {
    /// # Errors
    /// Returns error on I/O issues
    fn read_page(&self, page_id: PageId, data: &mut [u8; PAGE_SIZE]) -> Result<(), StorageError> {
        self.ensure_file_open(page_id.file_id)?;
        let handles = self.file_handles.lock();
        let handle = handles
            .get(&page_id.file_id)
            .ok_or(StorageError::FileError(page_id.file_id))?;

        let mut file = handle.fd.lock();
        let offset = u64::from(page_id.page_num) * (PAGE_SIZE as u64);

        let file_len = file.metadata()?.len();
        if offset >= file_len {
            data.fill(0);
            return Ok(());
        }

        file.seek(SeekFrom::Start(offset))?;
        let mut temp_buf = vec![0; PAGE_SIZE];
        let bytes_read = file.read(&mut temp_buf)?;
        data.copy_from_slice(&temp_buf);
        if bytes_read < PAGE_SIZE {
            data[bytes_read..].fill(0);
        }

        Ok(())
    }

    /// # Errors
    /// Returns error on I/O issues
    fn write_page(&self, page_id: PageId, data: &[u8; PAGE_SIZE]) -> Result<(), StorageError> {
        self.ensure_file_open(page_id.file_id)?;
        let handles = self.file_handles.lock();
        let handle = handles
            .get(&page_id.file_id)
            .ok_or(StorageError::FileError(page_id.file_id))?;

        let mut file = handle.fd.lock();
        let offset = u64::from(page_id.page_num) * (PAGE_SIZE as u64);
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(data)?;
        file.sync_data()?;
        Ok(())
    }

    /// # Errors
    /// Returns error on I/O issues
    fn allocate_page(&self, file_id: u32) -> Result<PageId, StorageError> {
        self.ensure_file_open(file_id)?;
        let handles = self.file_handles.lock();
        let handle = handles
            .get(&file_id)
            .ok_or(StorageError::FileError(file_id))?;

        let page_num = handle.total_pages.fetch_add(1, Ordering::SeqCst);
        Ok(PageId { file_id, page_num })
    }

    fn deallocate_page(&self, _page_id: PageId) {
        // Space reuse is typically handled by a Free Space Map (FSM) in Postgres.
        // For simplicity in this educational DBMS, we leave it as a no-op initially.
    }
}

#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn allocate_should_return_valid_page_id() {
        let dir = tempdir().unwrap();
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();

        let page_id = disk_manager.allocate_page(100).unwrap();

        assert_eq!(page_id.file_id, 100);
        assert_eq!(page_id.page_num, 0);
    }

    #[test]
    fn read_write_should_persist_data() {
        let dir = tempdir().unwrap();
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let page_id = disk_manager.allocate_page(100).unwrap();

        let mut write_data = [0u8; TEST_PAGE_SIZE];
        write_data[0] = 42;
        write_data[TEST_PAGE_SIZE - 1] = 24;

        disk_manager.write_page(page_id, &write_data).unwrap();

        let mut read_data = [0u8; TEST_PAGE_SIZE];
        disk_manager.read_page(page_id, &mut read_data).unwrap();

        assert_eq!(read_data[0], 42);
        assert_eq!(read_data[TEST_PAGE_SIZE - 1], 24);
        assert_eq!(read_data[1..TEST_PAGE_SIZE - 1], [0u8; TEST_PAGE_SIZE - 2]);
    }
}
