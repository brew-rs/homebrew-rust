//! Configuration and XDG directory management for brew-rs
//!
//! This crate handles:
//! - XDG Base Directory Specification compliance
//! - Configuration file management
//! - Directory initialization and setup

mod paths;
mod settings;

pub use paths::Paths;
pub use settings::Settings;

use anyhow::Result;
use std::path::PathBuf;

/// Main configuration manager for brew-rs
pub struct Config {
    pub paths: Paths,
    pub settings: Settings,
}

impl Config {
    /// Load or create default configuration
    pub fn load() -> Result<Self> {
        let paths = Paths::new()?;
        let settings = Settings::load(&paths)?;

        Ok(Self { paths, settings })
    }

    /// Initialize all required directories
    pub fn init_directories(&self) -> Result<()> {
        self.paths.init_all()
    }

    /// Save current settings to config file
    pub fn save(&self) -> Result<()> {
        self.settings.save(&self.paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let config = Config::load();
        assert!(config.is_ok());
    }
}
