use std::io;
use thiserror::Error;

/// Represents an error occurring during physical storage operations.
#[derive(Error, Debug)]
pub enum StorageError {
    /// A lower-level OS or filesystem I/O error.
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    /// Attempted to read or write a page that does not exist in the file.
    #[error("Page {page_num} out of bounds in file {file_id}")]
    OutOfBounds {
        /// The file ID.
        file_id: u32,
        /// The invalid page number.
        page_num: u32,
    },
    /// The specified file could not be created or opened.
    #[error("File ID {0} could not be created/opened")]
    FileError(u32),
}
