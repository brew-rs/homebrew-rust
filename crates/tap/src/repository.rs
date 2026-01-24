//! Individual tap repository management

use anyhow::{Context, Result};
use brew_formula::Formula;
use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// A single tap repository
#[derive(Debug, Clone)]
pub struct Tap {
    pub name: String,
    pub url: String,
    pub path: PathBuf,
}

impl Tap {
    /// Create a new tap reference
    pub fn new(name: String, url: String, path: PathBuf) -> Self {
        Self { name, url, path }
    }

    /// Clone the tap repository if it doesn't exist
    pub fn clone_if_needed(&self) -> Result<()> {
        if self.path.exists() {
            debug!("Tap {} already exists at {}", self.name, self.path.display());
            return Ok(());
        }

        info!("Cloning tap {} from {}", self.name, self.url);

        // Create parent directory if needed
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create tap parent directory")?;
        }

        Repository::clone(&self.url, &self.path)
            .with_context(|| format!("Failed to clone tap {} from {}", self.name, self.url))?;

        info!("✓ Cloned tap {}", self.name);
        Ok(())
    }

    /// Update the tap repository (git pull)
    pub fn update(&self) -> Result<()> {
        if !self.path.exists() {
            return self.clone_if_needed();
        }

        info!("Updating tap {}", self.name);

        let repo = Repository::open(&self.path)
            .with_context(|| format!("Failed to open tap repository: {}", self.path.display()))?;

        // Fetch from origin
        let mut remote = repo.find_remote("origin")
            .context("Failed to find origin remote")?;

        remote.fetch(&["main", "master"], None, None)
            .context("Failed to fetch from origin")?;

        // Get the current branch name
        let head = repo.head().context("Failed to get HEAD")?;
        let branch_name = head.shorthand().unwrap_or("main");

        // Merge FETCH_HEAD into current branch
        let fetch_head = repo.find_reference("FETCH_HEAD")
            .context("Failed to find FETCH_HEAD")?;
        let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)
            .context("Failed to get FETCH_HEAD commit")?;

        // Do a fast-forward merge
        let analysis = repo.merge_analysis(&[&fetch_commit])
            .context("Failed to analyze merge")?;

        if analysis.0.is_up_to_date() {
            debug!("Tap {} is up to date", self.name);
        } else if analysis.0.is_fast_forward() {
            // Fast-forward merge
            let refname = format!("refs/heads/{}", branch_name);
            let mut reference = repo.find_reference(&refname)
                .context("Failed to find branch reference")?;
            reference.set_target(fetch_commit.id(), "Fast-forward")
                .context("Failed to fast-forward")?;
            repo.set_head(&refname)
                .context("Failed to set HEAD")?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .context("Failed to checkout HEAD")?;
            info!("✓ Updated tap {}", self.name);
        } else {
            anyhow::bail!("Tap {} requires a manual merge", self.name);
        }

        Ok(())
    }

    /// Get the formulas directory within the tap
    pub fn formulas_dir(&self) -> PathBuf {
        self.path.join("formulas")
    }

    /// List all formula files in the tap
    pub fn list_formulas(&self) -> Result<Vec<String>> {
        let formulas_dir = self.formulas_dir();

        if !formulas_dir.exists() {
            return Ok(Vec::new());
        }

        let mut formulas = Vec::new();

        for entry in std::fs::read_dir(formulas_dir)
            .context("Failed to read formulas directory")?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    formulas.push(name.to_string());
                }
            }
        }

        Ok(formulas)
    }

    /// Load a formula from the tap
    pub fn load_formula(&self, name: &str) -> Result<Formula> {
        let formula_path = self.formulas_dir().join(format!("{}.toml", name));

        if !formula_path.exists() {
            anyhow::bail!("Formula {} not found in tap {}", name, self.name);
        }

        Formula::from_file(&formula_path)
            .with_context(|| format!("Failed to load formula {} from {}", name, self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tap_creation() {
        let temp = TempDir::new().unwrap();
        let tap = Tap::new(
            "test/tap".to_string(),
            "https://example.com/test.git".to_string(),
            temp.path().join("tap"),
        );

        assert_eq!(tap.name, "test/tap");
        assert_eq!(tap.url, "https://example.com/test.git");
    }

    #[test]
    fn test_formulas_dir() {
        let temp = TempDir::new().unwrap();
        let tap = Tap::new(
            "test/tap".to_string(),
            "https://example.com/test.git".to_string(),
            temp.path().join("tap"),
        );

        let formulas_dir = tap.formulas_dir();
        assert!(formulas_dir.to_string_lossy().contains("formulas"));
    }
}
