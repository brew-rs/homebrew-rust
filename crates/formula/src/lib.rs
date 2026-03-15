//! Package formula parsing and validation
//!
//! This crate defines the TOML-based formula format and provides
//! fast parsing using Serde (300-800 MB/s).

mod validation;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

pub use validation::ValidationError;

/// A package dependency with an optional version constraint.
///
/// Deserializes from a string like `"openssl ^3.0"` or `"libssh2"`.
/// Bare names get `version_req: None`, meaning "any version".
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    /// Package name
    pub name: String,
    /// Optional semver version requirement (e.g. `^3.0`, `>=1.2.11`)
    pub version_req: Option<semver::VersionReq>,
}

impl Dependency {
    /// Create a dependency with no version constraint.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), version_req: None }
    }

    /// Create a dependency with a version constraint.
    pub fn with_req(name: impl Into<String>, req: semver::VersionReq) -> Self {
        Self { name: name.into(), version_req: Some(req) }
    }

    /// Parse from a string like `"openssl ^3.0"` or `"libssh2"`.
    pub fn from_dep_str(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        match trimmed.splitn(2, ' ').collect::<Vec<_>>().as_slice() {
            [name] => Ok(Self::new(*name)),
            [name, req_str] => {
                let req = semver::VersionReq::parse(req_str.trim())
                    .map_err(|e| format!("Invalid version constraint '{}': {}", req_str, e))?;
                Ok(Self::with_req(*name, req))
            }
            _ => Err(format!("Cannot parse dependency: '{}'", s)),
        }
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version_req {
            Some(req) => write!(f, "{} {}", self.name, req),
            None => write!(f, "{}", self.name),
        }
    }
}

impl Serialize for Dependency {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Dependency::from_dep_str(&s).map_err(serde::de::Error::custom)
    }
}

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
    pub runtime: Vec<Dependency>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<Dependency>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<Dependency>,
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

    // ── Dependency parsing tests ──────────────────────────────────────────────

    #[test]
    fn test_dependency_parse_bare_name() {
        let dep = Dependency::from_dep_str("libssh2").unwrap();
        assert_eq!(dep.name, "libssh2");
        assert!(dep.version_req.is_none());
    }

    #[test]
    fn test_dependency_parse_caret() {
        let dep = Dependency::from_dep_str("openssl ^3.0").unwrap();
        assert_eq!(dep.name, "openssl");
        let req = dep.version_req.unwrap();
        // ^3.0 should match 3.2.0
        assert!(req.matches(&semver::Version::parse("3.2.0").unwrap()));
        // ^3.0 should NOT match 4.0.0
        assert!(!req.matches(&semver::Version::parse("4.0.0").unwrap()));
    }

    #[test]
    fn test_dependency_parse_tilde() {
        let dep = Dependency::from_dep_str("zlib ~1.2.11").unwrap();
        assert_eq!(dep.name, "zlib");
        let req = dep.version_req.unwrap();
        assert!(req.matches(&semver::Version::parse("1.2.13").unwrap()));
        assert!(!req.matches(&semver::Version::parse("1.3.0").unwrap()));
    }

    #[test]
    fn test_dependency_parse_gte() {
        let dep = Dependency::from_dep_str("pcre >=8.45.0").unwrap();
        assert_eq!(dep.name, "pcre");
        let req = dep.version_req.unwrap();
        assert!(req.matches(&semver::Version::parse("8.45.0").unwrap()));
        assert!(req.matches(&semver::Version::parse("9.0.0").unwrap()));
        assert!(!req.matches(&semver::Version::parse("8.44.0").unwrap()));
    }

    #[test]
    fn test_dependency_parse_exact() {
        let dep = Dependency::from_dep_str("bzip2 =1.0.8").unwrap();
        assert_eq!(dep.name, "bzip2");
        let req = dep.version_req.unwrap();
        assert!(req.matches(&semver::Version::parse("1.0.8").unwrap()));
        assert!(!req.matches(&semver::Version::parse("1.0.9").unwrap()));
    }

    #[test]
    fn test_dependency_serialize_bare_name() {
        let dep = Dependency::new("libssh2");
        assert_eq!(dep.to_string(), "libssh2");
    }

    #[test]
    fn test_dependency_serialize_with_constraint() {
        let req = semver::VersionReq::parse("^3.0").unwrap();
        let dep = Dependency::with_req("openssl", req);
        let s = dep.to_string();
        assert!(s.starts_with("openssl"), "must start with name: '{}'", s);
        assert!(s.contains('^'), "must contain caret: '{}'", s);
    }

    #[test]
    fn test_dependency_roundtrip() {
        let cases = ["libssh2", "openssl ^3.0", "zlib >=1.2.11"];
        for case in &cases {
            let dep = Dependency::from_dep_str(case).unwrap();
            let serialized = dep.to_string();
            let dep2 = Dependency::from_dep_str(&serialized).unwrap();
            assert_eq!(dep, dep2, "roundtrip failed for '{}'", case);
        }
    }

    #[test]
    fn test_dependency_serde_deserialize() {
        // Simulate TOML deserialization of a string
        let s: Dependency = toml::from_str("dep = \"openssl ^3.0\"\n")
            .map(|v: toml::Value| {
                v["dep"].as_str().map(|s| Dependency::from_dep_str(s).unwrap()).unwrap()
            })
            .unwrap();
        assert_eq!(s.name, "openssl");
        assert!(s.version_req.is_some());
    }

    // ── Existing formula tests (must still pass) ──────────────────────────────

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
        // Verify names are preserved
        assert_eq!(formula.dependencies.runtime[0].name, "dep1");
        assert_eq!(formula.dependencies.runtime[1].name, "dep2");
    }

    #[test]
    fn test_parse_formula_with_versioned_deps() {
        let toml = r#"
            [package]
            name = "curl"
            version = "8.18.0"
            description = "HTTP client"

            [source]
            url = "https://example.com/curl.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [dependencies]
            runtime = ["openssl ^3.0", "zlib >=1.2.11", "libssh2"]
        "#;

        let formula = Formula::from_str_unchecked(toml).unwrap();
        assert_eq!(formula.dependencies.runtime.len(), 3);

        let openssl = &formula.dependencies.runtime[0];
        assert_eq!(openssl.name, "openssl");
        assert!(openssl.version_req.is_some());

        let zlib = &formula.dependencies.runtime[1];
        assert_eq!(zlib.name, "zlib");
        assert!(zlib.version_req.is_some());

        let libssh2 = &formula.dependencies.runtime[2];
        assert_eq!(libssh2.name, "libssh2");
        assert!(libssh2.version_req.is_none());
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
