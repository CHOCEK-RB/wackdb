/// Supported data types in `WackDB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// 4-byte signed integer
    Integer,
    /// 1-byte boolean
    Boolean,
    /// Variable-length string
    Varchar,
}

/// Represents a scalar value in a tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
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
}
