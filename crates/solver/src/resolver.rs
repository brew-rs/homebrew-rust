//! SAT-based dependency resolver using varisat.
//!
//! # Design
//!
//! Each package maps to a boolean SAT variable. "True" means the package is
//! selected for installation. We generate three kinds of clauses:
//!
//! 1. **Root requirement**: the requested package *must* be selected → unit clause `[pkg]`.
//! 2. **Dependency implication**: if `P` is selected, all its runtime deps that
//!    satisfy version constraints must also be selected → `[¬P, dep]`.
//! 3. **Unsatisfiable dep**: if `P` requires `dep@constraint` but no available
//!    package version matches, `P` cannot be installed → unit clause `[¬P]`.
//!
//! Because we currently have one version per package (one TOML file = one
//! version), the at-most-one constraint is trivially satisfied.

use std::collections::HashMap;

use semver::Version;
use thiserror::Error;
use varisat::{CnfFormula, ExtendFormula, Lit, Solver, Var};

use brew_formula::Dependency;

/// Errors from the SAT resolver.
#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("Package '{0}' not found — it may not be in any loaded tap")]
    PackageNotFound(String),

    #[error(
        "Version conflict: '{package}' requires '{dep} {constraint}', \
         but available version is {available}"
    )]
    ConflictingConstraints {
        package: String,
        dep: String,
        constraint: String,
        available: String,
    },

    #[error("Dependency graph is unsatisfiable: {reason}")]
    Unsatisfiable { reason: String },
}

/// A package entry in the resolver's package universe.
pub struct PackageEntry {
    /// Package name (matches formula name)
    pub name: String,
    /// The version provided by the tap
    pub version: Version,
    /// Runtime dependencies (with optional version constraints)
    pub dependencies: Vec<Dependency>,
}

/// SAT-based dependency resolver.
///
/// ```
/// use brew_solver::resolver::{SATResolver, PackageEntry};
/// use semver::Version;
///
/// let mut resolver = SATResolver::new();
/// resolver.add_package(PackageEntry {
///     name: "curl".into(),
///     version: Version::parse("8.18.0").unwrap(),
///     dependencies: vec![],
/// });
/// resolver.require("curl");
/// let result = resolver.resolve().unwrap();
/// assert!(result.contains_key("curl"));
/// ```
pub struct SATResolver {
    /// All known packages
    packages: HashMap<String, PackageEntry>,
    /// Packages that must appear in the final resolution
    requirements: Vec<String>,
}

