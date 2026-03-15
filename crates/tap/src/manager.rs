//! Tap manager for handling multiple taps with persistence

use crate::cache::FormulaCache;
use crate::persistence::{TapEntry, TapRegistry};
use crate::{parse_tap_name, Tap, TapError};
use anyhow::{Context, Result};
use brew_config::Paths;
use brew_formula::Formula;
use std::collections::HashMap;
use std::fs;
use tracing::{info, warn};

/// Manages multiple taps with persistent configuration
pub struct TapManager {
    paths: Paths,
    registry: TapRegistry,
    taps: HashMap<String, Tap>,
    cache: FormulaCache,
}

impl TapManager {
    /// Create a new tap manager and load persisted taps
    ///
    /// This will:
    /// 1. Load the tap registry from `~/.config/brew-rs/taps.toml`
    /// 2. Initialize Tap objects for all enabled taps
    /// 3. Migrate any legacy taps found on disk but not in registry
    /// 4. Initialize formula cache and rebuild stale caches
    pub fn new(paths: Paths) -> Result<Self> {
        let registry = TapRegistry::load(&paths.taps_file)
            .context("Failed to load tap registry")?;

        let cache = FormulaCache::open(&paths.formula_cache_file)
            .context("Failed to open formula cache")?;

        let mut manager = Self {
            paths,
            registry,
            taps: HashMap::new(),
            cache,
        };

        // Load all enabled taps into memory
        manager.load_enabled_taps()?;

        // Migrate any legacy taps not in the registry
        manager.migrate_legacy_taps()?;

        // Ensure cache is valid for all taps
        manager.sync_cache()?;

        Ok(manager)
    }

    /// Sync the formula cache with all taps
    fn sync_cache(&mut self) -> Result<()> {
        for (name, tap) in &self.taps {
            // Get current commit hash
            let current_commit = match tap.get_head_commit() {
                Ok(commit) => commit,
                Err(_) => continue,
            };

            // Rebuild cache if stale
            if !self.cache.is_valid_for_tap(name, &current_commit) {
                info!("Cache stale for tap {}, rebuilding...", name);
                if let Err(e) = self.cache.rebuild_for_tap(tap) {
                    warn!("Failed to rebuild cache for tap {}: {}", name, e);
                }
            }
        }
        Ok(())
    }

    /// Load all enabled taps from registry into memory
    fn load_enabled_taps(&mut self) -> Result<()> {
        for entry in self.registry.enabled_taps() {
            match self.load_tap_from_entry(entry) {
                Ok(tap) => {
                    self.taps.insert(entry.name.clone(), tap);
                }
                Err(e) => {
                    warn!("Failed to load tap {}: {}", entry.name, e);
                }
            }
        }
        Ok(())
    }

    /// Create a Tap object from a registry entry
    fn load_tap_from_entry(&self, entry: &TapEntry) -> Result<Tap> {
        let (user, repo) = parse_tap_name(&entry.name)?;
        let tap_path = self.paths.tap_dir(&user, &repo);
        Ok(Tap::new(entry.name.clone(), entry.url.clone(), tap_path))
    }

    /// Migrate legacy taps found on disk but not in registry
    ///
    /// This handles upgrades from versions without persistence.
    fn migrate_legacy_taps(&mut self) -> Result<()> {
        if !self.paths.taps_dir.exists() {
            return Ok(());
        }

        let mut migrated = false;

        // Scan for existing tap directories
        for user_dir in fs::read_dir(&self.paths.taps_dir)
            .context("Failed to read taps directory")?
        {
            let user_dir = user_dir?;
            if !user_dir.path().is_dir() {
                continue;
            }

            let user = user_dir.file_name().to_string_lossy().to_string();

            for repo_dir in fs::read_dir(user_dir.path())? {
                let repo_dir = repo_dir?;
                if !repo_dir.path().is_dir() {
                    continue;
                }

                // Check if it's a git repository
                if !repo_dir.path().join(".git").exists() {
                    continue;
                }

                let repo = repo_dir.file_name().to_string_lossy().to_string();
                let tap_name = format!("{}/{}", user, repo);

                // Skip if already in registry
                if self.registry.contains(&tap_name) {
                    continue;
                }

                // Try to get the origin URL from git
                if let Ok(git_repo) = git2::Repository::open(repo_dir.path()) {
                    if let Ok(remote) = git_repo.find_remote("origin") {
                        if let Some(url) = remote.url() {
                            info!("Migrating legacy tap: {}", tap_name);

                            // Add to registry
                            self.registry.add(tap_name.clone(), url.to_string())?;

                            // Create Tap object
                            let tap = Tap::new(
                                tap_name.clone(),
                                url.to_string(),
                                repo_dir.path().to_path_buf(),
                            );

                            // Get commit hash
                            if let Ok(commit) = tap.get_head_commit() {
                                self.registry.update_commit(&tap_name, commit);
                            }

                            self.taps.insert(tap_name, tap);
                            migrated = true;
                        }
                    }
                }
            }
        }

        // Save registry if we migrated any taps
        if migrated {
            self.registry.save(&self.paths.taps_file)?;
        }

        Ok(())
    }

