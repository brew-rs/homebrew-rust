//! Formula cache with SQLite FTS5 for fast text search
//!
//! This module provides:
//! - SQLite-backed persistent cache with FTS5 full-text search
//! - In-memory HashMap for O(1) lookups by name
//! - Cache invalidation based on tap commit hashes

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

use crate::Tap;

/// A cached formula entry with essential metadata
#[derive(Debug, Clone)]
pub struct FormulaCacheEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub tap_name: String,
    pub formula_path: String,
}

/// Formula cache with two-tier lookup:
/// 1. In-memory HashMap for O(1) name lookups
/// 2. SQLite FTS5 for full-text search
pub struct FormulaCache {
    conn: Connection,
    by_name: HashMap<String, FormulaCacheEntry>,
    sorted_names: Vec<String>,
    commit_validity: HashMap<String, String>,
}

impl FormulaCache {
    /// Open or create a formula cache at the given path
    pub fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open formula cache: {}", path.display()))?;

        // Enable WAL mode for better concurrent access
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // Initialize schema
        Self::init_schema(&conn)?;

        let mut cache = Self {
            conn,
            by_name: HashMap::new(),
            sorted_names: Vec::new(),
            commit_validity: HashMap::new(),
        };

        // Load existing data into memory
        cache.load_into_memory()?;

