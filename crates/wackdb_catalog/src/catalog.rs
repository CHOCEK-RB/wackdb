use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use crate::error::CatalogError;
use crate::metadata::TableMetadata;

/// Catalog data persisted to disk
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CatalogData {
    pub(crate) next_relation_id: u32,
    pub(crate) tables: HashMap<String, TableMetadata>,
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
        let mut is_new = false;

        if catalog_path.exists() {
            let file = File::open(&catalog_path)?;
            let reader = BufReader::new(file);
            data = serde_json::from_reader(reader)?;
        } else {
            is_new = true;
        }

        let catalog = Self { catalog_path, data };
        if is_new {
            catalog.flush()?;
        }
        Ok(catalog)
    }

    /// Flushes the catalog state to disk.
    /// # Errors
    /// Returns `CatalogError::Io` if saving the catalog to disk fails.
    /// Returns `CatalogError::Serialization` if serialization fails.
    pub fn flush(&self) -> Result<(), CatalogError> {
        let mut temp_path = self.catalog_path.clone();
        temp_path.set_extension("json.tmp");

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.data)?;

        std::fs::rename(&temp_path, &self.catalog_path)?;
        Ok(())
    }

    /// Creates a new table with an empty schema. Returns error if table already exists.
    /// # Errors
    /// Returns `CatalogError::TableExists` if the table name is taken.
    pub fn create_table(&mut self, name: &str) -> Result<&TableMetadata, CatalogError> {
        self.create_table_with_schema(name, wackdb_tuple::Schema::new(vec![]))
    }

    /// Creates a new table with a given schema.
    /// # Errors
    /// Returns `CatalogError::TableExists` if the table name is taken.
    pub fn create_table_with_schema(
        &mut self,
        name: &str,
        schema: wackdb_tuple::Schema,
    ) -> Result<&TableMetadata, CatalogError> {
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
            schema,
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

    /// Retrieves schema by name.
    /// # Errors
    /// Returns `CatalogError::TableNotFound` if the table does not exist.
    pub fn get_schema(&self, name: &str) -> Result<wackdb_tuple::Schema, CatalogError> {
        self.get_table(name).map(|meta| meta.schema)
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

    /// Drops a table from the catalog.
    /// # Errors
    /// Returns `CatalogError::TableNotFound` if the table does not exist.
    pub fn drop_table(&mut self, name: &str) -> Result<TableMetadata, CatalogError> {
        let meta = self
            .data
            .tables
            .remove(name)
            .ok_or_else(|| CatalogError::TableNotFound(name.to_string()))?;
        self.flush()?;
        Ok(meta)
    }

    /// Lists all tables in the catalog.
    #[must_use]
    pub fn list_tables(&self) -> Vec<TableMetadata> {
        self.data.tables.values().cloned().collect()
    }
}