    /// Add a tap with the given name and URL
    ///
    /// This will:
    /// 1. Clone the git repository
    /// 2. Add the tap to the registry
    /// 3. Save the registry to disk
    pub fn add_tap(&mut self, name: &str, url: &str) -> Result<()> {
        // Check if already exists
        if self.registry.contains(name) {
            anyhow::bail!(TapError::AlreadyExists(name.to_string()));
        }

        let (user, repo) = parse_tap_name(name)?;
        let tap_path = self.paths.tap_dir(&user, &repo);

        let tap = Tap::new(name.to_string(), url.to_string(), tap_path);

        // Clone the repository
        tap.clone_if_needed()?;

        // Get initial commit hash
        let commit_hash = tap.get_head_commit().ok();

        // Add to registry
        self.registry.add(name.to_string(), url.to_string())?;

        // Update commit hash if we got one
        if let Some(commit) = commit_hash {
            self.registry.update_commit(name, commit);
        }

        // Save registry
        self.registry.save(&self.paths.taps_file)?;

        // Add to in-memory map
        self.taps.insert(name.to_string(), tap.clone());

        // Rebuild cache for new tap
        if let Err(e) = self.cache.rebuild_for_tap(&tap) {
            warn!("Failed to build cache for tap {}: {}", name, e);
        }

        info!("Added tap: {}", name);
        Ok(())
    }

    /// Remove a tap
    ///
    /// This will:
    /// 1. Remove the tap from the registry
    /// 2. Delete the tap directory from disk
    /// 3. Save the updated registry
    pub fn remove_tap(&mut self, name: &str) -> Result<()> {
        // Remove from registry (will error if not found)
        self.registry.remove(name)?;

        // Remove tap directory from disk
        let (user, repo) = parse_tap_name(name)?;
        let tap_path = self.paths.tap_dir(&user, &repo);

        if tap_path.exists() {
            fs::remove_dir_all(&tap_path)
                .with_context(|| format!("Failed to remove tap directory: {}", tap_path.display()))?;

            // Also remove user directory if empty
            if let Some(user_dir) = tap_path.parent() {
                if user_dir.exists() {
                    if let Ok(entries) = fs::read_dir(user_dir) {
                        if entries.count() == 0 {
                            let _ = fs::remove_dir(user_dir);
                        }
                    }
                }
            }
        }

        // Save registry
        self.registry.save(&self.paths.taps_file)?;

        // Remove from in-memory map
        self.taps.remove(name);

        // Remove from cache
        if let Err(e) = self.cache.remove_tap(name) {
            warn!("Failed to remove tap {} from cache: {}", name, e);
        }

        info!("Removed tap: {}", name);
        Ok(())
    }

    /// Update a specific tap
    ///
    /// This will:
    /// 1. Git pull the tap repository
    /// 2. Update the commit hash in the registry
    /// 3. Save the updated registry
    /// 4. Rebuild formula cache
    pub fn update_tap(&mut self, name: &str) -> Result<()> {
        let tap = self
            .taps
            .get(name)
            .ok_or_else(|| TapError::NotFound(name.to_string()))?
            .clone();

        // Update the git repository
        tap.update()?;

        // Update commit hash in registry
        if let Ok(commit) = tap.get_head_commit() {
            self.registry.update_commit(name, commit);
            self.registry.save(&self.paths.taps_file)?;
        }

        // Rebuild cache for updated tap
        if let Err(e) = self.cache.rebuild_for_tap(&tap) {
            warn!("Failed to rebuild cache for tap {}: {}", name, e);
        }

        info!("Updated tap: {}", name);
        Ok(())
    }

