//! Page management for `WackDB`
//!
//! Provides the physical layout and abstractions for a slotted page,
//! including slot directory, tuple headers, and page metadata.

#![warn(missing_docs)]
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_ptr_alignment,
    unused_crate_dependencies
)]
/// Defines the structure of the page and tuple headers.
pub mod header;
/// The core `SlottedPage` implementation.
pub mod page;
/// Defines the `PageSlot` structure for the slot directory.
pub mod slot;

pub use header::{SlottedPageHeader, TupleHeader};
pub use page::SlottedPage;
pub use slot::PageSlot;
