use crate::{Executor, QueryError};
use wackdb_tuple::{Schema, Tuple};

/// Filters tuples from a child executor based on a predicate.
pub struct Select<E: Executor> {
    child: E,
    predicate: Box<dyn Fn(&Tuple, &Schema) -> bool>,
}

impl<E: Executor> Select<E> {
    /// Initializes a new selection executor.
    pub fn new(child: E, predicate: Box<dyn Fn(&Tuple, &Schema) -> bool>) -> Self {
        Self { child, predicate }
    }
}

impl<E: Executor> Executor for Select<E> {
    /// Retrieves the next tuple that satisfies the predicate.
    ///
    /// # Errors
    /// Returns `QueryError` if the child executor encounters an error.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        while let Some(tuple) = self.child.next()? {
            if (self.predicate)(&tuple, self.child.schema()) {
                return Ok(Some(tuple));
            }
        }
        Ok(None)
    }

    /// Returns the schema of the tuples, identical to the child's schema.
    fn schema(&self) -> &Schema {
        self.child.schema()
    }
}
