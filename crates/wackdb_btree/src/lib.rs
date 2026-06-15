//! B+ Tree Indexing for `WackDB`
#![warn(missing_docs)]
#![allow(unused_crate_dependencies)]

/// Tree node pages definition.
pub mod node;
/// Abstract interface for indexing structures.
pub mod traits;
/// B+ Tree index orchestrator.
pub mod tree;
