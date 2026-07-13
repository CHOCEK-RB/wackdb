#![warn(missing_docs)]
//! `WackDB` Catalog
//!
//! Minimal catalog for mapping logical table names to physical relations.

/// Catalog manager and logical persistence
pub mod catalog;
/// Catalog errors
pub mod error;
/// Catalog metadata definitions
pub mod metadata;

pub use catalog::{Catalog, CatalogData};
pub use error::CatalogError;
pub use metadata::TableMetadata;

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_get_table() {
        let dir = tempdir().unwrap();
        let mut catalog = Catalog::open(dir.path()).unwrap();

        let table = catalog.create_table("test_users").unwrap();
        assert_eq!(table.name, "test_users");
        assert_eq!(table.heap_relation_id, 0);
        assert_eq!(table.index_relation_id, 1);
        assert_eq!(table.root_page_num, None);

        let table2 = catalog.get_table("test_users").unwrap();
        assert_eq!(table2.name, "test_users");
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
