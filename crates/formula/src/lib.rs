//! Package formula parsing and validation
//!
//! This crate defines the TOML-based formula format and provides
//! fast parsing using Serde (300-800 MB/s).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    pub name: String,
    pub version: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,

    #[serde(default)]
    pub source: SourceInfo,

    #[serde(default)]
    pub dependencies: Dependencies,

    #[serde(default)]
    pub build: BuildInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub test: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildInfo {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl Formula {
    /// Parse a formula from a TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let formula: Formula = toml::from_str(&contents)?;
        Ok(formula)
    }

    /// Parse a formula from a TOML string
    pub fn from_str(contents: &str) -> Result<Self> {
        let formula: Formula = toml::from_str(contents)?;
        Ok(formula)
    }

    /// Validate the formula
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Formula name cannot be empty");
        }
        if self.version.is_empty() {
            anyhow::bail!("Formula version cannot be empty");
        }
        // TODO: Add more validation rules
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_formula() {
        let toml = r#"
            name = "example"
            version = "1.0.0"
            description = "An example package"

            [source]
            url = "https://example.com/release.tar.gz"
            sha256 = "abc123"

            [dependencies]
            runtime = ["dep1", "dep2"]
        "#;

        let formula = Formula::from_str(toml).unwrap();
        assert_eq!(formula.name, "example");
        assert_eq!(formula.version, "1.0.0");
        assert_eq!(formula.dependencies.runtime.len(), 2);
    }
}
