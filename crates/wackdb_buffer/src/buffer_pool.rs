use crate::frame::FrameDescriptor;
use crate::lru::LRUReplacer;
use crate::replacer::ReplacementPolicy;
use crate::BufferError;
use crate::LogManager;
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wackdb_page::SlottedPage;
use wackdb_storage::{DiskManager, PageId};

/// `BufferPoolManager` orchestrates the fetching and flushing of pages to disk.
pub struct BufferPoolManager<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    _pool_size: usize,
    /// Physical memory pages.
    frames: Vec<RwLock<SlottedPage<PAGE_SIZE>>>,
    /// Metadata for each frame.
    descriptors: Vec<FrameDescriptor>,
    /// Page mapping table to find which frame holds a page.
    page_table: Mutex<HashMap<PageId, usize>>,
    /// Disk manager for fetching/flushing.
    disk_manager: D,
    /// Page replacement policy.
    replacer: LRUReplacer,

    // Metrics for the Performance Report
    hits: AtomicUsize,
    misses: AtomicUsize,

    // Optional WAL Manager for crash recovery
    log_manager: Option<Arc<dyn LogManager>>,
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BufferPoolManager<PAGE_SIZE, D> {
    /// Creates a new Buffer Pool Manager.
    pub fn new(pool_size: usize, disk_manager: D) -> Self {
        Self::new_with_log_manager(pool_size, disk_manager, None)
    }

    /// Creates a new Buffer Pool Manager with an optional Log Manager injected.
    pub fn new_with_log_manager(
        pool_size: usize,
        disk_manager: D,
        log_manager: impl Into<Option<Arc<dyn LogManager>>>,
    ) -> Self {
        let mut frames = Vec::with_capacity(pool_size);
        let mut descriptors = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            frames.push(RwLock::new(SlottedPage::<PAGE_SIZE>::new()));
            descriptors.push(FrameDescriptor::new());
        }

        Self {
            _pool_size: pool_size,
            frames,
            descriptors,
            page_table: Mutex::new(HashMap::with_capacity(pool_size)),
            disk_manager,
            replacer: LRUReplacer::new(pool_size),

            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
            log_manager: log_manager.into(),
        }
    }

    /// Returns a clone of the `LogManager` ARC if present.
    pub fn get_log_manager(&self) -> Option<std::sync::Arc<dyn LogManager>> {
        self.log_manager.clone()
    }

    /// Returns the cache hit rate as a float between 0.0 and 1.0.
    pub fn get_hit_rate(&self) -> f64 {
        let h = self.hits.load(Ordering::SeqCst) as f64;
        let m = self.misses.load(Ordering::SeqCst) as f64;
        if h + m == 0.0 {
            0.0
        } else {
            h / (h + m)
        }
    }

    /// Returns the total number of cache hits.
    pub fn get_hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    /// Returns the total number of cache misses.
    pub fn get_misses(&self) -> usize {
        self.misses.load(Ordering::SeqCst)
    }

    /// Returns metadata about all frames for debugging or visualization.
    pub fn get_frames_metadata(&self) -> Vec<(usize, Option<PageId>, usize, bool)> {
        self.descriptors
            .iter()
            .enumerate()
            .map(|(i, desc)| {
                let pid = *desc.page_id.lock();
                let pins = desc.pin_count.load(Ordering::SeqCst) as usize;
                let dirty = desc.is_dirty.load(Ordering::SeqCst);
                (i, pid, pins, dirty)
            })
            .collect()
    }

    /// Finds a free frame, either by finding an empty one or evicting a victim.
    fn find_victim_frame(&self) -> Result<usize, BufferError> {
        // look for an empty frame
        for (i, desc) in self.descriptors.iter().enumerate() {
            if desc.page_id.lock().is_none() {
                return Ok(i);
            }
        }

        // Use replacement policy
        self.replacer.evict().ok_or(BufferError::NoFreeFrames)
    }

    /// Fetches a page from the buffer pool. If it doesn't exist, reads from disk.
    /// The returned frame is already pinned.
    ///
    /// # Errors
    /// Returns a `BufferError` if no free frames are available or disk I/O fails.
    pub fn fetch_page(&self, page_id: PageId) -> Result<usize, BufferError> {
        let mut page_table = self.page_table.lock();

        if let Some(&frame_id) = page_table.get(&page_id) {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let desc = &self.descriptors[frame_id];
            desc.pin();
            self.replacer.record_access(frame_id);
            self.replacer.set_pin(frame_id, true);

            return Ok(frame_id);
        }

        self.misses.fetch_add(1, Ordering::SeqCst);

        // Cache miss. Need to find a free frame.
        let frame_id = self.find_victim_frame()?;

        // If the frame has a dirty page, flush it (enforcing WAL Before Data).
        self.flush_frame_to_disk(frame_id)?;

        let desc = &self.descriptors[frame_id];

        {
            let mut page_id_guard = desc.page_id.lock();
            if let Some(old_page_id) = *page_id_guard {
                page_table.remove(&old_page_id);
            }
            *page_id_guard = Some(page_id);
        }
        page_table.insert(page_id, frame_id);

        {
            let mut page_data = self.frames[frame_id].write();
            self.disk_manager.read_page(page_id, &mut page_data.data)?;
        }

        // Setup descriptor
        desc.pin();
        self.replacer.record_access(frame_id);
        self.replacer.set_pin(frame_id, true);

        Ok(frame_id)
    }

    /// Unpins a page. Marks it as dirty if `is_dirty` is true.
    ///
    /// # Errors
    /// Returns a `BufferError` if the page is not currently in the buffer pool.
    pub fn unpin_page(&self, page_id: PageId, is_dirty: bool) -> Result<(), BufferError> {
        let page_table = self.page_table.lock();
        if let Some(&frame_id) = page_table.get(&page_id) {
            let desc = &self.descriptors[frame_id];
            if is_dirty {
                desc.is_dirty.store(true, Ordering::SeqCst);
            }
            desc.unpin();
            if desc.pin_count.load(Ordering::SeqCst) == 0 {
                self.replacer.set_pin(frame_id, false);
            }

            Ok(())
        } else {
            Err(BufferError::PageNotFound)
        }
    }

    /// Flushes a specific page to disk.
    ///
    /// # Errors
    /// Returns a `BufferError` if the page is not found or a disk write fails.
    pub fn flush_page(&self, page_id: PageId) -> Result<(), BufferError> {
        let page_table = self.page_table.lock();
        if let Some(&frame_id) = page_table.get(&page_id) {
            self.flush_frame_to_disk(frame_id)
        } else {
            Err(BufferError::PageNotFound)
        }
    }

    /// Flushes all dirty pages to disk.
    ///
    /// # Errors
    /// Returns a `BufferError` if any disk write fails.
    pub fn flush_all_pages(&self) -> Result<(), BufferError> {
        let _page_table = self.page_table.lock(); // Keep lock to avoid simultaneous evictions
        for frame_id in 0..self.descriptors.len() {
            self.flush_frame_to_disk(frame_id)?;
        }
        Ok(())
    }

    /// Internal helper that strictly enforces 'WAL Before Data' before flushing.
    fn flush_frame_to_disk(&self, frame_id: usize) -> Result<(), BufferError> {
        let desc = &self.descriptors[frame_id];

        if desc.is_dirty.load(Ordering::SeqCst) {
            if let Some(page_id) = *desc.page_id.lock() {
                let page_read_guard = self.frames[frame_id].read();

                // CRITICAL: WAL Before Data protocol enforcement
                if let Some(ref log_mgr) = self.log_manager {
                    let page_lsn = page_read_guard.get_lsn();
                    log_mgr.flush_up_to(page_lsn)?;
                }

                // Proceed with flushing the dirty page to permanent storage
                self.disk_manager
                    .write_page(page_id, &page_read_guard.data)?;
                desc.is_dirty.store(false, Ordering::SeqCst);
            }
        }
        Ok(())
    }

    /// Returns the total number of pages allocated for the given file ID.
    ///
    /// # Errors
    /// Returns `BufferError::PageNotFound` if the disk manager returns an error.
    pub fn get_total_pages(&self, file_id: u32) -> Result<u32, BufferError> {
        self.disk_manager
            .get_total_pages(file_id)
            .map_err(|_| BufferError::PageNotFound)
    }

    /// Provides read access to the actual slotted page.
    pub fn read_page(&self, frame_id: usize) -> RwLockReadGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].read()
    }

    /// Provides write access to the actual slotted page.
    pub fn write_page(&self, frame_id: usize) -> RwLockWriteGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].write()
    }

    /// Access the underlying disk manager
    pub fn disk_manager(&self) -> &D {
        &self.disk_manager
    }

    /// Allocates a new page on disk and brings it into the buffer pool.
    ///
    /// # Errors
    /// Returns a `BufferError` if no free frames are available or disk allocation fails.
    pub fn new_page(&self, file_id: u32) -> Result<(usize, PageId), BufferError> {
        let page_id = self.disk_manager.allocate_page(file_id)?;
        let frame_id = self.fetch_page(page_id)?;
        Ok((frame_id, page_id))
    }

    /// Drops all pages associated with a specific relation (`file_id`) from the buffer pool
    /// without flushing them to disk. Used when a table is dropped.
    pub fn drop_relation(&self, file_id: u32) {
        let mut page_table = self.page_table.lock();
        for frame_id in 0..self.descriptors.len() {
            let desc = &self.descriptors[frame_id];
            let mut page_id_guard = desc.page_id.lock();
            if let Some(pid) = *page_id_guard {
                if pid.file_id == file_id {
                    // Invalidate without flushing
                    page_table.remove(&pid);
                    *page_id_guard = None;
                    desc.is_dirty.store(false, Ordering::SeqCst);
                    desc.pin_count.store(0, Ordering::SeqCst);
                    self.replacer.set_pin(frame_id, false);
                }
            }
        }
    }
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> Drop for BufferPoolManager<PAGE_SIZE, D> {
    fn drop(&mut self) {
        // Automatically flush all dirty pages when the system shuts down or the buffer pool is dropped.
        let _ = self.flush_all_pages();
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wackdb_storage::BasicDiskManager;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn test_buffer_pool_evicts_lru() {
        let dir = tempdir().unwrap();
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let pool = BufferPoolManager::new(3, disk_manager);

        // Fetch 3 pages, pool is full
        let page0 = pool.new_page(1).unwrap().1; // frame 0
        let page1 = pool.new_page(1).unwrap().1; // frame 1
        let page2 = pool.new_page(1).unwrap().1; // frame 2

        pool.unpin_page(page0, false).unwrap();
        pool.unpin_page(page1, false).unwrap();
        pool.unpin_page(page2, false).unwrap();

        // Fetch a 4th page, should evict page0
        let page3 = pool.new_page(1).unwrap().1; // Reuses frame 0

        pool.unpin_page(page3, false).unwrap();

        // Fetch page0 again, should evict page1
        let page0_new = pool.fetch_page(page0).unwrap(); // Reuses frame 1
        pool.unpin_page(page0, false).unwrap();

        assert_eq!(page0_new, 1);
    }

    #[test]
    fn test_buffer_pool_respects_pins() {
        let dir = tempdir().unwrap();
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let pool = BufferPoolManager::new(2, disk_manager);

        let _page0 = pool.new_page(1).unwrap().1; // pinned
        let _page1 = pool.new_page(1).unwrap().1; // pinned

        // Pool is full and all pinned, fetching 3rd should fail
        let res = pool.new_page(1);
        assert!(matches!(res, Err(BufferError::NoFreeFrames)));
    }

    #[test]
    fn test_buffer_pool_wal_before_data_enforcement() {
        use crate::BufferError;
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;

        struct MockLogManager {
            highest_flushed_lsn: AtomicU64,
            flush_called: AtomicBool,
        }
        impl crate::LogManager for MockLogManager {
            fn flush_up_to(&self, lsn: u64) -> Result<(), BufferError> {
                self.highest_flushed_lsn.fetch_max(lsn, Ordering::SeqCst);
                self.flush_called.store(true, Ordering::SeqCst);
                Ok(())
            }

            fn append_record(&self, _payload: &[u8]) -> Result<u64, BufferError> {
                Ok(1)
            }
            fn checkpoint(&self) -> Result<(), BufferError> {
                Ok(())
            }
            fn log_size(&self) -> u64 {
                0
            }
        }

        struct MockDiskManager {
            write_called_after_flush: AtomicBool,
            log_manager: Arc<MockLogManager>,
        }
        impl wackdb_storage::DiskManager<TEST_PAGE_SIZE> for MockDiskManager {
            fn read_page(
                &self,
                _page_id: PageId,
                _page_data: &mut [u8; TEST_PAGE_SIZE],
            ) -> Result<(), wackdb_storage::StorageError> {
                Ok(())
            }
            fn write_page(
                &self,
                _page_id: PageId,
                _page_data: &[u8; TEST_PAGE_SIZE],
            ) -> Result<(), wackdb_storage::StorageError> {
                let flushed = self.log_manager.flush_called.load(Ordering::SeqCst);
                self.write_called_after_flush
                    .store(flushed, Ordering::SeqCst);
                Ok(())
            }
            fn allocate_page(&self, _file_id: u32) -> Result<PageId, wackdb_storage::StorageError> {
                Ok(PageId {
                    file_id: 1,
                    page_num: 1,
                })
            }
            fn deallocate_page(&self, _page_id: wackdb_storage::PageId) {}
            fn delete_file(&self, _file_id: u32) -> Result<(), wackdb_storage::StorageError> {
                Ok(())
            }
            fn close_all(&self) {}
            fn get_total_pages(&self, _file_id: u32) -> Result<u32, wackdb_storage::StorageError> {
                Ok(0)
            }
        }

        let log_manager = Arc::new(MockLogManager {
            highest_flushed_lsn: AtomicU64::new(0),
            flush_called: AtomicBool::new(false),
        });

        let disk_manager = MockDiskManager {
            write_called_after_flush: AtomicBool::new(false),
            log_manager: log_manager.clone(),
        };

        let dyn_log_manager: Arc<dyn crate::LogManager> = log_manager.clone();
        let pool = BufferPoolManager::new_with_log_manager(2, disk_manager, Some(dyn_log_manager));
        let frame_id = pool.new_page(1).unwrap().0;

        {
            let mut page_write = pool.write_page(frame_id);
            page_write.set_lsn(100);
        }

        pool.unpin_page(
            PageId {
                file_id: 1,
                page_num: 1,
            },
            true,
        )
        .unwrap();
        pool.flush_page(PageId {
            file_id: 1,
            page_num: 1,
        })
        .unwrap();

        assert_eq!(
            log_manager.highest_flushed_lsn.load(Ordering::SeqCst),
            100,
            "log manager must be flushed up to page LSN"
        );
        assert!(
            pool.disk_manager
                .write_called_after_flush
                .load(Ordering::SeqCst),
            "disk write occurred before WAL flush"
        );
    }

    #[test]
    fn test_buffer_pool_metrics_atomic_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let disk_manager = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let pool = BufferPoolManager::new(2, disk_manager);

        let page0 = pool.new_page(1).unwrap().1;
        pool.unpin_page(page0, false).unwrap();

        pool.fetch_page(page0).unwrap();
        pool.unpin_page(page0, false).unwrap();

        let hits = pool.get_hits();
        let misses = pool.get_misses();

        assert_eq!(hits, 1, "expected exactly 1 cache hit");
        assert!(misses >= 1, "expected at least 1 cache miss");
        let rate = pool.get_hit_rate();
        assert!(rate > 0.0 && rate < 1.0, "hit rate must be tracked");
    }
}
