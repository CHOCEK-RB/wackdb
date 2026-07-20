use crate::BufferError;

/// Manages the Write-Ahead Log (WAL) to ensure ACID durability.
pub trait LogManager: Send + Sync {
    /// Forces the log to be flushed to disk up to the given Log Sequence Number (LSN).
    /// This is critical to enforce the 'WAL Before Data' protocol.
    ///
    /// # Errors
    /// Returns a `BufferError` if the underlying disk write fails.
    fn flush_up_to(&self, lsn: u64) -> Result<(), BufferError>;

    /// Appends a log record and returns its assigned LSN.
    ///
    /// # Errors
    /// Returns a `BufferError` if the underlying disk write fails or mutex is poisoned.
    fn append_record(&self, payload: &[u8]) -> Result<u64, BufferError>;

    /// Truncates the log file for checkpointing.
    ///
    /// # Errors
    /// Returns a `BufferError` if the underlying disk operation fails.
    fn checkpoint(&self) -> Result<(), BufferError>;

    /// Gets the current estimated size of the WAL (disk + buffer) in bytes.
    fn log_size(&self) -> u64;
}
