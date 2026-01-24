//! Tap manager for handling multiple taps

use crate::{parse_tap_name, Tap, TapError};
use anyhow::Result;
use brew_config::Paths;
use brew_formula::Formula;
use std::collections::HashMap;
use tracing::info;

/// Manages multiple taps
pub struct TapManager {
    paths: Paths,
    taps: HashMap<String, Tap>,
}

impl TapManager {
    /// Create a new tap manager
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            taps: HashMap::new(),
        }
    }

    /// Add a tap with the given name and URL
    pub fn add_tap(&mut self, name: &str, url: &str) -> Result<()> {
        if self.taps.contains_key(name) {
            anyhow::bail!(TapError::AlreadyExists(name.to_string()));
        }

        let (user, repo) = parse_tap_name(name)?;
        let tap_path = self.paths.tap_dir(&user, &repo);

        let tap = Tap::new(name.to_string(), url.to_string(), tap_path);
        tap.clone_if_needed()?;

        self.taps.insert(name.to_string(), tap);
        info!("Added tap: {}", name);

        Ok(())
    }

    /// Load an existing tap from disk
    pub fn load_tap(&mut self, name: &str, url: &str) -> Result<()> {
        let (user, repo) = parse_tap_name(name)?;
        let tap_path = self.paths.tap_dir(&user, &repo);

        let tap = Tap::new(name.to_string(), url.to_string(), tap_path);
        self.taps.insert(name.to_string(), tap);

        Ok(())
    }

    /// Update a specific tap
    pub fn update_tap(&self, name: &str) -> Result<()> {
        let tap = self.taps.get(name)
            .ok_or_else(|| TapError::NotFound(name.to_string()))?;

        tap.update()
    }

    /// Update all taps
    pub fn update_all(&self) -> Result<()> {
        info!("Updating all taps");

        for (name, tap) in &self.taps {
            if let Err(e) = tap.update() {
                eprintln!("Warning: Failed to update tap {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Find a formula across all taps
    pub fn find_formula(&self, name: &str) -> Result<Formula> {
        for tap in self.taps.values() {
            if let Ok(formula) = tap.load_formula(name) {
                return Ok(formula);
            }
        }

        anyhow::bail!("Formula {} not found in any tap", name);
    }

    /// Search for formulas matching a query
    pub fn search(&self, query: &str) -> Result<Vec<(String, String)>> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        for (tap_name, tap) in &self.taps {
            if let Ok(formulas) = tap.list_formulas() {
                for formula_name in formulas {
                    if formula_name.to_lowercase().contains(&query_lower) {
                        results.push((tap_name.clone(), formula_name));
                    }
                }
            }
        }

        Ok(results)
    }

    /// List all available formulas
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tap_manager_creation() {
        let temp = TempDir::new().unwrap();
        let mut paths = Paths::new().unwrap();
        paths.taps_dir = temp.path().to_path_buf();

        let manager = TapManager::new(paths);
        assert_eq!(manager.taps.len(), 0);
    }

    #[test]
    fn test_tap_names() {
        let temp = TempDir::new().unwrap();
        let mut paths = Paths::new().unwrap();
        paths.taps_dir = temp.path().to_path_buf();

        let mut manager = TapManager::new(paths);

        // Can't actually add taps without a real git repo,
        // but we can test the basic functionality
        assert_eq!(manager.tap_names().len(), 0);
    }
}
