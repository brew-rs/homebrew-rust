//! Database module for tracking installed packages
//!
//! This module provides SQLite-based storage for:
//! - Installed package records
//! - Package file manifests (for uninstall)
//! - Dependency relationships
//! - Installation history

mod migrations;
pub mod models;
pub mod queries;

use anyhow::{Context, Result};
use brew_config::Paths;
use rusqlite::Connection;
use tracing::info;

pub use models::*;
pub use queries::PackageRepository;

/// Database handle for package tracking
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create the package database
    ///
    /// This will:
    /// 1. Create the database file if needed
    /// 2. Enable WAL mode for better concurrency
    /// 3. Run any pending migrations
    pub fn open(paths: &Paths) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = paths.db_file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
        }

        let conn = Connection::open(&paths.db_file)
            .with_context(|| format!("Failed to open database: {}", paths.db_file.display()))?;

        // Enable WAL mode for better concurrent access
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Enable foreign keys
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Run migrations
        migrations::run_migrations(&conn)
            .context("Failed to run database migrations")?;

        info!("Database opened: {}", paths.db_file.display());

        Ok(Self { conn })
    }

    /// Get a package repository for CRUD operations
    pub fn packages(&self) -> PackageRepository<'_> {
        PackageRepository::new(&self.conn)
    }

    /// Get the raw connection for advanced operations
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Check if there are pending migrations
    pub fn has_pending_migrations(&self) -> Result<bool> {
        migrations::has_pending_migrations(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> Paths {
        let mut paths = Paths::new().unwrap();
        paths.data_dir = temp.path().join("data");
        paths.db_file = temp.path().join("db/packages.db");
        paths
    }

    #[test]
    fn test_database_open() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        let db = Database::open(&paths).unwrap();
        assert!(!db.has_pending_migrations().unwrap());
    }

    #[test]
    fn test_database_crud() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        let db = Database::open(&paths).unwrap();
        let repo = db.packages();

        // Insert a package
        let pkg = InstalledPackage::new(
            "test-pkg".to_string(),
            "1.0.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/test-pkg/1.0.0"),
        );

        let id = repo.insert(&pkg).unwrap();
        assert!(id > 0);

        // Verify it's installed
        assert!(repo.is_installed("test-pkg").unwrap());
        assert_eq!(repo.count().unwrap(), 1);

        // List packages
        let list = repo.list_all().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test-pkg");
    }

    #[test]
    fn test_database_reopens() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        // Open, insert, close
        {
            let db = Database::open(&paths).unwrap();
            let repo = db.packages();

            let pkg = InstalledPackage::new(
                "persistent".to_string(),
                "1.0.0".to_string(),
                PathBuf::from("/opt/brew-rs/Cellar/persistent/1.0.0"),
            );
            repo.insert(&pkg).unwrap();
        }

        // Reopen and verify
        {
            let db = Database::open(&paths).unwrap();
            let repo = db.packages();
            assert!(repo.is_installed("persistent").unwrap());
        }
    }
}
