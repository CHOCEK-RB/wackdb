use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize)]
pub enum DatabaseError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Page corrupted: ID {page_id}, checksum mismatch")]
    PageCorrupted { page_id: u32 },

    #[error("Storage error: {0}")]
    Storage(String),
}

impl From<io::Error> for DatabaseError {
    fn from(err: io::Error) -> Self {
        DatabaseError::Io(err.to_string())
    }
}
