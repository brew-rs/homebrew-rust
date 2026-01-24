//! Git-based formula repository (tap) management
//!
//! This crate handles:
//! - Cloning tap repositories from git
//! - Updating existing taps
//! - Discovering formulas within taps
//! - Managing multiple taps

mod manager;
mod repository;

pub use manager::TapManager;
pub use repository::Tap;

use anyhow::Result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TapError {
    #[error("Tap not found: {0}")]
    NotFound(String),

    #[error("Tap already exists: {0}")]
    AlreadyExists(String),

    #[error("Git error: {0}")]
    GitError(String),

    #[error("Invalid tap name: {0}")]
    InvalidName(String),
}

/// Parse tap name in format "user/repo"
pub fn parse_tap_name(name: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!(TapError::InvalidName(name.to_string()));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tap_name() {
        let (user, repo) = parse_tap_name("brew-rs/core").unwrap();
        assert_eq!(user, "brew-rs");
        assert_eq!(repo, "core");
    }

    #[test]
    fn test_parse_tap_name_invalid() {
        assert!(parse_tap_name("invalid").is_err());
        assert!(parse_tap_name("too/many/parts").is_err());
    }
}
