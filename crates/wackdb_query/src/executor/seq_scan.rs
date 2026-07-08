use crate::{Executor, QueryError, get_record_from_bytes, get_total_slots_from_bytes};
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::DiskManager;
use wackdb_tuple::{Schema, Tuple};

/// Performs a sequential scan over all pages in a heap file.
pub struct SeqScan<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    file_id: u32,
    schema: Schema,
    current_page_num: u32,
    current_slot: u16,
    max_pages: u32,
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> SeqScan<'a, PAGE_SIZE, D> {
    /// Initializes a new sequential scan executor.
    pub fn new(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        file_id: u32,
        schema: Schema,
        max_pages: u32,
    ) -> Self {
        Self {
            buffer_pool,
            file_id,
            schema,
            current_page_num: 0,
            current_slot: 0,
            max_pages,
        }
    }
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> Executor for SeqScan<'_, PAGE_SIZE, D> {
    /// Retrieves the next tuple from the relation.
    ///
    /// # Errors
    /// Returns `QueryError` if disk or buffer pool errors occur during page fetches.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        while self.current_page_num <= self.max_pages {
            let page_id = wackdb_storage::PageId {
                file_id: self.file_id,
                page_num: self.current_page_num,
            };

            let Ok(frame_id) = self.buffer_pool.fetch_page(page_id) else {
                self.current_page_num += 1;
                self.current_slot = 0;
                continue;
            };

            let page_guard = self.buffer_pool.read_page(frame_id);
            let max_slots = get_total_slots_from_bytes(&page_guard.data.0) as u16;

            let mut found = None;
            while self.current_slot < max_slots {
                let slot = self.current_slot;
                self.current_slot += 1;

                if let Some(record) = get_record_from_bytes(&page_guard.data.0, slot as usize) {
                    found = Some(Tuple {
                        data: record.1.to_vec(),
                    });
                    break;
                }
            }

            drop(page_guard);
            let _ = self.buffer_pool.unpin_page(page_id, false);

            if let Some(tuple) = found {
                return Ok(Some(tuple));
            }

            self.current_page_num += 1;
            self.current_slot = 0;
        }
        Ok(None)
    }

    /// Returns the schema of the tuples produced by this scan.
    fn schema(&self) -> &Schema {
        &self.schema
    }
}
