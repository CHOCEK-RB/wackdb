use crate::executor::{IndexScan, SeqScan};
use crate::{Executor, QueryError};
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::DiskManager;
use wackdb_tuple::Schema;

/// Query Optimizer that selects the best execution plan.
pub struct Optimizer;

impl Optimizer {
    /// Dynamically optimizes and constructs a scan executor.
    /// Uses `IndexScan` if index metadata is available, otherwise falls back to `SeqScan`.
    ///
    /// # Errors
    /// Returns `QueryError` if the chosen executor fails to initialize.
    pub fn optimize<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        file_id: u32,
        schema: Schema,
        max_pages: u32,
        index_opt: Option<(&BTreeIndex<'a, PAGE_SIZE, D>, i32, i32)>,
    ) -> Result<Box<dyn Executor + 'a>, QueryError> {
        if let Some((index, start_key, end_key)) = index_opt {
            let index_scan = IndexScan::new(buffer_pool, schema, index, start_key, end_key)?;
            Ok(Box::new(index_scan))
        } else {
            let seq_scan = SeqScan::new(buffer_pool, file_id, schema, max_pages);
            Ok(Box::new(seq_scan))
        }
    }
}