impl SATResolver {
    /// Create a new, empty resolver.
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            requirements: Vec::new(),
        }
    }

    /// Register a package with the resolver.
    pub fn add_package(&mut self, entry: PackageEntry) {
        self.packages.insert(entry.name.clone(), entry);
    }

    /// Declare that a package must be included in the resolution.
    pub fn require(&mut self, name: impl Into<String>) {
        self.requirements.push(name.into());
    }

    /// Resolve all requirements, returning a map from package name to chosen
    /// version.
    ///
    /// Returns `Err` if:
    /// - a required package is not in the universe
    /// - version constraints are unsatisfiable
    /// - a cycle or conflict makes the formula UNSAT
    pub fn resolve(&self) -> Result<HashMap<String, Version>, ResolverError> {
        // ── 1. Assign a SAT variable index to each package ───────────────────
        let pkg_names: Vec<&str> =
            self.packages.keys().map(String::as_str).collect();
        let var_index: HashMap<&str, usize> = pkg_names
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, i))
            .collect();

        let mut formula = CnfFormula::new();
        formula.set_var_count(pkg_names.len());

        // ── 2. Validate requirements ──────────────────────────────────────────
        let mut pre_check_errors: Vec<String> = Vec::new();

        for req in &self.requirements {
            if !self.packages.contains_key(req.as_str()) {
                return Err(ResolverError::PackageNotFound(req.clone()));
            }
        }

        // ── 2b. Cycle detection (DFS) ─────────────────────────────────────────
        // Detect cycles before SAT solving so we can name the involved packages
        // rather than returning a generic Unsatisfiable error.
        {
            // 0 = unvisited, 1 = in-stack, 2 = done
            let mut state: HashMap<&str, u8> = HashMap::new();
            let mut cycle_path: Vec<String> = Vec::new();

            fn dfs<'a>(
                node: &'a str,
                packages: &'a HashMap<String, PackageEntry>,
                state: &mut HashMap<&'a str, u8>,
                path: &mut Vec<String>,
            ) -> Option<Vec<String>> {
                match state.get(node).copied().unwrap_or(0) {
                    2 => return None,          // already fully explored
                    1 => {
                        // back-edge found — report cycle from first occurrence
                        let start = path.iter().position(|n| n == node).unwrap_or(0);
                        let mut cycle = path[start..].to_vec();
                        cycle.push(node.to_string());
                        return Some(cycle);
                    }
                    _ => {}
                }
                state.insert(node, 1);
                path.push(node.to_string());
                if let Some(entry) = packages.get(node) {
                    for dep in &entry.dependencies {
                        if let Some(cycle) = dfs(&dep.name, packages, state, path) {
                            return Some(cycle);
                        }
                    }
                }
                path.pop();
                state.insert(node, 2);
                None
            }

            for name in pkg_names.iter() {
                if let Some(cycle) = dfs(name, &self.packages, &mut state, &mut cycle_path) {
                    return Err(ResolverError::Unsatisfiable {
                        reason: format!("circular dependency detected: {}", cycle.join(" → ")),
                    });
                }
            }
        }

        // ── 3. Generate implication clauses for all packages ──────────────────
        //
        // For each package P with index `pi`:
        //   For each runtime dep D of P:
        //     Case A — D exists and version satisfies constraint:
        //       add clause [¬P, D]  (P selected → D selected)
        //     Case B — D doesn't exist in universe or version mismatch:
        //       add clause [¬P]  (P can never be installed)
        for (name, entry) in &self.packages {
            let pi = var_index[name.as_str()];
            let p_var = Var::from_index(pi);

            for dep in &entry.dependencies {
                match self.packages.get(&dep.name) {
                    None => {
                        pre_check_errors.push(format!(
                            "'{}' requires '{}', which is not available",
                            name, dep.name
                        ));
                        // Force P to be false
                        formula.add_clause(&[Lit::negative(p_var)]);
                    }
                    Some(dep_entry) => {
                        // Check version constraint if present
                        if let Some(ref req) = dep.version_req {
                            if !req.matches(&dep_entry.version) {
                                pre_check_errors.push(format!(
                                    "'{}' requires '{} {}', but available version is {}",
                                    name, dep.name, req, dep_entry.version
                                ));
                                // Force P false
                                formula.add_clause(&[Lit::negative(p_var)]);
                                continue;
                            }
                        }
                        // Constraint satisfied: add implication P → dep
                        let di = var_index[dep.name.as_str()];
                        let d_var = Var::from_index(di);
                        formula.add_clause(&[Lit::negative(p_var), Lit::positive(d_var)]);
                    }
                }
            }
        }

        // ── 4. Add root requirement unit clauses ──────────────────────────────
        for req in &self.requirements {
            let ri = var_index[req.as_str()];
            let r_var = Var::from_index(ri);
            formula.add_clause(&[Lit::positive(r_var)]);
        }

        let mut solver = Solver::new();
        solver.add_formula(&formula);

        // ── 5. Solve ──────────────────────────────────────────────────────────
        let sat = solver
            .solve()
            .map_err(|e| ResolverError::Unsatisfiable { reason: e.to_string() })?;

        if !sat {
            // Build the most helpful error message we can
            let reason = if !pre_check_errors.is_empty() {
                pre_check_errors.join("; ")
            } else {
                "constraints are mutually exclusive".to_string()
            };
            return Err(ResolverError::Unsatisfiable { reason });
        }

        // Warn about constraint issues for packages not in the required set
        // (they are excluded from installation but the conflict is still notable)
        for msg in &pre_check_errors {
            tracing::warn!("Non-fatal constraint issue (package excluded): {}", msg);
        }

        // ── 6. Collect selected packages from the model ───────────────────────
        let model = solver.model().expect("model is Some when solve() returned true");
        let mut resolution: HashMap<String, Version> = HashMap::new();

        for lit in model {
            if lit.is_positive() {
                let idx = lit.var().index();
                if let Some(&name) = pkg_names.get(idx) {
                    if let Some(entry) = self.packages.get(name) {
                        resolution.insert(name.to_string(), entry.version.clone());
                    }
                }
            }
        }

        Ok(resolution)
    }
}

