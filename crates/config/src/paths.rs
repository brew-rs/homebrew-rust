//! XDG Base Directory paths for brew-rs

use anyhow::{Context, Result};
use std::path::PathBuf;

/// XDG-compliant directory paths for brew-rs
#[derive(Debug, Clone)]
pub struct Paths {
    /// Data directory: ~/.local/share/brew-rs
    pub data_dir: PathBuf,

    /// Config directory: ~/.config/brew-rs
    pub config_dir: PathBuf,

    /// Cache directory: ~/.cache/brew-rs
    pub cache_dir: PathBuf,

    /// Cellar directory: ~/.local/share/brew-rs/cellar
    pub cellar_dir: PathBuf,

    /// Database directory: ~/.local/share/brew-rs/db
    pub db_dir: PathBuf,

    /// Taps directory: ~/.local/share/brew-rs/taps
    pub taps_dir: PathBuf,

    /// Downloads cache: ~/.cache/brew-rs/downloads
    pub downloads_dir: PathBuf,

    /// Binary symlinks: ~/.local/bin
    pub bin_dir: PathBuf,

    /// Database file path
    pub db_file: PathBuf,

    /// Config file path
    pub config_file: PathBuf,
}

impl Paths {
    /// Create new Paths using XDG Base Directory specification
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;

        // XDG Base Directory defaults
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".local/share"));

        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"));

        let cache_home = std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".cache"));

        // brew-rs specific paths
        let data_dir = data_home.join("brew-rs");
        let config_dir = config_home.join("brew-rs");
        let cache_dir = cache_home.join("brew-rs");

        let cellar_dir = data_dir.join("cellar");
        let db_dir = data_dir.join("db");
        let taps_dir = data_dir.join("taps");
        let downloads_dir = cache_dir.join("downloads");
        let bin_dir = home.join(".local/bin");

        let db_file = db_dir.join("packages.db");
        let config_file = config_dir.join("config.toml");

        Ok(Self {
            data_dir,
            config_dir,
            cache_dir,
            cellar_dir,
            db_dir,
            taps_dir,
            downloads_dir,
            bin_dir,
            db_file,
            config_file,
        })
    }

    /// Initialize all directories
    pub fn init_all(&self) -> Result<()> {
        self.create_if_missing(&self.data_dir)?;
        self.create_if_missing(&self.config_dir)?;
        self.create_if_missing(&self.cache_dir)?;
        self.create_if_missing(&self.cellar_dir)?;
        self.create_if_missing(&self.db_dir)?;
        self.create_if_missing(&self.taps_dir)?;
        self.create_if_missing(&self.downloads_dir)?;
        self.create_if_missing(&self.bin_dir)?;

        Ok(())
    }

    /// Create directory if it doesn't exist
    fn create_if_missing(&self, path: &PathBuf) -> Result<()> {
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        }
        Ok(())
    }

    /// Get cellar path for a specific package and version
    pub fn package_cellar(&self, name: &str, version: &str) -> PathBuf {
        self.cellar_dir.join(name).join(version)
    }

    /// Get tap directory path
    pub fn tap_dir(&self, user: &str, repo: &str) -> PathBuf {
        self.taps_dir.join(user).join(repo)
    }

    /// Check if binary directory is in PATH
    pub fn is_bin_in_path(&self) -> bool {
        if let Some(path_var) = std::env::var_os("PATH") {
            std::env::split_paths(&path_var)
                .any(|p| p == self.bin_dir)
        } else {
            false
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new().expect("Failed to create Paths")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_creation() {
        let paths = Paths::new().unwrap();
        assert!(paths.data_dir.to_string_lossy().contains(".local/share/brew-rs"));
        assert!(paths.config_dir.to_string_lossy().contains(".config/brew-rs"));
        assert!(paths.cache_dir.to_string_lossy().contains(".cache/brew-rs"));
    }

    #[test]
    fn test_package_cellar_path() {
        let paths = Paths::new().unwrap();
        let cellar = paths.package_cellar("curl", "8.5.0");
        assert!(cellar.to_string_lossy().contains("cellar/curl/8.5.0"));
    }

    #[test]
    fn test_tap_dir_path() {
        let paths = Paths::new().unwrap();
        let tap = paths.tap_dir("brew-rs", "core");
        assert!(tap.to_string_lossy().contains("taps/brew-rs/core"));
    }
}
