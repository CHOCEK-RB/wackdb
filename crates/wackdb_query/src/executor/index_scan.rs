use crate::{Executor, QueryError, get_record_from_bytes};
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::{CTID, DiskManager};
use wackdb_tuple::{Schema, Tuple};

/// Performs an index scan using a B+Tree.
pub struct IndexScan<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    schema: Schema,
    ctids: std::vec::IntoIter<CTID>,
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> IndexScan<'a, PAGE_SIZE, D> {
    /// Initializes a new index scan executor over a specific key range.
    ///
    /// # Errors
    /// Returns a `QueryError` if the B+Tree search fails.
    pub fn new(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        schema: Schema,
        index: &BTreeIndex<'a, PAGE_SIZE, D>,
        start_key: i32,
        end_key: i32,
    ) -> Result<Self, QueryError> {
        use wackdb_btree::traits::Index;
        let ctids = index.range_search(start_key, end_key)?;
        Ok(Self {
            buffer_pool,
            schema,
            ctids: ctids.into_iter(),
        })
    }
}

impl<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> Executor for IndexScan<'_, PAGE_SIZE, D> {
    /// Retrieves the next tuple from the index scan.
    ///
    /// # Errors
    /// Returns `QueryError` if fetching the underlying page or parsing the tuple fails.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        if let Some(ctid) = self.ctids.next() {
            let frame_id = self.buffer_pool.fetch_page(ctid.page_id)?;
            let page_guard = self.buffer_pool.read_page(frame_id);

            let record = get_record_from_bytes(&page_guard.data.0, ctid.slot_idx as usize)
                .ok_or_else(|| QueryError::Execution("CTID not found".into()))?;

            let tuple = Tuple {
                data: record.1.to_vec(),
            };

            drop(page_guard);
            let _ = self.buffer_pool.unpin_page(ctid.page_id, false);

            return Ok(Some(tuple));
        }
        Ok(None)
    }

    /// Returns the schema of the tuples produced by this scan.
    fn schema(&self) -> &Schema {
        &self.schema
    }
}
