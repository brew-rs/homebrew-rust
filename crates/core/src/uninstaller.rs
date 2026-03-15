//! Package uninstaller
//!
//! Removes a package from the Cellar, cleans up symlinks in bin_dir,
//! and deletes the database record. Checks reverse dependencies before
//! removing unless `force` is true.

use anyhow::{bail, Context, Result};
use brew_config::Paths;
use std::fs;
use tracing::info;

use crate::database::{Database, InstallAction, InstallHistoryEntry};
use crate::linker;

/// Uninstall a package.
///
/// Steps:
/// 1. Look up the package in the database — error if not installed.
/// 2. Check reverse dependencies — error if any exist and `force` is false.
/// 3. Remove symlinks from bin_dir.
/// 4. Remove the cellar directory.
/// 5. Delete the database record (cascades to files + deps).
/// 6. Record the uninstall action in history.
///
/// Returns the version string of the uninstalled package on success.
pub fn uninstall(db: &Database, paths: &Paths, name: &str, force: bool) -> Result<String> {
    let repo = db.packages();

    // Step 1: Confirm the package is installed.
    let pkg = repo
        .find_by_name(name)
        .context("Failed to query package database")?
        .with_context(|| format!("{} is not installed", name))?;

    // Step 2: Reverse dependency check.
    let rdeps = repo
        .get_reverse_dependencies(name)
        .context("Failed to query reverse dependencies")?;

    if !rdeps.is_empty() && !force {
        bail!(
            "Cannot uninstall {}: required by {}",
            name,
            rdeps.join(", ")
        );
    }

    // Step 3: Remove symlinks from bin_dir.
    let cellar_path = paths.package_cellar(name, &pkg.version);
    linker::unlink_package(&cellar_path, &paths.bin_dir)
        .with_context(|| format!("Failed to unlink {}", name))?;

    // Step 4: Remove the cellar directory.
    if cellar_path.exists() {
        fs::remove_dir_all(&cellar_path)
            .with_context(|| format!("Failed to remove cellar directory: {}", cellar_path.display()))?;
        info!("Removed cellar: {}", cellar_path.display());
    }

    // Step 5: Delete the database record (CASCADE removes files + deps).
    repo.delete(name)
        .context("Failed to delete package from database")?;

    // Step 6: Record history.
    let history = InstallHistoryEntry::new(
        name.to_string(),
        pkg.version.clone(),
        InstallAction::Uninstall,
        true,
    );
    repo.record_history(&history)
        .context("Failed to record uninstall history")?;

    info!("Uninstalled {} {}", name, pkg.version);
    Ok(pkg.version.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{
        Database, DependencyType, InstallAction, InstallHistoryEntry, InstalledPackage,
        PackageDependency,
    };
    use brew_config::Paths;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_paths(tmp: &TempDir) -> Paths {
        let mut paths = Paths::new().unwrap();
        paths.cellar_dir = tmp.path().join("cellar");
        paths.bin_dir = tmp.path().join("bin");
        paths.downloads_dir = tmp.path().join("downloads");
        paths.cache_dir = tmp.path().join("cache");
        paths.db_dir = tmp.path().join("db");
        paths.db_file = tmp.path().join("db").join("packages.db");
        paths.data_dir = tmp.path().join("data");
        paths
    }

    fn insert_test_package(db: &Database, paths: &Paths, name: &str, version: &str) {
        let cellar = paths.package_cellar(name, version);
        let bin = cellar.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let binary = bin.join(name);
        fs::write(&binary, b"#!/bin/sh").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let pkg = InstalledPackage::new(
            name.to_string(),
            version.to_string(),
            cellar,
        );
        db.packages().insert(&pkg).unwrap();
    }

    fn setup_db(tmp: &TempDir) -> (Database, Paths) {
        let paths = make_test_paths(tmp);
        fs::create_dir_all(&paths.db_dir).unwrap();
        fs::create_dir_all(&paths.bin_dir).unwrap();
        let db = Database::open(&paths).unwrap();
        (db, paths)
    }

    #[test]
    fn test_uninstall_not_installed_errors() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        let result = uninstall(&db, &paths, "nonexistent", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed"));
    }

    #[test]
    fn test_uninstall_with_reverse_deps_blocked() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        // Install openssl and curl (curl depends on openssl)
        insert_test_package(&db, &paths, "openssl", "3.4.4");
        insert_test_package(&db, &paths, "curl", "8.18.0");

        let curl_id = db.packages().find_by_name("curl").unwrap().unwrap().id.unwrap();
        let dep = PackageDependency {
            id: None,
            package_id: curl_id,
            dependency_name: "openssl".to_string(),
            dependency_type: DependencyType::Runtime,
            version_constraint: None,
            is_satisfied: true,
        };
        db.packages().add_dependency(&dep).unwrap();

        let result = uninstall(&db, &paths, "openssl", false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("required by"), "expected 'required by' in: {}", msg);
        assert!(msg.contains("curl"), "expected 'curl' in: {}", msg);

        // openssl should still be installed
        assert!(db.packages().is_installed("openssl").unwrap());
    }

    #[test]
    fn test_uninstall_force_ignores_reverse_deps() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        insert_test_package(&db, &paths, "openssl", "3.4.4");
        insert_test_package(&db, &paths, "curl", "8.18.0");

        let curl_id = db.packages().find_by_name("curl").unwrap().unwrap().id.unwrap();
        let dep = PackageDependency {
            id: None,
            package_id: curl_id,
            dependency_name: "openssl".to_string(),
            dependency_type: DependencyType::Runtime,
            version_constraint: None,
            is_satisfied: true,
        };
        db.packages().add_dependency(&dep).unwrap();

        // Force uninstall should succeed
        let result = uninstall(&db, &paths, "openssl", true);
        assert!(result.is_ok(), "force uninstall failed: {:?}", result);
        assert!(!db.packages().is_installed("openssl").unwrap());
    }

    #[test]
    fn test_uninstall_removes_cellar_directory() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        insert_test_package(&db, &paths, "jq", "1.8.1");
        let cellar = paths.package_cellar("jq", "1.8.1");
        assert!(cellar.exists());

        uninstall(&db, &paths, "jq", false).unwrap();
        assert!(!cellar.exists(), "cellar dir should be gone");
    }

    #[test]
    fn test_uninstall_removes_symlinks() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        insert_test_package(&db, &paths, "jq", "1.8.1");

        // First link the package so a symlink exists
        let cellar = paths.package_cellar("jq", "1.8.1");
        linker::link_package(&cellar, &paths.bin_dir, &paths.cellar_dir).unwrap();
        assert!(paths.bin_dir.join("jq").exists());

        uninstall(&db, &paths, "jq", false).unwrap();
        assert!(!paths.bin_dir.join("jq").exists(), "symlink should be removed");
    }

    #[test]
    fn test_uninstall_removes_database_record() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        insert_test_package(&db, &paths, "jq", "1.8.1");
        assert!(db.packages().is_installed("jq").unwrap());

        uninstall(&db, &paths, "jq", false).unwrap();
        assert!(!db.packages().is_installed("jq").unwrap());
    }

    #[test]
    fn test_uninstall_records_history() {
        let tmp = TempDir::new().unwrap();
        let (db, paths) = setup_db(&tmp);

        insert_test_package(&db, &paths, "jq", "1.8.1");
        uninstall(&db, &paths, "jq", false).unwrap();

        let history = db.packages().get_recent_history(10).unwrap();
        let uninstall_entry = history
            .iter()
            .find(|e| e.package_name == "jq" && e.action == InstallAction::Uninstall);
        assert!(uninstall_entry.is_some(), "uninstall history entry missing");
        assert!(uninstall_entry.unwrap().success);
    }
}
