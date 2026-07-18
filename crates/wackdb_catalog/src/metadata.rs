use serde::{Deserialize, Serialize};
use wackdb_tuple::Schema;

/// Minimal metadata for a single table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Logical table name
    pub name: String,
    /// The physical file ID for heap data.
    pub heap_relation_id: u32,
    /// The physical file ID for the B+Tree index.
    pub index_relation_id: u32,
    /// Root page number within this relation's file, if initialized
    pub root_page_num: Option<u32>,
    /// Schema of the table
    pub schema: Schema,
    /// Number of records in the table
    #[serde(default)]
    pub num_records: usize,
}
