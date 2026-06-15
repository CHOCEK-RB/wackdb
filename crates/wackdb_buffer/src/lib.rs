//! Buffer Management for `WackDB`
//!
//! Provides the Buffer Pool Manager to cache pages in memory and reduce disk I/O.

#![warn(missing_docs)]
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    unused_crate_dependencies
)]
/// Buffer pool manager implementation.
pub mod buffer_pool;
/// Buffer pool errors.
pub mod error;
/// Memory frame descriptors.
pub mod frame;

/// Buffer pool replacement policies.
pub mod replacer;

pub use error::BufferError;
pub use replacer::ReplacementPolicy;
