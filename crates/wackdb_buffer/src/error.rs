#![warn(missing_docs)]
use thiserror::Error;

/// Represents an error occurring within the Buffer Pool.
#[derive(Error, Debug)]
pub enum BufferError {
    /// No free frames are available in the buffer pool.
    #[error("No free frames available in the buffer pool")]
    NoFreeFrames,
    /// The requested page was not found in the buffer pool.
    #[error("Page not found in the buffer pool")]
    PageNotFound,
    /// An underlying storage error occurred.
    #[error("Storage error: {0}")]
    StorageError(#[from] wackdb_storage::StorageError),
}
