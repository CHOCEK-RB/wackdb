use serde::{Deserialize, Serialize};

/// Supported data types in `WackDB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    /// 4-byte signed integer
    Integer,
    /// 1-byte boolean
    Boolean,
    /// Variable-length string
    Varchar,
}

/// Represents a scalar value in a tuple.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Value {
    /// Represents an SQL NULL
    Null,
    /// Represents a 4-byte signed integer
    Integer(i32),
    /// Represents a 1-byte boolean
    Boolean(bool),
    /// Represents a variable-length string
    Varchar(String),
}

impl Value {
    /// Returns the underlying data type of this value.
    #[must_use]
    pub fn data_type(&self) -> Option<DataType> {
        match self {
            Self::Null => None,
            Self::Integer(_) => Some(DataType::Integer),
            Self::Boolean(_) => Some(DataType::Boolean),
            Self::Varchar(_) => Some(DataType::Varchar),
        }
    }

    /// Determines if the value is fixed-length.
    #[must_use]
    pub fn is_fixed_length(dt: DataType) -> bool {
        match dt {
            DataType::Integer | DataType::Boolean => true,
            DataType::Varchar => false,
        }
    }

    /// Returns the size in bytes for fixed-length data types.
    #[must_use]
    pub fn fixed_length_size(dt: DataType) -> usize {
        match dt {
            DataType::Integer => 4,
            DataType::Boolean => 1,
            DataType::Varchar => 0, // Variable length has 0 fixed size here (handled via offsets)
        }
    }

    /// Parses a string into a Typed Value.
    /// # Errors
    /// Returns an error if the string format does not match the expected data type.
    pub fn parse_from_string(s: &str, dt: DataType) -> Result<Self, String> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("null") {
            return Ok(Value::Null);
        }
        match dt {
            DataType::Integer => {
                let parsed = s
                    .parse::<i32>()
                    .map_err(|_| format!("TypeError: Expected Integer, got '{s}'"))?;
                Ok(Value::Integer(parsed))
            }
            DataType::Boolean => {
                let lower = s.to_lowercase();
                if lower == "true" {
                    Ok(Value::Boolean(true))
                } else if lower == "false" {
                    Ok(Value::Boolean(false))
                } else {
                    Err(format!("TypeError: Expected Boolean, got '{s}'"))
                }
            }
            DataType::Varchar => {
                if !s.starts_with('\'') && !s.starts_with('"') {
                    return Err(format!(
                        "TypeError: Expected Varchar (quoted string), got '{s}'"
                    ));
                }
                let unquoted = s.trim_matches('\'').trim_matches('"').to_string();
                Ok(Value::Varchar(unquoted))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_safety_mismatched_types() {
        let res = Value::parse_from_string("not_an_int", DataType::Integer);
        assert_eq!(
            res,
            Err("TypeError: Expected Integer, got 'not_an_int'".to_string())
        );

        let res = Value::parse_from_string("not_a_bool", DataType::Boolean);
        assert_eq!(
            res,
            Err("TypeError: Expected Boolean, got 'not_a_bool'".to_string())
        );
    }
}
