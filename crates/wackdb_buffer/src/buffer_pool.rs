use crate::frame::FrameDescriptor;
use crate::BufferError;
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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

    // Metrics
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BufferPoolManager<PAGE_SIZE, D> {
    /// Creates a new Buffer Pool Manager.
    pub fn new(pool_size: usize, disk_manager: D) -> Self {
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

            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
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

    /// Prints the physical state of the memory frames to standard output.
    pub fn print_buffer_state(&self) {
        println!("+--------------+------------+-----------+-----------+");
        println!("| Memory Frame | Page ID    | Pin Count | Is Dirty? |");
        println!("+--------------+------------+-----------+-----------+");
        for (i, desc) in self.descriptors.iter().enumerate() {
            let pid = match *desc.page_id.lock() {
                Some(p) => format!("{}:{}", p.file_id, p.page_num),
                None => "Empty".to_string(),
            };
            let pins = desc.pin_count.load(Ordering::SeqCst);
            let dirty = desc.is_dirty.load(Ordering::SeqCst);
            println!("| Frame {i:<6} | {pid:<10} | {pins:<9} | {dirty:<9} |");
        }
        println!("+--------------+------------+-----------+-----------+");
    }

    /// Finds a free frame, either by finding an empty one or evicting a victim.
    fn find_victim_frame(&self) -> Result<usize, BufferError> {
        // look for an empty frame
        for (i, desc) in self.descriptors.iter().enumerate() {
            if desc.page_id.lock().is_none() {
                return Ok(i);
            }
        }

        // Without a replacement policy
        Err(BufferError::NoFreeFrames)
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

            return Ok(frame_id);
        }

        self.misses.fetch_add(1, Ordering::SeqCst);

        // Cache miss. Need to find a free frame.
        let frame_id = self.find_victim_frame()?;
        let desc = &self.descriptors[frame_id];

        // If the frame has a dirty page, flush it.
        if desc.is_dirty.load(Ordering::SeqCst) {
            let page_id_guard = desc.page_id.lock();
            if let Some(old_page_id) = *page_id_guard {
                let page_data = self.frames[frame_id].read();
                self.disk_manager.write_page(old_page_id, &page_data.data)?;
                desc.is_dirty.store(false, Ordering::SeqCst);
            }
        }

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
            let desc = &self.descriptors[frame_id];
            let page_data = self.frames[frame_id].read();
            self.disk_manager.write_page(page_id, &page_data.data)?;
            desc.is_dirty.store(false, Ordering::SeqCst);
            Ok(())
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
        for (frame_id, desc) in self.descriptors.iter().enumerate() {
            if desc.is_dirty.load(Ordering::SeqCst) {
                if let Some(page_id) = *desc.page_id.lock() {
                    let page_data = self.frames[frame_id].read();
                    self.disk_manager.write_page(page_id, &page_data.data)?;
                    desc.is_dirty.store(false, Ordering::SeqCst);
                }
            }
        }
        Ok(())
    }

    /// Provides read access to the actual slotted page.
    pub fn read_page(&self, frame_id: usize) -> RwLockReadGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].read()
    }

    /// Provides write access to the actual slotted page.
    pub fn write_page(&self, frame_id: usize) -> RwLockWriteGuard<'_, SlottedPage<PAGE_SIZE>> {
        self.frames[frame_id].write()
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
}