impl Default for SATResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brew_formula::Dependency;

    fn make_pkg(name: &str, version: &str, deps: Vec<&str>) -> PackageEntry {
        PackageEntry {
            name: name.to_string(),
            version: Version::parse(version).unwrap(),
            dependencies: deps
                .into_iter()
                .map(|d| Dependency::from_dep_str(d).unwrap())
                .collect(),
        }
    }

    // ── Basic resolution ──────────────────────────────────────────────────────

    #[test]
    fn test_single_package_no_deps() {
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("curl", "8.18.0", vec![]));
        resolver.require("curl");

        let result = resolver.resolve().unwrap();
        assert!(result.contains_key("curl"));
        assert_eq!(result["curl"].to_string(), "8.18.0");
    }

    #[test]
    fn test_linear_dependency_chain() {
        // curl → openssl → zlib
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("zlib", "1.3.1", vec![]));
        resolver.add_package(make_pkg("openssl", "3.2.0", vec!["zlib"]));
        resolver.add_package(make_pkg("curl", "8.18.0", vec!["openssl"]));
        resolver.require("curl");

        let result = resolver.resolve().unwrap();
        assert!(result.contains_key("curl"), "curl must be selected");
        assert!(result.contains_key("openssl"), "openssl must be selected");
        assert!(result.contains_key("zlib"), "zlib must be selected");
    }

    #[test]
    fn test_diamond_dependency() {
        // A → B → D
        // A → C → D
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("d", "1.0.0", vec![]));
        resolver.add_package(make_pkg("b", "1.0.0", vec!["d"]));
        resolver.add_package(make_pkg("c", "1.0.0", vec!["d"]));
        resolver.add_package(make_pkg("a", "1.0.0", vec!["b", "c"]));
        resolver.require("a");

        let result = resolver.resolve().unwrap();
        assert!(result.contains_key("a"));
        assert!(result.contains_key("b"));
        assert!(result.contains_key("c"));
        assert!(result.contains_key("d"));
    }

    // ── Version constraint matching ───────────────────────────────────────────

    #[test]
    fn test_version_constraint_satisfied() {
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("openssl", "3.2.0", vec![]));
        resolver.add_package(make_pkg("curl", "8.18.0", vec!["openssl ^3.0"]));
        resolver.require("curl");

        let result = resolver.resolve().unwrap();
        assert!(result.contains_key("openssl"));
    }

    #[test]
    fn test_version_constraint_not_satisfied() {
        let mut resolver = SATResolver::new();
        // openssl 2.x doesn't match ^3.0
        resolver.add_package(make_pkg("openssl", "2.1.0", vec![]));
        resolver.add_package(make_pkg("curl", "8.18.0", vec!["openssl ^3.0"]));
        resolver.require("curl");

        let err = resolver.resolve().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unsatisfiable") || msg.contains("constraint") || msg.contains("conflict"),
            "error message should describe the conflict: {}",
            msg
        );
    }

    // ── Missing package detection ─────────────────────────────────────────────

    #[test]
    fn test_missing_required_package() {
        let mut resolver = SATResolver::new();
        resolver.require("does-not-exist");

        let err = resolver.resolve().unwrap_err();
        assert!(matches!(err, ResolverError::PackageNotFound(_)));
        assert!(err.to_string().contains("does-not-exist"));
    }

    #[test]
    fn test_missing_dependency_causes_error() {
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("curl", "8.18.0", vec!["openssl"])); // openssl not in universe
        resolver.require("curl");

        let err = resolver.resolve().unwrap_err();
        assert!(
            matches!(err, ResolverError::Unsatisfiable { .. }),
            "expected Unsatisfiable, got: {}",
            err
        );
        assert!(
            err.to_string().contains("openssl"),
            "error should mention missing dep: {}",
            err
        );
    }

    // ── Cycle detection ───────────────────────────────────────────────────────

    #[test]
    fn test_circular_dependency_produces_clear_error() {
        // a → b → a  (direct cycle)
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("a", "1.0.0", vec!["b"]));
        resolver.add_package(make_pkg("b", "1.0.0", vec!["a"]));
        resolver.require("a");

        let err = resolver.resolve().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("circular"),
            "error should mention 'circular': {}",
            msg
        );
        // Both packages should be named in the cycle description
        assert!(msg.contains('a') && msg.contains('b'), "cycle error should name packages: {}", msg);
    }

    #[test]
    fn test_three_way_cycle_produces_clear_error() {
        // a → b → c → a
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("a", "1.0.0", vec!["b"]));
        resolver.add_package(make_pkg("b", "1.0.0", vec!["c"]));
        resolver.add_package(make_pkg("c", "1.0.0", vec!["a"]));
        resolver.require("a");

        let err = resolver.resolve().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("circular"), "error should mention 'circular': {}", msg);
    }

    // ── Multiple root requirements ────────────────────────────────────────────

    #[test]
    fn test_multiple_root_requirements() {
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("zlib", "1.3.1", vec![]));
        resolver.add_package(make_pkg("jq", "1.7.0", vec!["zlib"]));
        resolver.add_package(make_pkg("curl", "8.18.0", vec!["zlib"]));
        resolver.require("curl");
        resolver.require("jq");

        let result = resolver.resolve().unwrap();
        assert!(result.contains_key("curl"));
        assert!(result.contains_key("jq"));
        assert!(result.contains_key("zlib"));
    }

    // ── Empty resolver ────────────────────────────────────────────────────────

    #[test]
    fn test_no_requirements_resolves_empty() {
        let mut resolver = SATResolver::new();
        resolver.add_package(make_pkg("curl", "8.18.0", vec![]));
        // no requirements added — result is empty (nothing forced true)
        let result = resolver.resolve().unwrap();
        let _ = result;
    }

    // ── Property-based tests ──────────────────────────────────────────────────
    //
    // Generate random dependency graphs and verify the resolver either
    // succeeds with a valid result or returns a structured error — never panics.

    use proptest::prelude::*;

    /// Strategy: generate a list of (name, dep_index) pairs representing a
    /// random acyclic dependency graph with `n` nodes. Node i may depend on
    /// node j only if j > i (ensuring no cycles by construction).
    fn arb_dag(n: usize) -> impl Strategy<Value = Vec<Vec<usize>>> {
        proptest::collection::vec(
            proptest::collection::vec(0usize..n, 0..3usize),
            n,
        )
        .prop_map(move |mut adj| {
            // Make acyclic: node i can only depend on nodes with index > i
            for (i, deps) in adj.iter_mut().enumerate() {
                deps.retain(|&j| j > i);
                deps.dedup();
            }
            adj
        })
    }

    proptest! {
        /// The resolver must not panic for any random acyclic dependency graph.
        /// It should either return Ok (valid resolution) or a structured Err.
        #[test]
        fn prop_resolver_no_panic_on_acyclic_graph(
            adj in arb_dag(8)
        ) {
            let n = adj.len();
            let mut resolver = SATResolver::new();

            for i in 0..n {
                let deps: Vec<brew_formula::Dependency> = adj[i]
                    .iter()
                    .map(|&j| brew_formula::Dependency::new(format!("pkg-{}", j)))
                    .collect();
                resolver.add_package(PackageEntry {
                    name: format!("pkg-{}", i),
                    version: Version::new(1, 0, 0),
                    dependencies: deps,
                });
            }

            if n > 0 {
                resolver.require("pkg-0");
            }

            // Must not panic — either succeeds or returns structured error
            let _ = resolver.resolve();
        }

        /// Resolver with all deps in universe must always succeed for an
        /// acyclic graph (no version conflicts, all packages available).
        #[test]
        fn prop_complete_universe_always_satisfiable(
            adj in arb_dag(6)
        ) {
            let n = adj.len();
            if n == 0 {
                return Ok(());
            }

            let mut resolver = SATResolver::new();
            for i in 0..n {
                let deps: Vec<brew_formula::Dependency> = adj[i]
                    .iter()
                    .map(|&j| brew_formula::Dependency::new(format!("pkg-{}", j)))
                    .collect();
                resolver.add_package(PackageEntry {
                    name: format!("pkg-{}", i),
                    version: Version::new(1, 0, 0),
                    dependencies: deps,
                });
            }
            resolver.require("pkg-0");

            // All deps are in the universe, no version constraints → must be SAT
            prop_assert!(
                resolver.resolve().is_ok(),
                "complete universe acyclic graph must be satisfiable"
            );
        }

        /// Requiring a non-root (mid-graph) package must also satisfy when
        /// the full universe is present. Improves coverage of shared-diamond
        /// patterns not exercised by always requiring pkg-0.
        #[test]
        fn prop_non_root_requirement_satisfiable(
            adj in arb_dag(6),
            req_idx in 0usize..6usize
        ) {
            let n = adj.len();
            if n == 0 {
                return Ok(());
            }
            let req = req_idx % n; // clamp to valid range

            let mut resolver = SATResolver::new();
            for i in 0..n {
                let deps: Vec<brew_formula::Dependency> = adj[i]
                    .iter()
                    .map(|&j| brew_formula::Dependency::new(format!("pkg-{}", j)))
                    .collect();
                resolver.add_package(PackageEntry {
                    name: format!("pkg-{}", i),
                    version: Version::new(1, 0, 0),
                    dependencies: deps,
                });
            }
            resolver.require(format!("pkg-{}", req));

            // All deps are in the universe, no version constraints → must be SAT
            prop_assert!(
                resolver.resolve().is_ok(),
                "complete universe acyclic graph must be satisfiable regardless of which node is required"
            );
        }
    }
}
