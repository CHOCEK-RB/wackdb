use thiserror::Error;

/// Catalog errors
#[derive(Error, Debug)]
pub enum CatalogError {
    /// IO Error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Table already exists
    #[error("Table '{0}' already exists")]
    TableExists(String),
    /// Table not found
    #[error("Table '{0}' not found")]
    TableNotFound(String),
}
