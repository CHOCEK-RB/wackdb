//! Application state module for the web server.

use parking_lot::RwLock;
use std::sync::Arc;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::disk_manager::BasicDiskManager;

/// The size of a page in bytes, matching the database configuration.
pub const PAGE_SIZE: usize = 8192;

/// A thread-safe, reference-counted pointer to the Buffer Pool Manager.
pub type SharedBufferPool =
    Arc<RwLock<BufferPoolManager<{ PAGE_SIZE }, BasicDiskManager<{ PAGE_SIZE }>>>>;

/// Application state shared across all HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    /// Shared reference to the Buffer Pool Manager.
    pub buffer_pool: SharedBufferPool,
    /// The directory where database files are stored.
    pub data_dir: String,
}
