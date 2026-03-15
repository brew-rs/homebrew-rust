//! Package formula parsing and validation
//!
//! This crate defines the TOML-based formula format and provides
//! fast parsing using Serde (300-800 MB/s).

mod validation;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use validation::ValidationError;

/// Complete formula definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    pub package: PackageInfo,

    pub source: SourceInfo,

    #[serde(default)]
    pub dependencies: Dependencies,

    #[serde(default)]
    pub build: BuildInfo,

    #[serde(default)]
    pub bottle: BottleInfo,
}

/// Package metadata section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<String>,
}

/// Source download information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub url: String,
    pub sha256: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<String>,
}

/// Package dependencies
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<String>,
}

/// Build instructions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,

    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,

    #[serde(default = "default_parallel")]
    pub parallel: bool,
}

fn default_parallel() -> bool {
    true
}

/// Pre-built binary (bottle) information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BottleInfo {
    #[serde(rename = "macos-arm64", skip_serializing_if = "Option::is_none")]
    pub macos_arm64: Option<BottleVariant>,

    #[serde(rename = "macos-x86_64", skip_serializing_if = "Option::is_none")]
    pub macos_x86_64: Option<BottleVariant>,

    #[serde(rename = "linux-x86_64", skip_serializing_if = "Option::is_none")]
    pub linux_x86_64: Option<BottleVariant>,
}

/// Individual bottle variant for a platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleVariant {
    pub url: String,
    pub sha256: String,
}

impl Formula {
    /// Parse a formula from a TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let formula: Formula = toml::from_str(&contents)?;
        formula.validate()?;
        Ok(formula)
    }

    /// Parse a formula from a TOML string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(contents: &str) -> Result<Self> {
        let formula: Formula = toml::from_str(contents)?;
        formula.validate()?;
        Ok(formula)
    }

    /// Parse without validation (useful for tests)
    pub fn from_str_unchecked(contents: &str) -> Result<Self> {
        let formula: Formula = toml::from_str(contents)?;
        Ok(formula)
    }

    /// Validate the formula
    pub fn validate(&self) -> Result<()> {
        validation::validate_formula(self)
    }

    /// Get the package name
    pub fn name(&self) -> &str {
        &self.package.name
    }

    /// Get the package version
    pub fn version(&self) -> &str {
        &self.package.version
    }

    /// Get bottle for current platform, if available
    pub fn current_platform_bottle(&self) -> Option<&BottleVariant> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        return self.bottle.macos_arm64.as_ref();

        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        return self.bottle.macos_x86_64.as_ref();

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return self.bottle.linux_x86_64.as_ref();

        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64")
        )))]
        None
    }

    /// Check if a bottle is available for current platform
    pub fn has_bottle(&self) -> bool {
        self.current_platform_bottle().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_formula_with_package_section() {
        let toml = r#"
            [package]
            name = "example"
            version = "1.0.0"
            description = "An example package"

            [source]
            url = "https://example.com/release.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [dependencies]
            runtime = ["dep1", "dep2"]
        "#;

        let formula = Formula::from_str(toml).unwrap();
        assert_eq!(formula.package.name, "example");
        assert_eq!(formula.package.version, "1.0.0");
        assert_eq!(formula.dependencies.runtime.len(), 2);
    }

    #[test]
    fn test_parse_formula_with_maintainers() {
        let toml = r#"
            [package]
            name = "test"
            version = "1.0.0"
            description = "Test package"
            maintainers = ["alice@example.com", "bob@example.com"]

            [source]
            url = "https://example.com/test.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        "#;

        let formula = Formula::from_str(toml).unwrap();
        assert_eq!(formula.package.maintainers.len(), 2);
    }

    #[test]
    fn test_parse_formula_with_bottles() {
        let toml = r#"
            [package]
            name = "test"
            version = "1.0.0"
            description = "Test package"

            [source]
            url = "https://example.com/test.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [bottle.macos-arm64]
            url = "https://example.com/test-arm64.tar.gz"
            sha256 = "abc123def456"

            [bottle.macos-x86_64]
            url = "https://example.com/test-x86.tar.gz"
            sha256 = "xyz789"
        "#;

        let formula = Formula::from_str(toml).unwrap();
        assert!(formula.bottle.macos_arm64.is_some());
        assert!(formula.bottle.macos_x86_64.is_some());
        assert!(formula.bottle.linux_x86_64.is_none());
    }

    #[test]
    fn test_build_parallel_default() {
        let toml = r#"
            [package]
            name = "test"
            version = "1.0.0"
            description = "Test"

            [source]
            url = "https://example.com/test.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [build]
        "#;

        let formula = Formula::from_str(toml).unwrap();
        assert!(formula.build.parallel); // Should default to true
    }
}