        Ok(cache)
    }

    /// Initialize the SQLite schema
    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- Main formula table
            CREATE TABLE IF NOT EXISTS formulas (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                description TEXT,
                tap_name TEXT NOT NULL,
                formula_path TEXT NOT NULL,
                UNIQUE(tap_name, name)
            );

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS formulas_fts USING fts5(
                name,
                description,
                content='formulas',
                content_rowid='id'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS formulas_ai AFTER INSERT ON formulas BEGIN
                INSERT INTO formulas_fts(rowid, name, description)
                VALUES (new.id, new.name, new.description);
            END;

            CREATE TRIGGER IF NOT EXISTS formulas_ad AFTER DELETE ON formulas BEGIN
                INSERT INTO formulas_fts(formulas_fts, rowid, name, description)
                VALUES ('delete', old.id, old.name, old.description);
            END;

            CREATE TRIGGER IF NOT EXISTS formulas_au AFTER UPDATE ON formulas BEGIN
                INSERT INTO formulas_fts(formulas_fts, rowid, name, description)
                VALUES ('delete', old.id, old.name, old.description);
                INSERT INTO formulas_fts(rowid, name, description)
                VALUES (new.id, new.name, new.description);
            END;

            -- Tap commit tracking for cache invalidation
            CREATE TABLE IF NOT EXISTS tap_commits (
                tap_name TEXT PRIMARY KEY,
                commit_hash TEXT NOT NULL
            );

            -- Indices
            CREATE INDEX IF NOT EXISTS idx_formulas_name ON formulas(name);
            CREATE INDEX IF NOT EXISTS idx_formulas_tap ON formulas(tap_name);
            "#,
        )
        .context("Failed to initialize formula cache schema")?;

        Ok(())
    }

    /// Load all formulas from SQLite into memory
    fn load_into_memory(&mut self) -> Result<()> {
        self.by_name.clear();
        self.sorted_names.clear();
        self.commit_validity.clear();

        // Load formulas
        let mut stmt = self.conn.prepare(
            "SELECT name, version, description, tap_name, formula_path FROM formulas ORDER BY name",
        )?;

        let entries = stmt.query_map([], |row| {
            Ok(FormulaCacheEntry {
                name: row.get(0)?,
                version: row.get(1)?,
                description: row.get(2)?,
                tap_name: row.get(3)?,
                formula_path: row.get(4)?,
            })
        })?;

        for entry in entries {
            let entry = entry?;
            self.sorted_names.push(entry.name.clone());
            self.by_name.insert(entry.name.clone(), entry);
        }

        // Load tap commits
        let mut stmt = self.conn.prepare("SELECT tap_name, commit_hash FROM tap_commits")?;
        let commits = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;

        for commit in commits {
            let (tap_name, hash) = commit?;
            self.commit_validity.insert(tap_name, hash);
        }

        debug!(
            "Loaded {} formulas from cache",
            self.by_name.len()
        );

        Ok(())
    }

    /// Check if the cache is valid for a tap based on commit hash
    pub fn is_valid_for_tap(&self, tap_name: &str, current_commit: &str) -> bool {
        self.commit_validity
            .get(tap_name)
            .map(|cached| cached == current_commit)
            .unwrap_or(false)
    }

    /// Rebuild the cache for a specific tap
    pub fn rebuild_for_tap(&mut self, tap: &Tap) -> Result<()> {
        info!("Rebuilding formula cache for tap: {}", tap.name);

        // Get current commit hash
        let commit_hash = tap.get_head_commit().unwrap_or_else(|_| "unknown".to_string());

        // Delete existing entries for this tap
        self.conn.execute(
            "DELETE FROM formulas WHERE tap_name = ?1",
            params![tap.name],
        )?;

        // Load all formulas from the tap
        let formulas = tap.list_formulas().unwrap_or_default();
        let formulas_dir = tap.formulas_dir();

        let mut insert_count = 0;
        for formula_name in formulas {
            let formula_path = formulas_dir.join(format!("{}.toml", formula_name));

            // Try to load the formula to get metadata
            match tap.load_formula(&formula_name) {
                Ok(formula) => {
                    self.conn.execute(
                        "INSERT OR REPLACE INTO formulas (name, version, description, tap_name, formula_path)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            formula.package.name,
                            formula.package.version,
                            formula.package.description,
                            tap.name,
                            formula_path.to_string_lossy(),
                        ],
                    )?;
                    insert_count += 1;
                }
                Err(e) => {
                    warn!("Failed to load formula {}: {}", formula_name, e);
                }
            }
        }

        // Update commit hash
        self.conn.execute(
            "INSERT OR REPLACE INTO tap_commits (tap_name, commit_hash) VALUES (?1, ?2)",
            params![tap.name, commit_hash],
        )?;

        info!("Cached {} formulas from tap {}", insert_count, tap.name);

        // Reload into memory
        self.load_into_memory()?;

        Ok(())
    }

    /// Remove all cached entries for a tap
    pub fn remove_tap(&mut self, tap_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM formulas WHERE tap_name = ?1",
            params![tap_name],
        )?;

        self.conn.execute(
            "DELETE FROM tap_commits WHERE tap_name = ?1",
            params![tap_name],
        )?;

        // Reload into memory
        self.load_into_memory()?;

        Ok(())
    }

    /// Get a formula by exact name - O(1) lookup
    pub fn get_by_name(&self, name: &str) -> Option<&FormulaCacheEntry> {
        self.by_name.get(name)
    }

    /// Search for formulas by name prefix - O(log n) binary search
    pub fn search_prefix(&self, prefix: &str) -> Vec<&FormulaCacheEntry> {
        let prefix_lower = prefix.to_lowercase();

        // Binary search to find starting position
        let start = self.sorted_names.partition_point(|name| {
            name.to_lowercase() < prefix_lower
        });

        // Collect matches
        self.sorted_names[start..]
            .iter()
            .take_while(|name| name.to_lowercase().starts_with(&prefix_lower))
            .filter_map(|name| self.by_name.get(name))
            .collect()
    }

    /// Full-text search using FTS5
    pub fn search_fts(&self, query: &str) -> Result<Vec<FormulaCacheEntry>> {
        // Escape special FTS5 characters and build query
        let fts_query = format!("{}*", query.replace('"', "\"\""));

        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.name, f.version, f.description, f.tap_name, f.formula_path
            FROM formulas f
            JOIN formulas_fts fts ON f.id = fts.rowid
            WHERE formulas_fts MATCH ?1
            ORDER BY rank
            LIMIT 50
            "#,
        )?;

        let results = stmt
            .query_map(params![fts_query], |row| {
                Ok(FormulaCacheEntry {
                    name: row.get(0)?,
                    version: row.get(1)?,
                    description: row.get(2)?,
                    tap_name: row.get(3)?,
                    formula_path: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Search combining prefix and FTS (union of results)
    pub fn search(&self, query: &str) -> Result<Vec<FormulaCacheEntry>> {
        let query_lower = query.to_lowercase();
        let mut results: HashMap<String, FormulaCacheEntry> = HashMap::new();

        // First, add prefix matches (highest priority)
        for entry in self.search_prefix(query) {
            results.insert(entry.name.clone(), entry.clone());
        }

        // Then add FTS matches
        if let Ok(fts_results) = self.search_fts(query) {
            for entry in fts_results {
                results.entry(entry.name.clone()).or_insert(entry);
            }
        }

        // Also do a substring match on description for fuzzy results
        for entry in self.by_name.values() {
            if entry.description.to_lowercase().contains(&query_lower) {
                results.entry(entry.name.clone()).or_insert_with(|| entry.clone());
            }
        }

        // Sort results: exact match first, then prefix match, then others
        let mut sorted: Vec<_> = results.into_values().collect();
        sorted.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == query_lower;
            let b_exact = b.name.to_lowercase() == query_lower;
            let a_prefix = a.name.to_lowercase().starts_with(&query_lower);
            let b_prefix = b.name.to_lowercase().starts_with(&query_lower);

            match (a_exact, b_exact) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match (a_prefix, b_prefix) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                },
            }
        });

        Ok(sorted)
    }

    /// Get the total number of cached formulas
    pub fn formula_count(&self) -> usize {
        self.by_name.len()
    }

    /// Get all formula names
    pub fn all_names(&self) -> &[String] {
        &self.sorted_names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_cache() -> (FormulaCache, TempDir) {
        let temp = TempDir::new().unwrap();
        let cache_path = temp.path().join("formula_cache.db");
        let cache = FormulaCache::open(&cache_path).unwrap();
        (cache, temp)
    }

    #[test]
    fn test_cache_creation() {
        let (cache, _temp) = create_test_cache();
        assert_eq!(cache.formula_count(), 0);
    }

    #[test]
    fn test_in_memory_lookup() {
        let (mut cache, _temp) = create_test_cache();

        // Insert directly for testing
        cache.conn.execute(
            "INSERT INTO formulas (name, version, description, tap_name, formula_path)
             VALUES ('curl', '8.5.0', 'Transfer data with URLs', 'brew-rs/core', '/path/to/curl.toml')",
            [],
        ).unwrap();

        cache.load_into_memory().unwrap();

        let entry = cache.get_by_name("curl");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().version, "8.5.0");
    }

    #[test]
    fn test_prefix_search() {
        let (mut cache, _temp) = create_test_cache();

        // Insert test data
        cache.conn.execute_batch(
            "INSERT INTO formulas (name, version, description, tap_name, formula_path) VALUES
             ('curl', '8.5.0', 'Transfer data with URLs', 'test/tap', '/path/curl.toml'),
             ('curlie', '1.7.0', 'Power of curl, ease of use', 'test/tap', '/path/curlie.toml'),
             ('wget', '1.21', 'Internet file retriever', 'test/tap', '/path/wget.toml');"
        ).unwrap();

        cache.load_into_memory().unwrap();

        let results = cache.search_prefix("cur");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|e| e.name == "curl"));
        assert!(results.iter().any(|e| e.name == "curlie"));
    }

    #[test]
    fn test_fts_search() {
        let (mut cache, _temp) = create_test_cache();

        // Insert test data
        cache.conn.execute_batch(
            "INSERT INTO formulas (name, version, description, tap_name, formula_path) VALUES
             ('curl', '8.5.0', 'Transfer data with URLs', 'test/tap', '/path/curl.toml'),
             ('wget', '1.21', 'Internet file retriever', 'test/tap', '/path/wget.toml'),
             ('jq', '1.7', 'Lightweight JSON processor', 'test/tap', '/path/jq.toml');"
        ).unwrap();

        cache.load_into_memory().unwrap();

        // Search for "json" should find jq
        let results = cache.search_fts("json").unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|e| e.name == "jq"));
    }

    #[test]
    fn test_combined_search() {
        let (mut cache, _temp) = create_test_cache();

        cache.conn.execute_batch(
            "INSERT INTO formulas (name, version, description, tap_name, formula_path) VALUES
             ('curl', '8.5.0', 'Transfer data with URLs', 'test/tap', '/path/curl.toml'),
             ('jq', '1.7', 'JSON processor with curl-like syntax', 'test/tap', '/path/jq.toml');"
        ).unwrap();

        cache.load_into_memory().unwrap();

        // Searching "curl" should find both (exact match + description match)
        let results = cache.search("curl").unwrap();
        assert_eq!(results.len(), 2);
        // curl should be first (exact match)
        assert_eq!(results[0].name, "curl");
    }

    #[test]
    fn test_cache_validity() {
        let (mut cache, _temp) = create_test_cache();

        cache.conn.execute(
            "INSERT INTO tap_commits (tap_name, commit_hash) VALUES ('test/tap', 'abc123')",
            [],
        ).unwrap();

        cache.load_into_memory().unwrap();

        assert!(cache.is_valid_for_tap("test/tap", "abc123"));
        assert!(!cache.is_valid_for_tap("test/tap", "different"));
        assert!(!cache.is_valid_for_tap("other/tap", "abc123"));
    }
}
