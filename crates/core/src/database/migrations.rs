//! Database migration system
//!
//! Handles schema versioning and migrations for the package database.

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::{debug, info};

/// Current schema version
pub const CURRENT_VERSION: u32 = 1;

/// List of migrations in order
const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", include_str!("migrations/001_initial.sql")),
];

/// Run all pending migrations
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Create schema_version table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL,
            description TEXT
        )",
        [],
    )
    .context("Failed to create schema_version table")?;

    let current_version = get_current_version(conn)?;
    debug!("Current schema version: {}", current_version);

    if current_version >= CURRENT_VERSION {
        debug!("Database schema is up to date");
        return Ok(());
    }

    // Run pending migrations
    for (i, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as u32;

        if version <= current_version {
            continue;
        }

        info!("Running migration {} (v{})", name, version);

        // Run migration in a transaction
        let tx = conn.unchecked_transaction()?;

        tx.execute_batch(sql)
            .with_context(|| format!("Failed to run migration {}", name))?;

        // Record the migration
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO schema_version (version, applied_at, description) VALUES (?1, ?2, ?3)",
            rusqlite::params![version, now, *name],
        )?;

        tx.commit()?;
        info!("Migration {} completed successfully", name);
    }

    Ok(())
}

/// Get the current schema version
fn get_current_version(conn: &Connection) -> Result<u32> {
    let result: Result<u32, _> = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(version) => Ok(version),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

/// Check if any migrations are pending
pub fn has_pending_migrations(conn: &Connection) -> Result<bool> {
    let current = get_current_version(conn)?;
    Ok(current < CURRENT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_run_migrations() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();

        // Run migrations
        run_migrations(&conn).unwrap();

        // Check version
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"packages".to_string()));
        assert!(tables.contains(&"package_files".to_string()));
        assert!(tables.contains(&"package_dependencies".to_string()));
        assert!(tables.contains(&"install_history".to_string()));
        assert!(tables.contains(&"schema_version".to_string()));
    }

    #[test]
    fn test_idempotent_migrations() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();

        // Run migrations twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Should still be at current version
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }

    #[test]
    fn test_pending_migrations() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();

        // Fresh database should have pending migrations
        conn.execute(
            "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at INTEGER, description TEXT)",
            [],
        ).unwrap();

        assert!(has_pending_migrations(&conn).unwrap());

        // After running, no more pending
        run_migrations(&conn).unwrap();
        assert!(!has_pending_migrations(&conn).unwrap());
    }
}
