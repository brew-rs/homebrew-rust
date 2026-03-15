//! Core package manager functionality
//!
//! This crate provides the core logic for package management including:
//! - Package installation and removal
//! - State management (tracking installed packages)
//! - Build execution
//! - Symlink management
//! - Package database with migration support

pub mod database;
pub mod installer;
pub mod state;

use anyhow::Result;
use brew_config::Paths;
use brew_solver::Resolver;
use tracing::info;

pub use database::{Database, InstalledPackage, PackageSummary, PackageRepository};

/// Package manager core
pub struct PackageManager {
    db: Database,
    #[allow(dead_code)]
    resolver: Resolver,
    paths: Paths,
}

impl PackageManager {
    /// Create a new package manager instance
    pub fn new(paths: Paths) -> Result<Self> {
        let db = Database::open(&paths)?;
        Ok(Self {
            db,
            resolver: Resolver::new(),
            paths,
        })
    }

    /// Install a package
    pub async fn install(&mut self, package_name: &str) -> Result<()> {
        info!("Installing package: {}", package_name);
        // TODO: Implement installation logic
        Ok(())
    }

    /// Uninstall a package
    pub async fn uninstall(&mut self, package_name: &str) -> Result<()> {
        info!("Uninstalling package: {}", package_name);
        // TODO: Implement uninstallation logic
        Ok(())
    }

    /// List installed packages
    pub fn list_installed(&self) -> Result<Vec<PackageSummary>> {
        self.db.packages().list_all()
    }

    /// Check if a package is installed
    pub fn is_installed(&self, name: &str) -> Result<bool> {
        self.db.packages().is_installed(name)
    }

    /// Get database reference
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Get paths reference
    pub fn paths(&self) -> &Paths {
        &self.paths
    }
}
