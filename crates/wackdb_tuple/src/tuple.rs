#![allow(clippy::indexing_slicing, clippy::match_same_arms, clippy::pedantic)]

use crate::schema::Schema;
use crate::value::{DataType, Value};
use std::convert::TryInto;
use thiserror::Error;

/// Errors that can occur during tuple operations.
#[derive(Error, Debug)]
pub enum TupleError {
    /// Schema column count does not match the provided value count.
    #[error("Schema column count ({schema_count}) does not match value count ({value_count})")]
    ColumnCountMismatch {
        /// Expected number of columns based on schema
        schema_count: usize,
        /// Actual number of values provided
        value_count: usize,
    },
    /// A null value was provided for a non-nullable column.
    #[error("Null value provided for non-nullable column '{column_name}'")]
    NullNotAllowed {
        /// The name of the column that rejected the null
        column_name: String,
    },
    /// An error occurred during byte deserialization.
    #[error("Data deserialization error: {0}")]
    DeserializationError(String),
}

/// A serialized record in WackDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tuple {
    /// The raw byte representation of the tuple, ready to be inserted into a SlottedPage.
    pub data: Vec<u8>,
}

impl Tuple {
    /// Serializes a sequence of values into a dense binary tuple according to the schema.
    ///
    /// The binary layout is:
    /// [ Null Bitmap ] [ Fixed Length Data & VarLen Offsets ] [ Variable Length Data ]
    ///
    /// # Errors
    /// Returns `TupleError` if the values do not match the schema (e.g. invalid counts or nulls).
    pub fn from_values(schema: &Schema, values: &[Value]) -> Result<Self, TupleError> {
        if schema.column_count() != values.len() {
            return Err(TupleError::ColumnCountMismatch {
                schema_count: schema.column_count(),
                value_count: values.len(),
            });
        }

        let bitmap_size = schema.bitmap_size();
        let mut bitmap = vec![0u8; bitmap_size];

        let mut fixed_section = Vec::new();
        let mut var_section = Vec::new();

        for (i, (col, val)) in schema.columns.iter().zip(values.iter()).enumerate() {
            let byte_idx = i / 8;
            let bit_idx = i % 8;

            if val == &Value::Null {
                if !col.is_nullable {
                    return Err(TupleError::NullNotAllowed {
                        column_name: col.name.clone(),
                    });
                }
                // Set the null bit to 1
                bitmap[byte_idx] |= 1 << bit_idx;

                // Advance fixed section with zeros depending on type to maintain alignment
                match col.data_type {
                    DataType::Integer => fixed_section.extend_from_slice(&[0; 4]),
                    DataType::Boolean => fixed_section.extend_from_slice(&[0; 1]),
                    DataType::Varchar => fixed_section.extend_from_slice(&[0; 4]), // offset + length
                }
            } else {
                match val {
                    Value::Integer(v) => {
                        fixed_section.extend_from_slice(&v.to_le_bytes());
                    }
                    Value::Boolean(v) => {
                        let byte = if *v { 1u8 } else { 0u8 };
                        fixed_section.push(byte);
                    }
                    Value::Varchar(s) => {
                        let bytes = s.as_bytes();
                        let offset: u16 = var_section.len().try_into().unwrap_or(0);
                        let length: u16 = bytes.len().try_into().unwrap_or(0);

                        // Store u16 offset and u16 length
                        fixed_section.extend_from_slice(&offset.to_le_bytes());
                        fixed_section.extend_from_slice(&length.to_le_bytes());

                        var_section.extend_from_slice(bytes);
                    }
                    Value::Null => unreachable!(),
                }
            }
        }

        // The true var offset begins after the bitmap and fixed section
        let var_base_offset = bitmap.len() + fixed_section.len();

        // Adjust all the Varchar offsets in the fixed section to be absolute from the start of the tuple
        let mut fixed_offset_cursor = 0;
        for (i, col) in schema.columns.iter().enumerate() {
            if bitmap[i / 8] & (1 << (i % 8)) == 0 && col.data_type == DataType::Varchar {
                let current_offset_bytes =
                    &fixed_section[fixed_offset_cursor..fixed_offset_cursor + 2];
                let relative_offset =
                    u16::from_le_bytes(current_offset_bytes.try_into().unwrap_or([0; 2]));

                // Add the base offset
                #[allow(clippy::cast_possible_truncation)]
                let absolute_offset = relative_offset + (var_base_offset as u16);

                fixed_section[fixed_offset_cursor..fixed_offset_cursor + 2]
                    .copy_from_slice(&absolute_offset.to_le_bytes());
            }

            // Advance cursor
            match col.data_type {
                DataType::Integer => fixed_offset_cursor += 4,
                DataType::Boolean => fixed_offset_cursor += 1,
                DataType::Varchar => fixed_offset_cursor += 4, // 2 offset + 2 length
            }
        }

        let mut data = Vec::with_capacity(bitmap.len() + fixed_section.len() + var_section.len());
        data.extend(bitmap);
        data.extend(fixed_section);
        data.extend(var_section);

        Ok(Self { data })
    }

