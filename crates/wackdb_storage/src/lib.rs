//! Storage Engine for `WackDB`
//!
//! Manages disk operations, physical pages, and tuple identifiers.

#![warn(missing_docs)]

/// Disk manager for fetching and flushing pages to physical files.
pub mod disk_manager;
/// Error types for the storage module.
pub mod error;
/// Base types and identifiers used across the storage manager.
pub mod types;

pub use disk_manager::{BasicDiskManager, DiskManager, FileHandle};
pub use error::StorageError;
pub use types::{PageId, CTID};
