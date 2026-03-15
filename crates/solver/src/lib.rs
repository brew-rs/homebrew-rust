//! SAT-based dependency resolution
//!
//! This crate implements dependency resolution using a SAT solver approach
//! for maximum performance (targeting 100x+ improvement over naive backtracking).
//!
//! Also provides installation queue with topological sorting.

pub mod queue;
pub mod resolver;

use anyhow::{Context, Result};
use brew_formula::Formula;
use semver::Version;
use std::collections::HashMap;
use tracing::debug;

pub use queue::{DryRunEntry, DryRunSummary, InstallQueue, QueueError, QueueItem};
pub use resolver::{PackageEntry, ResolverError, SATResolver};

/// High-level dependency resolver.
///
/// Loads formulas, translates them into a SAT problem, and returns the set of
/// packages (with pinned versions) needed to satisfy the requirements.
pub struct Resolver {
    /// All formulas known to this resolver
    formulas: HashMap<String, Formula>,
}

impl Resolver {
    pub fn new() -> Self {
        Self { formulas: HashMap::new() }
    }

    /// Add a formula to the resolver's knowledge base.
    pub fn add_formula(&mut self, formula: Formula) {
        self.formulas.insert(formula.package.name.clone(), formula);
    }

    /// Resolve dependencies for a package, returning a list of
    /// `(name, version)` pairs for every package that must be installed.
    ///
    /// Returns packages in no particular order — use `InstallQueue` for
    /// dependency-ordered installation.
    pub fn resolve(&self, package_name: &str) -> Result<Vec<(String, Version)>> {
        debug!("Resolving dependencies for: {}", package_name);

        if !self.formulas.contains_key(package_name) {
            return Err(ResolverError::PackageNotFound(package_name.to_string()).into());
        }

        // Build the SAT resolver's package universe from loaded formulas
        let mut sat = SATResolver::new();
        for formula in self.formulas.values() {
            let version = Version::parse(formula.version())
                .with_context(|| format!("Invalid version in formula '{}'", formula.name()))?;
            sat.add_package(PackageEntry {
                name: formula.name().to_string(),
                version,
                dependencies: formula.dependencies.runtime.clone(),
            });
        }

        sat.require(package_name);
        let resolution = sat.resolve()?;

        Ok(resolution.into_iter().collect())
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_formula(name: &str, version: &str, deps: Vec<&str>) -> Formula {
        let deps_toml = deps
            .iter()
            .map(|d| format!("\"{}\"", d))
            .collect::<Vec<_>>()
            .join(", ");
        let toml = format!(
            r#"
            [package]
            name = "{}"
            version = "{}"
            description = "Test package"

            [source]
            url = "https://example.com/{}.tar.gz"
            sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

            [dependencies]
            runtime = [{}]
            "#,
            name, version, name, deps_toml
        );
        Formula::from_str_unchecked(&toml).unwrap()
    }

    #[test]
    fn test_resolver_creation() {
        let resolver = Resolver::new();
        assert_eq!(resolver.formulas.len(), 0);
    }

    #[test]
    fn test_resolver_unknown_package() {
        let resolver = Resolver::new();
        assert!(resolver.resolve("does-not-exist").is_err());
    }

    #[test]
    fn test_resolver_single_package() {
        let mut resolver = Resolver::new();
        resolver.add_formula(make_formula("curl", "8.18.0", vec![]));

        let result = resolver.resolve("curl").unwrap();
        assert!(!result.is_empty());
        assert!(result.iter().any(|(n, _)| n == "curl"));
    }

    #[test]
    fn test_resolver_with_dependencies() {
        let mut resolver = Resolver::new();
        resolver.add_formula(make_formula("zlib", "1.3.1", vec![]));
        resolver.add_formula(make_formula("openssl", "3.2.0", vec!["zlib"]));
        resolver.add_formula(make_formula("curl", "8.18.0", vec!["openssl"]));

        let result = resolver.resolve("curl").unwrap();
        let names: Vec<&str> = result.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"curl"), "curl missing");
        assert!(names.contains(&"openssl"), "openssl missing");
        assert!(names.contains(&"zlib"), "zlib missing");
    }

    #[test]
    fn test_resolver_with_version_constraints() {
        let mut resolver = Resolver::new();
        resolver.add_formula(make_formula("openssl", "3.2.0", vec![]));
        resolver.add_formula(make_formula("curl", "8.18.0", vec!["openssl ^3.0"]));

        let result = resolver.resolve("curl").unwrap();
        let names: Vec<&str> = result.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"curl"));
        assert!(names.contains(&"openssl"));
    }

    #[test]
    fn test_resolver_version_conflict() {
        let mut resolver = Resolver::new();
        // openssl 2.x won't satisfy ^3.0
        resolver.add_formula(make_formula("openssl", "2.1.0", vec![]));
        resolver.add_formula(make_formula("curl", "8.18.0", vec!["openssl ^3.0"]));

        assert!(resolver.resolve("curl").is_err());
    }
}
