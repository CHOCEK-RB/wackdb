#![warn(missing_docs)]
//! `WackDB` Catalog
//!
//! Minimal catalog for mapping logical table names to physical relations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Catalog errors
#[derive(Error, Debug)]
pub enum CatalogError {
    /// IO Error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Table already exists
    #[error("Table '{0}' already exists")]
    TableExists(String),
    /// Table not found
    #[error("Table '{0}' not found")]
    TableNotFound(String),
}

/// Minimal metadata for a single table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Logical table name
    pub name: String,
    /// The physical file ID for heap data.
    pub heap_relation_id: u32,
    /// The physical file ID for the B+Tree index.
    pub index_relation_id: u32,
    /// Root page number within this relation's file, if initialized
    pub root_page_num: Option<u32>,
}

/// Catalog data persisted to disk
#[derive(Debug, Serialize, Deserialize, Default)]
struct CatalogData {
    next_relation_id: u32,
    tables: HashMap<String, TableMetadata>,
}

/// Catalog manager
pub struct Catalog {
    catalog_path: PathBuf,
    data: CatalogData,
}

impl Catalog {
    /// Opens an existing catalog or creates a new one at the specified directory.
    /// # Errors
    /// Returns `CatalogError::Io` if reading or creating the catalog file fails.
    /// Returns `CatalogError::Serialization` if the catalog data is corrupted.
    pub fn open<P: AsRef<Path>>(data_dir: P) -> Result<Self, CatalogError> {
        let catalog_path = data_dir.as_ref().join("catalog.json");
        let mut data = CatalogData::default();

        if catalog_path.exists() {
            let file = File::open(&catalog_path)?;
            let reader = BufReader::new(file);
            data = serde_json::from_reader(reader)?;
        }

        Ok(Self { catalog_path, data })
    }

    /// Flushes the catalog state to disk.
    /// # Errors
    /// Returns `CatalogError::Io` if saving the catalog to disk fails.
    /// Returns `CatalogError::Serialization` if serialization fails.
    pub fn flush(&self) -> Result<(), CatalogError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.catalog_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.data)?;
        Ok(())
    }

    /// Creates a new table. Returns error if table already exists.
    /// # Errors
    /// Returns `CatalogError::TableAlreadyExists` if the table name is taken.
    pub fn create_table(&mut self, name: &str) -> Result<&TableMetadata, CatalogError> {
        if self.data.tables.contains_key(name) {
            return Err(CatalogError::TableExists(name.to_string()));
        }

        let heap_id = self.data.next_relation_id;
        self.data.next_relation_id += 1;
        let index_id = self.data.next_relation_id;
        self.data.next_relation_id += 1;

        let meta = TableMetadata {
            name: name.to_string(),
            heap_relation_id: heap_id,
            index_relation_id: index_id,
            root_page_num: None,
        };

        self.data.tables.insert(name.to_string(), meta);
        self.flush()?;

        self.data
            .tables
            .get(name)
            .ok_or_else(|| CatalogError::TableNotFound(name.to_string()))
    }

    /// Retrieves table metadata by name.
    /// # Errors
    /// Returns `CatalogError::TableNotFound` if the table does not exist.
    pub fn get_table(&self, name: &str) -> Result<TableMetadata, CatalogError> {
        self.data
            .tables
            .get(name)
            .cloned()
            .ok_or_else(|| CatalogError::TableNotFound(name.to_string()))
    }

    /// Updates the root page number for a given table.
    /// # Errors
    /// Returns `CatalogError::TableNotFound` if the table does not exist.
    pub fn update_root_page(
        &mut self,
        name: &str,
        root_page_num: Option<u32>,
    ) -> Result<(), CatalogError> {
        if let Some(meta) = self.data.tables.get_mut(name) {
            meta.root_page_num = root_page_num;
            self.flush()?;
            Ok(())
        } else {
            Err(CatalogError::TableNotFound(name.to_string()))
        }
    }

    /// Lists all tables in the catalog.
    #[must_use]
    pub fn list_tables(&self) -> Vec<TableMetadata> {
        self.data.tables.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_get_table() {
        let dir = tempdir().unwrap();
        let mut catalog = Catalog::open(dir.path()).unwrap();

        let table = catalog.create_table("users").unwrap();
        assert_eq!(table.name, "users");
        assert_eq!(table.heap_relation_id, 0);
        assert_eq!(table.index_relation_id, 1);
        assert_eq!(table.root_page_num, None);

        let table2 = catalog.get_table("users").unwrap();
        assert_eq!(table2.name, "users");
        assert_eq!(table2.heap_relation_id, 0);
        assert_eq!(table2.index_relation_id, 1);
    }

    #[test]
    fn test_duplicate_table() {
        let dir = tempdir().unwrap();
        let mut catalog = Catalog::open(dir.path()).unwrap();

        catalog.create_table("test").unwrap();
        let err = catalog.create_table("test").unwrap_err();
        assert!(matches!(err, CatalogError::TableExists(_)));
    }

    #[test]
    fn test_persistence() {
        let dir = tempdir().unwrap();

        {
            let mut catalog = Catalog::open(dir.path()).unwrap();
            catalog.create_table("t1").unwrap();
            catalog.update_root_page("t1", Some(42)).unwrap();
        }

        {
            let catalog = Catalog::open(dir.path()).unwrap();
            let table = catalog.get_table("t1").unwrap();
            assert_eq!(table.name, "t1");
            assert_eq!(table.heap_relation_id, 0);
            assert_eq!(table.index_relation_id, 1);
            assert_eq!(table.root_page_num, Some(42));
        }
    }

    #[test]
    fn test_list_tables() {
        let dir = tempdir().unwrap();
        let mut catalog = Catalog::open(dir.path()).unwrap();

        catalog.create_table("a").unwrap();
        catalog.create_table("b").unwrap();

        let tables = catalog.list_tables();
        assert_eq!(tables.len(), 2);
    }
}
