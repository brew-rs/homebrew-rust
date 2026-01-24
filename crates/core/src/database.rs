//! Database for tracking installed packages and their state

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Create a new database connection
    pub fn new() -> Result<Self> {
        // TODO: Use proper config directory
        let db_path = Self::get_db_path();
        let conn = Connection::open(db_path)?;

        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS packages (
                name TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                installed_at INTEGER NOT NULL,
                checksum TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    fn get_db_path() -> PathBuf {
        // TODO: Use proper XDG config directory
        PathBuf::from("/tmp/brew-rs.db")
    }

    /// Record a package installation
    pub fn install_package(&self, name: &str, version: &str, checksum: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT OR REPLACE INTO packages (name, version, installed_at, checksum) VALUES (?1, ?2, ?3, ?4)",
            params![name, version, now, checksum],
        )?;
        Ok(())
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
}
