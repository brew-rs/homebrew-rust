//! Database query operations
//!
//! CRUD operations for package records.

use super::models::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;

/// Repository for package database operations
pub struct PackageRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PackageRepository<'a> {
    /// Create a new repository with a database connection
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ==================== Package CRUD ====================

    /// Insert a new package record
    pub fn insert(&self, pkg: &InstalledPackage) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO packages (
                name, version, description, homepage, license, tap,
                installed_at, updated_at, install_type, build_from_source,
                cellar_path, linked, pinned, source_sha256
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                pkg.name,
                pkg.version,
                pkg.description,
                pkg.homepage,
                pkg.license,
                pkg.tap,
                pkg.installed_at,
                pkg.updated_at,
                pkg.install_type.as_str(),
                pkg.build_from_source as i32,
                pkg.cellar_path.to_string_lossy(),
                pkg.linked as i32,
                pkg.pinned as i32,
                pkg.source_sha256,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Find a package by name
    pub fn find_by_name(&self, name: &str) -> Result<Option<InstalledPackage>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, version, description, homepage, license, tap,
                   installed_at, updated_at, install_type, build_from_source,
                   cellar_path, linked, pinned, source_sha256
            FROM packages WHERE name = ?1
            "#,
        )?;

        let result = stmt.query_row(params![name], |row| {
            Ok(InstalledPackage {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                version: row.get(2)?,
                description: row.get(3)?,
                homepage: row.get(4)?,
                license: row.get(5)?,
                tap: row.get(6)?,
                installed_at: row.get(7)?,
                updated_at: row.get(8)?,
                install_type: InstallType::from_str(row.get::<_, String>(9)?.as_str())
                    .unwrap_or(InstallType::Formula),
                build_from_source: row.get::<_, i32>(10)? != 0,
                cellar_path: PathBuf::from(row.get::<_, String>(11)?),
                linked: row.get::<_, i32>(12)? != 0,
                pinned: row.get::<_, i32>(13)? != 0,
                source_sha256: row.get(14)?,
            })
        });

        match result {
            Ok(pkg) => Ok(Some(pkg)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all installed packages
    pub fn list_all(&self) -> Result<Vec<PackageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, version, tap, linked, pinned FROM packages ORDER BY name",
        )?;

        let results = stmt
            .query_map([], |row| {
                Ok(PackageSummary {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    tap: row.get(2)?,
                    linked: row.get::<_, i32>(3)? != 0,
                    pinned: row.get::<_, i32>(4)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Check if a package is installed
    pub fn is_installed(&self, name: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM packages WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Count installed packages
    pub fn count(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM packages",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Update a package record
    pub fn update(&self, pkg: &InstalledPackage) -> Result<()> {
        let id = pkg.id.context("Package must have an ID to update")?;
        let now = chrono::Utc::now().timestamp();

        self.conn.execute(
            r#"
            UPDATE packages SET
                version = ?2, description = ?3, homepage = ?4, license = ?5,
                tap = ?6, updated_at = ?7, install_type = ?8, build_from_source = ?9,
                cellar_path = ?10, linked = ?11, pinned = ?12, source_sha256 = ?13
            WHERE id = ?1
            "#,
            params![
                id,
                pkg.version,
                pkg.description,
                pkg.homepage,
                pkg.license,
                pkg.tap,
                now,
                pkg.install_type.as_str(),
                pkg.build_from_source as i32,
                pkg.cellar_path.to_string_lossy(),
                pkg.linked as i32,
                pkg.pinned as i32,
                pkg.source_sha256,
            ],
        )?;

        Ok(())
    }

    /// Set linked status for a package
    pub fn set_linked(&self, name: &str, linked: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE packages SET linked = ?2 WHERE name = ?1",
            params![name, linked as i32],
        )?;
        Ok(())
    }

    /// Set pinned status for a package
    pub fn set_pinned(&self, name: &str, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE packages SET pinned = ?2 WHERE name = ?1",
            params![name, pinned as i32],
        )?;
        Ok(())
    }

    /// Delete a package by name
    pub fn delete(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM packages WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

    // ==================== Dependencies ====================

    /// Add a dependency for a package
    pub fn add_dependency(&self, dep: &PackageDependency) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO package_dependencies (
                package_id, dependency_name, dependency_type, version_constraint, is_satisfied
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                dep.package_id,
                dep.dependency_name,
                dep.dependency_type.as_str(),
                dep.version_constraint,
                dep.is_satisfied as i32,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get dependencies for a package
    pub fn get_dependencies(&self, package_id: i64) -> Result<Vec<PackageDependency>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, package_id, dependency_name, dependency_type, version_constraint, is_satisfied
            FROM package_dependencies WHERE package_id = ?1
            "#,
        )?;

        let results = stmt
            .query_map(params![package_id], |row| {
                Ok(PackageDependency {
                    id: Some(row.get(0)?),
                    package_id: row.get(1)?,
                    dependency_name: row.get(2)?,
                    dependency_type: DependencyType::from_str(row.get::<_, String>(3)?.as_str())
                        .unwrap_or(DependencyType::Runtime),
                    version_constraint: row.get(4)?,
                    is_satisfied: row.get::<_, i32>(5)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get packages that depend on a given package name
    pub fn get_reverse_dependencies(&self, name: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT p.name
            FROM packages p
            JOIN package_dependencies pd ON p.id = pd.package_id
            WHERE pd.dependency_name = ?1
            "#,
        )?;

        let results = stmt
            .query_map(params![name], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    // ==================== Package Files ====================

    /// Add a file for a package
    pub fn add_file(&self, file: &PackageFile) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO package_files (package_id, file_path, file_type, symlink_path)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                file.package_id,
                file.file_path.to_string_lossy(),
                file.file_type.as_str(),
                file.symlink_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all files for a package
    pub fn get_files(&self, package_id: i64) -> Result<Vec<PackageFile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, package_id, file_path, file_type, symlink_path
            FROM package_files WHERE package_id = ?1
            "#,
        )?;

        let results = stmt
            .query_map(params![package_id], |row| {
                Ok(PackageFile {
                    id: Some(row.get(0)?),
                    package_id: row.get(1)?,
                    file_path: PathBuf::from(row.get::<_, String>(2)?),
                    file_type: FileType::from_str(row.get::<_, String>(3)?.as_str())
                        .unwrap_or(FileType::Other),
                    symlink_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    // ==================== History ====================

    /// Record an installation action in history
    pub fn record_history(&self, entry: &InstallHistoryEntry) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO install_history (
                package_name, version, action, performed_at, success, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                entry.package_name,
                entry.version,
                entry.action.as_str(),
                entry.performed_at,
                entry.success as i32,
                entry.error_message,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get recent history entries
    pub fn get_recent_history(&self, limit: i64) -> Result<Vec<InstallHistoryEntry>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, package_name, version, action, performed_at, success, error_message
            FROM install_history ORDER BY performed_at DESC LIMIT ?1
            "#,
        )?;

        let results = stmt
            .query_map(params![limit], |row| {
                Ok(InstallHistoryEntry {
                    id: Some(row.get(0)?),
                    package_name: row.get(1)?,
                    version: row.get(2)?,
                    action: InstallAction::from_str(row.get::<_, String>(3)?.as_str())
                        .unwrap_or(InstallAction::Install),
                    performed_at: row.get(4)?,
                    success: row.get::<_, i32>(5)? != 0,
                    error_message: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations::run_migrations;
    use tempfile::TempDir;

    fn setup_test_db() -> (Connection, TempDir) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        run_migrations(&conn).unwrap();
        (conn, temp)
    }

    #[test]
    fn test_insert_and_find_package() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        let pkg = InstalledPackage::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/curl/8.5.0"),
        );

        let id = repo.insert(&pkg).unwrap();
        assert!(id > 0);

        let found = repo.find_by_name("curl").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "curl");
        assert_eq!(found.version, "8.5.0");
    }

    #[test]
    fn test_list_packages() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        // Insert multiple packages
        for name in &["curl", "wget", "jq"] {
            let pkg = InstalledPackage::new(
                name.to_string(),
                "1.0.0".to_string(),
                PathBuf::from(format!("/opt/brew-rs/Cellar/{}/1.0.0", name)),
            );
            repo.insert(&pkg).unwrap();
        }

        let list = repo.list_all().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "curl"); // Sorted by name
    }

    #[test]
    fn test_delete_package() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        let pkg = InstalledPackage::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/curl/8.5.0"),
        );

        repo.insert(&pkg).unwrap();
        assert!(repo.is_installed("curl").unwrap());

        repo.delete("curl").unwrap();
        assert!(!repo.is_installed("curl").unwrap());
    }

    #[test]
    fn test_dependencies() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        // Insert curl package
        let pkg = InstalledPackage::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/curl/8.5.0"),
        );
        let pkg_id = repo.insert(&pkg).unwrap();

        // Add dependency
        let dep = PackageDependency {
            id: None,
            package_id: pkg_id,
            dependency_name: "openssl".to_string(),
            dependency_type: DependencyType::Runtime,
            version_constraint: Some(">=3.0".to_string()),
            is_satisfied: true,
        };
        repo.add_dependency(&dep).unwrap();

        let deps = repo.get_dependencies(pkg_id).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].dependency_name, "openssl");
    }

    #[test]
    fn test_reverse_dependencies() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        // Insert curl and libcurl
        let curl = InstalledPackage::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/curl/8.5.0"),
        );
        let curl_id = repo.insert(&curl).unwrap();

        // curl depends on openssl
        let dep = PackageDependency {
            id: None,
            package_id: curl_id,
            dependency_name: "openssl".to_string(),
            dependency_type: DependencyType::Runtime,
            version_constraint: None,
            is_satisfied: true,
        };
        repo.add_dependency(&dep).unwrap();

        // Get reverse deps for openssl
        let rdeps = repo.get_reverse_dependencies("openssl").unwrap();
        assert_eq!(rdeps.len(), 1);
        assert_eq!(rdeps[0], "curl");
    }

    #[test]
    fn test_history() {
        let (conn, _temp) = setup_test_db();
        let repo = PackageRepository::new(&conn);

        let entry = InstallHistoryEntry::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            InstallAction::Install,
            true,
        );
        repo.record_history(&entry).unwrap();

        let history = repo.get_recent_history(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].package_name, "curl");
        assert_eq!(history[0].action, InstallAction::Install);
    }
}
