use crate::{Executor, QueryError};
use wackdb_tuple::{Schema, Tuple};

/// Performs a nested loop join between a left and right executor.
pub struct NestedLoopJoin<L: Executor, R: Executor> {
    left: L,
    right: R,
    predicate: Box<dyn Fn(&Tuple, &Schema, &Tuple, &Schema) -> bool>,
    schema: Schema,
    left_tuple: Option<Tuple>,
    right_tuples: Option<Vec<Tuple>>,
    right_index: usize,
}

impl<L: Executor, R: Executor> NestedLoopJoin<L, R> {
    /// Initializes a new nested loop join executor.
    pub fn new(
        left: L,
        right: R,
        predicate: Box<dyn Fn(&Tuple, &Schema, &Tuple, &Schema) -> bool>,
    ) -> Self {
        let mut combined_cols = left.schema().columns.clone();
        combined_cols.extend(right.schema().columns.clone());
        let schema = Schema::new(combined_cols);

        Self {
            left,
            right,
            predicate,
            schema,
            left_tuple: None,
            right_tuples: None,
            right_index: 0,
        }
    }
}

impl<L: Executor, R: Executor> Executor for NestedLoopJoin<L, R> {
    /// Retrieves the next joined tuple.
    ///
    /// # Errors
    /// Returns `QueryError` if tuple materialization or child executors fail.
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        if self.right_tuples.is_none() {
            let mut rt = Vec::new();
            while let Some(t) = self.right.next()? {
                rt.push(t);
            }
            self.right_tuples = Some(rt);
            self.left_tuple = self.left.next()?;
        }

        let Some(right_tuples) = self.right_tuples.as_ref() else {
            return Ok(None);
        };

        loop {
            let Some(lt) = &self.left_tuple else {
                return Ok(None);
            };

            if self.right_index < right_tuples.len() {
                let rt = &right_tuples[self.right_index];
                self.right_index += 1;

                if (self.predicate)(lt, self.left.schema(), rt, self.right.schema()) {
                    let mut left_vals = lt.to_values(self.left.schema())?;
                    let mut right_vals = rt.to_values(self.right.schema())?;
                    left_vals.append(&mut right_vals);
                    let combined_tuple = Tuple::from_values(&self.schema, &left_vals)?;
                    return Ok(Some(combined_tuple));
                }
            } else {
                self.left_tuple = self.left.next()?;
                self.right_index = 0;
            }
        }
    }

    /// Returns the combined schema of the joined tuples.
    fn schema(&self) -> &Schema {
        &self.schema
    }
}
