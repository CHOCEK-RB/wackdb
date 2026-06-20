//! The `wackdb_tuple` crate provides the data structures for relational tuples.
//! It handles the definition of schemas, data types, and binary serialization of records
//! to be stored on slotted pages, supporting null bitmaps and variable-length encoding.

/// Contains schema and column definitions.
pub mod schema;
/// Contains the binary tuple format logic.
pub mod tuple;
/// Contains supported scalar values and data types.
pub mod value;

pub use schema::{Column, Schema};
pub use tuple::{Tuple, TupleError};
pub use value::{DataType, Value};
