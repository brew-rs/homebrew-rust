//! User configuration settings

use crate::Paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

/// User configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Maximum concurrent downloads
    #[serde(default = "default_max_downloads")]
    pub max_concurrent_downloads: usize,

    /// Maximum concurrent builds
    #[serde(default = "default_max_builds")]
    pub max_concurrent_builds: usize,

    /// Default taps to use
    #[serde(default = "default_taps")]
    pub default_taps: Vec<TapConfig>,

    /// Auto-update taps before install
    #[serde(default = "default_auto_update")]
    pub auto_update_taps: bool,

    /// Prefer bottles over source builds
    #[serde(default = "default_prefer_bottles")]
    pub prefer_bottles: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapConfig {
    pub name: String,
    pub url: String,
}

fn default_max_downloads() -> usize {
    50
}

fn default_max_builds() -> usize {
    4
}

fn default_taps() -> Vec<TapConfig> {
    vec![TapConfig {
        name: "brew-rs/core".to_string(),
        url: "https://github.com/brew-rs/core.git".to_string(),
    }]
}

fn default_auto_update() -> bool {
    true
}

fn default_prefer_bottles() -> bool {
    true
}

impl Settings {
    /// Load settings from config file or create default
    pub fn load(paths: &Paths) -> Result<Self> {
        if paths.config_file.exists() {
            let contents = fs::read_to_string(&paths.config_file)
                .context("Failed to read config file")?;
            let settings: Settings = toml::from_str(&contents)
                .context("Failed to parse config file")?;
            Ok(settings)
        } else {
            Ok(Self::default())
        }
    }

    /// Save settings to config file
    pub fn save(&self, paths: &Paths) -> Result<()> {
        // Ensure config directory exists
        if let Some(parent) = paths.config_file.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize settings")?;
        fs::write(&paths.config_file, contents)
            .context("Failed to write config file")?;

        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: default_max_downloads(),
            max_concurrent_builds: default_max_builds(),
            default_taps: default_taps(),
            auto_update_taps: default_auto_update(),
            prefer_bottles: default_prefer_bottles(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.max_concurrent_downloads, 50);
        assert_eq!(settings.max_concurrent_builds, 4);
        assert_eq!(settings.default_taps.len(), 1);
        assert!(settings.auto_update_taps);
        assert!(settings.prefer_bottles);
    }

    #[test]
    fn test_serialize_settings() {
        let settings = Settings::default();
        let toml = toml::to_string(&settings).unwrap();
        assert!(toml.contains("max_concurrent_downloads"));
        assert!(toml.contains("brew-rs/core"));
    }
}