    /// Deserializes a binary tuple back into values using the schema.
    ///
    /// # Errors
    /// Returns `TupleError` on bounds issues or malformed data.
    pub fn to_values(&self, schema: &Schema) -> Result<Vec<Value>, TupleError> {
        let bitmap_size = schema.bitmap_size();
        if self.data.len() < bitmap_size {
            return Err(TupleError::DeserializationError(
                "Data too short for bitmap".into(),
            ));
        }

        let bitmap = self
            .data
            .get(0..bitmap_size)
            .ok_or_else(|| TupleError::DeserializationError("Data too short for bitmap".into()))?;
        let mut fixed_cursor = bitmap_size;
        let mut values = Vec::with_capacity(schema.column_count());

        for (i, col) in schema.columns.iter().enumerate() {
            let is_null = bitmap
                .get(i / 8)
                .map(|b| (b & (1 << (i % 8))) != 0)
                .unwrap_or(false);
            if is_null {
                values.push(Value::Null);
                // Advance cursor over the empty fixed slot
                match col.data_type {
                    DataType::Boolean => fixed_cursor += 1,
                    DataType::Integer | DataType::Varchar => fixed_cursor += 4,
                }
                continue;
            }

            match col.data_type {
                DataType::Integer => {
                    if fixed_cursor + 4 > self.data.len() {
                        return Err(TupleError::DeserializationError(
                            "Out of bounds integer".into(),
                        ));
                    }
                    let bytes = self
                        .data
                        .get(fixed_cursor..fixed_cursor + 4)
                        .ok_or_else(|| {
                            TupleError::DeserializationError("Out of bounds integer".into())
                        })?;
                    let val = i32::from_le_bytes(bytes.try_into().unwrap_or([0; 4]));
                    values.push(Value::Integer(val));
                    fixed_cursor += 4;
                }
                DataType::Boolean => {
                    if fixed_cursor + 1 > self.data.len() {
                        return Err(TupleError::DeserializationError(
                            "Out of bounds boolean".into(),
                        ));
                    }
                    let byte = self.data.get(fixed_cursor).ok_or_else(|| {
                        TupleError::DeserializationError("Out of bounds boolean".into())
                    })?;
                    let val = *byte != 0;
                    values.push(Value::Boolean(val));
                    fixed_cursor += 1;
                }
                DataType::Varchar => {
                    if fixed_cursor + 4 > self.data.len() {
                        return Err(TupleError::DeserializationError(
                            "Out of bounds varchar pointer".into(),
                        ));
                    }
                    let offset_bytes =
                        self.data
                            .get(fixed_cursor..fixed_cursor + 2)
                            .ok_or_else(|| {
                                TupleError::DeserializationError(
                                    "Out of bounds varchar pointer".into(),
                                )
                            })?;
                    let length_bytes = self
                        .data
                        .get(fixed_cursor + 2..fixed_cursor + 4)
                        .ok_or_else(|| {
                            TupleError::DeserializationError("Out of bounds varchar pointer".into())
                        })?;

                    let offset =
                        u16::from_le_bytes(offset_bytes.try_into().unwrap_or([0; 2])) as usize;
                    let length =
                        u16::from_le_bytes(length_bytes.try_into().unwrap_or([0; 2])) as usize;

                    if offset + length > self.data.len() {
                        return Err(TupleError::DeserializationError(
                            "Out of bounds varchar payload".into(),
                        ));
                    }

                    let str_bytes = self.data.get(offset..offset + length).ok_or_else(|| {
                        TupleError::DeserializationError("Out of bounds varchar payload".into())
                    })?;
                    let s = String::from_utf8(str_bytes.to_vec())
                        .map_err(|_| TupleError::DeserializationError("Invalid UTF-8".into()))?;

                    values.push(Value::Varchar(s));
                    fixed_cursor += 4;
                }
            }
        }

        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Column;

    #[test]
    fn test_serialize_deserialize_mixed() {
        let schema = Schema::new(vec![
            Column::new("id", DataType::Integer, false),
            Column::new("is_active", DataType::Boolean, false),
            Column::new("name", DataType::Varchar, true),
        ]);

        let values = vec![
            Value::Integer(42),
            Value::Boolean(true),
            Value::Varchar("Alice".to_string()),
        ];

        let tuple = Tuple::from_values(&schema, &values).unwrap();

        // Bitmap (1 byte) + Integer (4 bytes) + Bool (1 byte) + Varchar ptr (4 bytes) + "Alice" (5 bytes)
        assert_eq!(tuple.data.len(), 1 + 4 + 1 + 4 + 5);

        let decoded = tuple.to_values(&schema).unwrap();
        assert_eq!(values, decoded);
    }

    #[test]
    fn test_serialize_deserialize_null() {
        let schema = Schema::new(vec![
            Column::new("id", DataType::Integer, false),
            Column::new("name", DataType::Varchar, true),
        ]);

        let values = vec![Value::Integer(100), Value::Null];

        let tuple = Tuple::from_values(&schema, &values).unwrap();
        let decoded = tuple.to_values(&schema).unwrap();
        assert_eq!(values, decoded);
    }
}
