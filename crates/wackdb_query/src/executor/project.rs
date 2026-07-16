use crate::{Executor, QueryError};
use wackdb_tuple::{Schema, Tuple, value::Value};

/// Projects specific columns from a child executor into a new schema.
pub struct Project<E: Executor> {
    child: E,
    output_schema: Schema,
    projection_indices: Vec<usize>,
}

impl<E: Executor> Project<E> {
    /// Initializes a new projection executor.
    pub fn new(child: E, output_schema: Schema, projection_indices: Vec<usize>) -> Self {
        Self {
            child,
            output_schema,
            projection_indices,
        }
    }
}

impl<E: Executor> Executor for Project<E> {
    /// Retrieves the next tuple with only the projected columns.
    ///
    /// # Errors
    /// Returns `QueryError` if tuple parsing or creation fails.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        if let Some(tuple) = self.child.next()? {
            let values = tuple.to_values(self.child.schema())?;
            let projected_values: Vec<Value> = self
                .projection_indices
                .iter()
                .map(|&i| values[i].clone())
                .collect();

            let new_tuple = Tuple::from_values(&self.output_schema, &projected_values)?;
            Ok(Some(new_tuple))
        } else {
            Ok(None)
        }
    }

    /// Returns the projected output schema.
    fn schema(&self) -> &Schema {
        &self.output_schema
    }
}
