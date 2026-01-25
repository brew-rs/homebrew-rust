//! Tap persistence - save and load taps from TOML configuration
//!
//! This module handles persistent storage of tap configurations in
//! `~/.config/brew-rs/taps.toml`, allowing taps to survive across
//! process restarts.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Default schema version for new registries
fn default_version() -> u32 {
    1
}

/// Default value for enabled field
fn default_enabled() -> bool {
    true
}

/// Persistent tap registry stored in taps.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapRegistry {
    /// Schema version for future migrations
    #[serde(default = "default_version")]
    pub version: u32,

    /// List of registered taps
    #[serde(default)]
    pub taps: Vec<TapEntry>,
}

impl Default for TapRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            taps: Vec::new(),
        }
    }
}

/// A single tap entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapEntry {
    /// Tap name in "user/repo" format
    pub name: String,

    /// Git repository URL
    pub url: String,

    /// When the tap was added
    #[serde(default)]
    pub added_at: Option<DateTime<Utc>>,

    /// Last successful update timestamp
    #[serde(default)]
    pub last_updated: Option<DateTime<Utc>>,

    /// Current git commit hash (for cache invalidation)
    #[serde(default)]
    pub commit_hash: Option<String>,

    /// Whether the tap is enabled (for soft-disable without removing)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl TapEntry {
    /// Create a new tap entry
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            added_at: Some(Utc::now()),
            last_updated: None,
            commit_hash: None,
            enabled: true,
        }
    }
}

impl TapRegistry {
    /// Load tap registry from file
    ///
    /// Returns a default empty registry if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read taps.toml from {}", path.display()))?;

        let registry: TapRegistry = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse taps.toml from {}", path.display()))?;

        Ok(registry)
    }

    /// Save tap registry to file
    ///
    /// Uses atomic write (temp file + rename) to prevent corruption.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize tap registry")?;

        // Atomic write via temp file + rename
        let temp_path = path.with_extension("toml.tmp");
        fs::write(&temp_path, &contents)
            .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;

        fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;

        Ok(())
    }

    /// Add a new tap entry
    ///
    /// Returns an error if a tap with the same name already exists.
    pub fn add(&mut self, name: String, url: String) -> Result<()> {
        if self.taps.iter().any(|t| t.name == name) {
            anyhow::bail!("Tap {} already exists", name);
        }

        self.taps.push(TapEntry::new(name, url));
        Ok(())
    }

    /// Remove a tap entry by name
    ///
    /// Returns the removed entry, or an error if not found.
    pub fn remove(&mut self, name: &str) -> Result<TapEntry> {
        let pos = self
            .taps
            .iter()
            .position(|t| t.name == name)
            .ok_or_else(|| anyhow::anyhow!("Tap {} not found", name))?;

        Ok(self.taps.remove(pos))
    }

    /// Get a tap entry by name
    pub fn get(&self, name: &str) -> Option<&TapEntry> {
        self.taps.iter().find(|t| t.name == name)
    }

    /// Get a mutable tap entry by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut TapEntry> {
        self.taps.iter_mut().find(|t| t.name == name)
    }

    /// Update commit hash and last_updated timestamp for a tap
    pub fn update_commit(&mut self, name: &str, commit: String) {
        if let Some(entry) = self.get_mut(name) {
            entry.commit_hash = Some(commit);
            entry.last_updated = Some(Utc::now());
        }
    }

    /// Check if a tap exists in the registry
    pub fn contains(&self, name: &str) -> bool {
        self.taps.iter().any(|t| t.name == name)
    }

    /// Get all enabled taps
    pub fn enabled_taps(&self) -> impl Iterator<Item = &TapEntry> {
        self.taps.iter().filter(|t| t.enabled)
    }

    /// Get the number of taps
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tap_entry_creation() {
        let entry = TapEntry::new(
            "brew-rs/core".to_string(),
            "https://github.com/brew-rs/core.git".to_string(),
        );

        assert_eq!(entry.name, "brew-rs/core");
        assert_eq!(entry.url, "https://github.com/brew-rs/core.git");
        assert!(entry.added_at.is_some());
        assert!(entry.enabled);
        assert!(entry.commit_hash.is_none());
        assert!(entry.last_updated.is_none());
    }

    #[test]
    fn test_tap_registry_add_remove() {
        let mut registry = TapRegistry::default();

        // Add a tap
        registry
            .add(
                "brew-rs/core".to_string(),
                "https://github.com/brew-rs/core.git".to_string(),
            )
            .unwrap();

        assert_eq!(registry.len(), 1);
        assert!(registry.contains("brew-rs/core"));

        // Try to add duplicate
        let result = registry.add(
            "brew-rs/core".to_string(),
            "https://github.com/brew-rs/core.git".to_string(),
        );
        assert!(result.is_err());

        // Remove the tap
        let removed = registry.remove("brew-rs/core").unwrap();
        assert_eq!(removed.name, "brew-rs/core");
        assert!(registry.is_empty());

        // Try to remove non-existent
        let result = registry.remove("nonexistent/tap");
        assert!(result.is_err());
    }

    #[test]
    fn test_tap_registry_persistence() {
        let temp = TempDir::new().unwrap();
        let taps_file = temp.path().join("taps.toml");

        // Create and save a registry
        let mut registry = TapRegistry::default();
        registry
            .add(
                "brew-rs/core".to_string(),
                "https://github.com/brew-rs/core.git".to_string(),
            )
            .unwrap();
        registry
            .add(
                "my-org/custom".to_string(),
                "git@github.com:my-org/homebrew-custom.git".to_string(),
            )
            .unwrap();

        // Update commit for one tap
        registry.update_commit("brew-rs/core", "abc123def456".to_string());

        registry.save(&taps_file).unwrap();

        // Load it back
        let loaded = TapRegistry::load(&taps_file).unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains("brew-rs/core"));
        assert!(loaded.contains("my-org/custom"));

        let core = loaded.get("brew-rs/core").unwrap();
        assert_eq!(core.commit_hash.as_deref(), Some("abc123def456"));
        assert!(core.last_updated.is_some());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        let taps_file = temp.path().join("nonexistent.toml");

        let registry = TapRegistry::load(&taps_file).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_enabled_taps_filter() {
        let mut registry = TapRegistry::default();

        registry
            .add("tap1".to_string(), "https://example.com/1.git".to_string())
            .unwrap();
        registry
            .add("tap2".to_string(), "https://example.com/2.git".to_string())
            .unwrap();

        // Disable one tap
        if let Some(tap) = registry.get_mut("tap1") {
            tap.enabled = false;
        }

        let enabled: Vec<_> = registry.enabled_taps().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "tap2");
    }
}