    /// Update all taps
    pub fn update_all(&mut self) -> Result<()> {
        info!("Updating all taps");

        let tap_names: Vec<String> = self.taps.keys().cloned().collect();

        for name in tap_names {
            if let Err(e) = self.update_tap(&name) {
                warn!("Failed to update tap {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// List all registered taps with their metadata
    pub fn list_taps(&self) -> Vec<&TapEntry> {
        self.registry.taps.iter().collect()
    }

    /// Get the number of registered taps
    pub fn tap_count(&self) -> usize {
        self.registry.len()
    }

    /// Find a formula across all taps
    ///
    /// Uses the formula cache for O(1) lookup when available.
    pub fn find_formula(&self, name: &str) -> Result<Formula> {
        // Try cache first for fast lookup
        if let Some(entry) = self.cache.get_by_name(name) {
            if let Some(tap) = self.taps.get(&entry.tap_name) {
                if let Ok(formula) = tap.load_formula(name) {
                    return Ok(formula);
                }
            }
        }

        // Fallback to scanning all taps
        for tap in self.taps.values() {
            if let Ok(formula) = tap.load_formula(name) {
                return Ok(formula);
            }
        }

        anyhow::bail!("Formula {} not found in any tap", name);
    }

    /// Search for formulas matching a query
    ///
    /// Uses the formula cache with FTS5 for fast full-text search.
    pub fn search(&self, query: &str) -> Result<Vec<(String, String)>> {
        // Use cache for fast search
        let results = self.cache.search(query)?;
        Ok(results
            .into_iter()
            .map(|entry| (entry.tap_name, entry.name))
            .collect())
    }

    /// Search for formulas with full metadata
    pub fn search_with_details(&self, query: &str) -> Result<Vec<crate::FormulaCacheEntry>> {
        self.cache.search(query)
    }

    /// List all available formulas across all taps
    pub fn list_all_formulas(&self) -> Result<Vec<(String, String)>> {
        let mut results = Vec::new();

        for (tap_name, tap) in &self.taps {
            if let Ok(formulas) = tap.list_formulas() {
                for formula_name in formulas {
                    results.push((tap_name.clone(), formula_name));
                }
            }
        }

        Ok(results)
    }

    /// Get all tap names
    pub fn tap_names(&self) -> Vec<String> {
        self.taps.keys().cloned().collect()
    }

    /// Get a reference to a tap by name
    pub fn get_tap(&self, name: &str) -> Option<&Tap> {
        self.taps.get(name)
    }

    /// Check if a tap is registered
    pub fn has_tap(&self, name: &str) -> bool {
        self.registry.contains(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> Paths {
        let mut paths = Paths::new().unwrap();
        paths.taps_dir = temp.path().join("taps");
        paths.config_dir = temp.path().join("config");
        paths.cache_dir = temp.path().join("cache");
        paths.taps_file = temp.path().join("config/taps.toml");
        paths.formula_cache_file = temp.path().join("cache/formula_cache.db");
        paths
    }

    #[test]
    fn test_tap_manager_creation() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        let manager = TapManager::new(paths).unwrap();
        assert_eq!(manager.tap_count(), 0);
    }

    #[test]
    fn test_tap_manager_empty_list() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        let manager = TapManager::new(paths).unwrap();
        let taps = manager.list_taps();
        assert!(taps.is_empty());
    }

    #[test]
    fn test_registry_persistence() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);

        // Create a registry directly and save it
        let mut registry = TapRegistry::default();
        registry
            .add(
                "test/tap".to_string(),
                "https://example.com/test.git".to_string(),
            )
            .unwrap();

        // Create config directory and save
        fs::create_dir_all(&paths.config_dir).unwrap();
        registry.save(&paths.taps_file).unwrap();

        // Load with TapManager
        let manager = TapManager::new(paths).unwrap();

        // The tap should be in the registry but not loaded (no git repo)
        assert!(manager.has_tap("test/tap"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_load_real_tap() {
        let paths = Paths::new().unwrap();
        let manager = TapManager::new(paths).unwrap();

        // List all taps
        let taps = manager.list_taps();
        println!("Found {} taps:", taps.len());
        for tap in taps {
            println!("  {} ({})", tap.name, tap.url);
            if let Some(updated) = &tap.last_updated {
                println!("    Last updated: {}", updated);
            }
            if let Some(commit) = &tap.commit_hash {
                println!("    Commit: {}", &commit[..8.min(commit.len())]);
            }
        }

        // Try to find curl if we have taps
        if manager.tap_count() > 0 {
            if let Ok(formula) = manager.find_formula("curl") {
                println!("Found curl version: {}", formula.package.version);
            }
        }
    }
}
