#![warn(missing_docs)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::type_complexity)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::cast_possible_truncation)]

//! Query Engine for `WackDB` implementing Volcano-style execution.

/// The executor module contains physical operators.
pub mod executor;

use serde_json::{Map, Value as JsonValue};
use std::mem::size_of;
use thiserror::Error;
use wackdb_page::header::{SlottedPageHeader, TupleHeader};
use wackdb_page::slot::PageSlot;
use wackdb_tuple::{Schema, Tuple, value::Value};

pub use executor::{
    ExternalMergeSort, IndexScan, NestedLoopJoin, Optimizer, Project, Select, SeqScan,
};

/// Retrieves a tuple record slice from raw page bytes given a slot index.
///
/// Returns a tuple containing the `TupleHeader` and a slice to the payload bytes.
#[must_use]
pub fn get_record_from_bytes(data: &[u8], slot_idx: usize) -> Option<(TupleHeader, &[u8])> {
    let header = unsafe { &*data.as_ptr().cast::<SlottedPageHeader>() };
    let total_slots = header.total_slots as usize;
    if slot_idx >= total_slots {
        return None;
    }

    let slots = unsafe {
        std::slice::from_raw_parts(
            data[size_of::<SlottedPageHeader>()..]
                .as_ptr()
                .cast::<PageSlot>(),
            total_slots,
        )
    };

    let slot = &slots[slot_idx];
    if slot.length == 0 {
        return None;
    }

    let start = slot.offset as usize;
    let header_end = start + size_of::<TupleHeader>();
    let end = start + slot.length as usize;

    let tuple_header =
        unsafe { std::ptr::read_unaligned(data[start..].as_ptr().cast::<TupleHeader>()) };
    if tuple_header.xmax != 0 {
        return None;
    }
    let record_data = &data[header_end..end];

    Some((tuple_header, record_data))
}

/// Retrieves the total number of slots allocated in the given page data block.
#[must_use]
pub fn get_total_slots_from_bytes(data: &[u8]) -> usize {
    let header = unsafe { &*data.as_ptr().cast::<SlottedPageHeader>() };
    header.total_slots as usize
}

/// Errors returned by the query engine execution pipeline.
#[derive(Error, Debug)]
pub enum QueryError {
    /// Execution logic failure.
    #[error("Execution failed: {0}")]
    Execution(String),
    /// Error originating from tuple parsing or encoding.
    #[error("Tuple error: {0}")]
    Tuple(#[from] wackdb_tuple::tuple::TupleError),
    /// Error during buffer pool interactions.
    #[error("Buffer error: {0}")]
    Buffer(#[from] wackdb_buffer::BufferError),
    /// Error during index interactions.
    #[error("Index error: {0}")]
    Index(#[from] wackdb_btree::node::BTreeError),
    /// Standard I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Core interface for Volcano-style physical operators.
pub trait Executor {
    /// Retrieves the next tuple from this operator.
    ///
    /// # Errors
    /// Returns a `QueryError` if execution fails.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError>;
    /// Retrieves the schema of the tuples returned by this operator.
    fn schema(&self) -> &Schema;
}

impl Executor for Box<dyn Executor + '_> {
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        self.as_mut().next()
    }
    fn schema(&self) -> &Schema {
        self.as_ref().schema()
    }
}

/// Converts a `Tuple` instance to a standard JSON object using its `Schema`.
///
/// # Errors
/// Returns `QueryError` if the tuple data fails to deserialize against the schema.
pub fn tuple_to_json(tuple: &Tuple, schema: &Schema) -> Result<JsonValue, QueryError> {
    let values = tuple.to_values(schema)?;
    let mut map = Map::new();
    for (col, val) in schema.columns.iter().zip(values.iter()) {
        let json_val = match val {
            Value::Integer(v) => JsonValue::Number(serde_json::Number::from(*v)),
            Value::Boolean(v) => JsonValue::Bool(*v),
            Value::Varchar(s) => JsonValue::String(s.clone()),
            Value::Null => JsonValue::Null,
        };
        map.insert(col.name.clone(), json_val);
    }
    Ok(JsonValue::Object(map))
}

#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::ptr_as_ptr
)]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wackdb_buffer::buffer_pool::BufferPoolManager;
    use wackdb_storage::disk_manager::BasicDiskManager;
    use wackdb_tuple::schema::Column;
    use wackdb_tuple::value::DataType;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn test_query_engine_iterators() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk);

        let schema = Schema::new(vec![
            Column::new("id", DataType::Integer, false),
            Column::new("val", DataType::Varchar, false),
        ]);

        let file_id = 1;
        let (frame_id, page_id) = buffer.new_page(file_id).unwrap();
        let mut page_write = buffer.write_page(frame_id);
        let mut slotted_page = wackdb_page::SlottedPage::<TEST_PAGE_SIZE>::new();

        let t1 = Tuple::from_values(&schema, &[Value::Integer(1), Value::Varchar("one".into())])
            .unwrap();
        let t2 = Tuple::from_values(&schema, &[Value::Integer(2), Value::Varchar("two".into())])
            .unwrap();
        let t3 = Tuple::from_values(
            &schema,
            &[Value::Integer(3), Value::Varchar("three".into())],
        )
        .unwrap();

        slotted_page.insert_record(&t2.data, 2).unwrap();
        slotted_page.insert_record(&t3.data, 3).unwrap();
        slotted_page.insert_record(&t1.data, 1).unwrap();

        page_write.data.copy_from_slice(&slotted_page.data.0);

        drop(page_write);
        buffer.unpin_page(page_id, true).unwrap();

        let scan = SeqScan::new(&buffer, file_id, schema, 1);
        let sort = ExternalMergeSort::new(scan, 0, 1000);
        let select = Select::new(
            sort,
            Box::new(|t, s| {
                let vals = t.to_values(s).unwrap();
                matches!(vals[0], Value::Integer(v) if v > 1)
            }),
        );
        let out_schema = Schema::new(vec![Column::new("val", DataType::Varchar, false)]);
        let mut project = Project::new(select, out_schema, vec![1]);

        let mut results = Vec::new();
        while let Some(t) = project.next().unwrap() {
            results.push(t);
        }
        assert_eq!(results.len(), 2);
    }
}
