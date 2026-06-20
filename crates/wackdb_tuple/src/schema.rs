use crate::value::DataType;

/// Represents a single column definition within a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The logical name of the column.
    pub name: String,
    /// The scalar data type of the column.
    pub data_type: DataType,
    /// Indicates whether this column permits NULL values.
    pub is_nullable: bool,
}

impl Column {
    /// Creates a new column definition.
    #[must_use]
    pub fn new(name: &str, data_type: DataType, is_nullable: bool) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            is_nullable,
        }
    }
}

/// Represents the structural schema of a tuple (a collection of columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    /// The ordered list of columns defining this schema.
    pub columns: Vec<Column>,
}

impl Schema {
    /// Creates a new schema from a vector of columns.
    #[must_use]
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    /// Returns the total number of columns.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Calculates the size of the null bitmap in bytes.
    /// Uses 1 bit per column, rounded up to the nearest byte.
    #[must_use]
    pub fn bitmap_size(&self) -> usize {
        self.columns.len().div_ceil(8)
    }
}
