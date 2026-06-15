#![allow(missing_docs)]
use crate::node::BTreeError;
use wackdb_storage::CTID;

/// Trait for defining the pluggable indexing strategy (`BTree`, Hash, etc.)
pub trait Index {
    /// Insert a key pointing to a CTID
    /// # Errors
    /// Returns `BTreeError` if insertion fails.
    fn insert(&mut self, key: i32, ctid: CTID) -> Result<(), BTreeError>;

    /// Exact match search
    /// # Errors
    /// Returns `BTreeError` if the key is not found.
    fn search(&self, key: i32) -> Result<CTID, BTreeError>;

    /// Range search for range queries
    /// # Errors
    /// Returns `BTreeError` on underlying storage or logic errors.
    fn range_search(&self, start_key: i32, end_key: i32) -> Result<Vec<CTID>, BTreeError>;

    /// Delete a key
    /// # Errors
    /// Returns `BTreeError` if the key is not found.
    fn delete(&mut self, key: i32) -> Result<(), BTreeError>;
}
