use crate::{Executor, QueryError};
use std::collections::HashMap;
use wackdb_tuple::{Schema, Tuple, value::Value};

/// Performs a hash join between a left and right executor based on an equality condition.
pub struct HashJoin<L: Executor, R: Executor> {
    left: L,
    right: R,
    left_col_name: String,
    right_col_name: String,
    schema: Schema,
    right_hash_table: Option<HashMap<Value, Vec<Tuple>>>,
    current_left_tuple: Option<Tuple>,
    current_matches: Vec<Tuple>,
    match_index: usize,
}

impl<L: Executor, R: Executor> HashJoin<L, R> {
    /// Initializes a new hash join executor.
    pub fn new(left: L, right: R, left_col: String, right_col: String) -> Self {
        let mut combined_cols = left.schema().columns.clone();
        combined_cols.extend(right.schema().columns.clone());
        let schema = Schema::new(combined_cols);

        Self {
            left,
            right,
            left_col_name: left_col,
            right_col_name: right_col,
            schema,
            right_hash_table: None,
            current_left_tuple: None,
            current_matches: Vec::new(),
            match_index: 0,
        }
    }

    fn build_hash_table(&mut self) -> Result<(), QueryError> {
        let mut hash_table: HashMap<Value, Vec<Tuple>> = HashMap::new();
        let right_schema = self.right.schema().clone();
        
        let right_idx = right_schema.columns.iter().position(|c| c.name == self.right_col_name)
            .ok_or_else(|| QueryError::Execution(format!("Right column {} not found", self.right_col_name)))?;

        while let Some(t) = self.right.next()? {
            let vals = t.to_values(&right_schema)?;
            if let Some(val) = vals.get(right_idx).cloned() {
                hash_table.entry(val).or_default().push(t);
            }
        }
        self.right_hash_table = Some(hash_table);
        Ok(())
    }
}

impl<L: Executor, R: Executor> Executor for HashJoin<L, R> {
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        if self.right_hash_table.is_none() {
            self.build_hash_table()?;
        }

        loop {
            if self.match_index < self.current_matches.len() {
                let rt = &self.current_matches[self.match_index];
                self.match_index += 1;

                if let Some(lt) = &self.current_left_tuple {
                    let mut left_vals = lt.to_values(self.left.schema())?;
                    let mut right_vals = rt.to_values(self.right.schema())?;
                    left_vals.append(&mut right_vals);
                    let combined_tuple = Tuple::from_values(&self.schema, &left_vals)?;
                    return Ok(Some(combined_tuple));
                }
            }

            self.current_matches.clear();
            self.match_index = 0;

            let Some(lt) = self.left.next()? else {
                return Ok(None);
            };

            let left_schema = self.left.schema();
            let left_idx = left_schema.columns.iter().position(|c| c.name == self.left_col_name)
                .ok_or_else(|| QueryError::Execution(format!("Left column {} not found", self.left_col_name)))?;
            
            let vals = lt.to_values(left_schema)?;
            if let Some(val) = vals.get(left_idx) {
                if let Some(hash_table) = &self.right_hash_table {
                    if let Some(matches) = hash_table.get(val) {
                        self.current_matches = matches.clone();
                    }
                }
            }
            
            self.current_left_tuple = Some(lt);
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }
}
