//! Installation queue with dependency resolution
//!
//! This module provides:
//! - Dependency-ordered installation queue
//! - Circular dependency detection
//! - Dry-run mode for previewing installations

use anyhow::{Context, Result};
use brew_formula::Formula;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// Error types for queue operations
#[derive(Debug, Clone)]
pub enum QueueError {
    /// A circular dependency was detected
    CircularDependency(Vec<String>),
    /// A required formula was not found
    FormulaNotFound(String),
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueError::CircularDependency(cycle) => {
                write!(f, "Circular dependency detected: {}", cycle.join(" -> "))
            }
            QueueError::FormulaNotFound(name) => {
                write!(f, "Formula not found: {}", name)
            }
        }
    }
}

impl std::error::Error for QueueError {}

/// An item in the installation queue
#[derive(Debug, Clone)]
pub struct QueueItem {
    /// The formula to install
    pub formula: Formula,
    /// Whether this is a dependency (vs explicitly requested)
    pub is_dependency: bool,
    /// Depth in the dependency tree (0 = root)
    pub depth: usize,
}

/// Summary of a dry-run installation
#[derive(Debug, Clone)]
pub struct DryRunSummary {
    /// Packages that will be installed
    pub to_install: Vec<DryRunEntry>,
    /// Packages already installed (will be skipped)
    pub already_installed: Vec<String>,
    /// Total number of dependencies to install
    pub dependency_count: usize,
}

/// Entry in the dry-run summary
#[derive(Debug, Clone)]
pub struct DryRunEntry {
    pub name: String,
    pub version: String,
    pub is_dependency: bool,
    pub dependencies: Vec<String>,
}

impl fmt::Display for DryRunSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.to_install.is_empty() {
            return writeln!(f, "Nothing to install. All packages are already installed.");
        }

        let pkg_count = self.to_install.len();
        let dep_str = if self.dependency_count > 0 {
            format!(" ({} dependencies)", self.dependency_count)
        } else {
            String::new()
        };

        writeln!(f, "Will install {} package(s){}:", pkg_count, dep_str)?;
        writeln!(f)?;

        for entry in &self.to_install {
            if entry.is_dependency {
                writeln!(f, "   -> {} {} (dependency)", entry.name, entry.version)?;
            } else {
                writeln!(f, " * {} {}", entry.name, entry.version)?;
            }
        }

        if !self.already_installed.is_empty() {
            writeln!(f)?;
            writeln!(f, "Already installed ({}):", self.already_installed.len())?;
            for name in &self.already_installed {
                writeln!(f, "   {} (skipped)", name)?;
            }
        }

        Ok(())
    }
}

/// State for cycle detection (three-color algorithm)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// Installation queue with dependency ordering
pub struct InstallQueue {
    /// All formulas by name
    formulas: HashMap<String, Formula>,
    /// Dependency graph: package -> dependencies
    dependencies: HashMap<String, Vec<String>>,
    /// Reverse dependency graph: package -> packages that depend on it
    reverse_deps: HashMap<String, Vec<String>>,
    /// Root packages (explicitly requested for installation)
    roots: HashSet<String>,
    /// Already installed packages (to skip)
    installed: HashSet<String>,
}

impl InstallQueue {
    /// Create a new empty installation queue
    pub fn new() -> Self {
        Self {
            formulas: HashMap::new(),
            dependencies: HashMap::new(),
            reverse_deps: HashMap::new(),
            roots: HashSet::new(),
            installed: HashSet::new(),
        }
    }

    /// Set the list of already installed packages
    pub fn set_installed(&mut self, installed: HashSet<String>) {
        self.installed = installed;
    }

    /// Add a root package to install
    ///
    /// This will also collect all transitive dependencies.
    pub fn add_root(&mut self, formula: Formula) -> Result<()> {
        let name = formula.name().to_string();
        self.roots.insert(name.clone());
        self.add_formula(formula)
    }

    /// Add a formula and its dependencies to the queue
    fn add_formula(&mut self, formula: Formula) -> Result<()> {
        let name = formula.name().to_string();

        // Skip if already added
        if self.formulas.contains_key(&name) {
            return Ok(());
        }

        // Collect runtime dependency names (version constraints handled by SATResolver)
        let deps: Vec<String> = formula
            .dependencies
            .runtime
            .iter()
            .map(|d| d.name.clone())
            .collect();

        // Update reverse dependencies
        for dep in &deps {
            self.reverse_deps
                .entry(dep.clone())
                .or_default()
                .push(name.clone());
        }

        // Store dependency list
        self.dependencies.insert(name.clone(), deps);

        // Store formula
        self.formulas.insert(name, formula);

        Ok(())
    }

    /// Add a dependency formula (called by external code to load dependencies)
    pub fn add_dependency(&mut self, formula: Formula) -> Result<()> {
        self.add_formula(formula)
    }

    /// Detect circular dependencies using DFS with three-color marking
    fn detect_cycles(&self) -> Result<(), QueueError> {
        let mut state: HashMap<String, VisitState> = self
            .formulas
            .keys()
            .map(|k| (k.clone(), VisitState::Unvisited))
            .collect();

        let mut path: Vec<String> = Vec::new();

        for name in self.formulas.keys() {
            if state[name] == VisitState::Unvisited {
                self.dfs_detect_cycle(name, &mut state, &mut path)?;
            }
        }

        Ok(())
    }

    /// DFS helper for cycle detection
    fn dfs_detect_cycle(
        &self,
        name: &str,
        state: &mut HashMap<String, VisitState>,
        path: &mut Vec<String>,
    ) -> Result<(), QueueError> {
        state.insert(name.to_string(), VisitState::Visiting);
        path.push(name.to_string());

        if let Some(deps) = self.dependencies.get(name) {
            for dep in deps {
                // Skip dependencies we don't have formulas for (may be installed)
                if !self.formulas.contains_key(dep) {
                    continue;
                }

                match state.get(dep) {
                    Some(VisitState::Visiting) => {
                        // Found a cycle!
                        let cycle_start = path.iter().position(|n| n == dep).unwrap();
                        let mut cycle: Vec<String> = path[cycle_start..].to_vec();
                        cycle.push(dep.clone()); // Close the cycle
                        return Err(QueueError::CircularDependency(cycle));
                    }
                    Some(VisitState::Unvisited) => {
                        self.dfs_detect_cycle(dep, state, path)?;
                    }
                    _ => {}
                }
            }
        }

        path.pop();
        state.insert(name.to_string(), VisitState::Visited);
        Ok(())
    }

    /// Topological sort using Kahn's algorithm
    ///
    /// Returns packages in dependency-first order (dependencies before dependents).
    fn topological_sort(&self) -> Result<Vec<String>, QueueError> {
        // Calculate in-degrees: how many dependencies each package HAS
        // (not how many packages depend on it)
        let mut in_degree: HashMap<String, usize> = self
            .formulas
            .keys()
            .map(|k| (k.clone(), 0))
            .collect();

        // A package's in-degree = number of its dependencies that are in the queue
        for (name, deps) in &self.dependencies {
            let dep_count = deps
                .iter()
                .filter(|d| self.formulas.contains_key(*d))
                .count();
            in_degree.insert(name.clone(), dep_count);
        }

        // Start with packages that have no dependencies (in-degree 0)
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut sorted: Vec<String> = Vec::new();

        while let Some(name) = queue.pop_front() {
            sorted.push(name.clone());

            // For each package that depends on 'name', reduce its in-degree
            if let Some(dependents) = self.reverse_deps.get(&name) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        if *degree > 0 {
                            *degree -= 1;
                            if *degree == 0 {
                                queue.push_back(dependent.clone());
                            }
                        }
                    }
                }
            }
        }

        // Check for remaining cycles (should be caught by detect_cycles, but just in case)
        if sorted.len() != self.formulas.len() {
            let remaining: Vec<String> = self
                .formulas
                .keys()
                .filter(|k| !sorted.contains(k))
                .cloned()
                .collect();
            return Err(QueueError::CircularDependency(remaining));
        }

        Ok(sorted)
    }

    /// Resolve the queue and return items in installation order
    pub fn resolve(&self) -> Result<Vec<QueueItem>> {
        // Check for circular dependencies
        self.detect_cycles()
            .context("Dependency resolution failed")?;

        // Get topological order
        let order = self.topological_sort()
            .context("Failed to determine installation order")?;

        // Calculate depths
        let depths = self.calculate_depths();

        // Build queue items, filtering out already installed
        let items: Vec<QueueItem> = order
            .into_iter()
            .filter(|name| !self.installed.contains(name))
            .filter_map(|name| {
                self.formulas.get(&name).map(|formula| QueueItem {
                    formula: formula.clone(),
                    is_dependency: !self.roots.contains(&name),
                    depth: *depths.get(&name).unwrap_or(&0),
                })
            })
            .collect();

        Ok(items)
    }

    /// Calculate depth for each package (0 = root, higher = deeper dependency)
    fn calculate_depths(&self) -> HashMap<String, usize> {
        let mut depths: HashMap<String, usize> = HashMap::new();

        // Initialize roots at depth 0
        for root in &self.roots {
            depths.insert(root.clone(), 0);
        }

        // BFS to calculate depths
        let mut queue: VecDeque<String> = self.roots.iter().cloned().collect();
        let mut visited: HashSet<String> = self.roots.clone();

        while let Some(name) = queue.pop_front() {
            let current_depth = *depths.get(&name).unwrap_or(&0);

            if let Some(deps) = self.dependencies.get(&name) {
                for dep in deps {
                    let new_depth = current_depth + 1;
                    let existing_depth = depths.get(dep).copied().unwrap_or(usize::MAX);

                    if new_depth < existing_depth {
                        depths.insert(dep.clone(), new_depth);
                    }

                    if !visited.contains(dep) {
                        visited.insert(dep.clone());
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        depths
    }

    /// Generate a dry-run summary
    pub fn dry_run_summary(&self) -> Result<DryRunSummary> {
        let items = self.resolve()?;

        let already_installed: Vec<String> = self
            .roots
            .iter()
            .filter(|name| self.installed.contains(*name))
            .cloned()
            .collect();

        let dependency_count = items.iter().filter(|item| item.is_dependency).count();

        let to_install: Vec<DryRunEntry> = items
            .into_iter()
            .map(|item| {
                let deps = self
                    .dependencies
                    .get(item.formula.name())
                    .cloned()
                    .unwrap_or_default();

                DryRunEntry {
                    name: item.formula.name().to_string(),
                    version: item.formula.version().to_string(),
                    is_dependency: item.is_dependency,
                    dependencies: deps,
                }
            })
            .collect();

        Ok(DryRunSummary {
            to_install,
            already_installed,
            dependency_count,
        })
    }

    /// Get the number of packages in the queue
    pub fn len(&self) -> usize {
        self.formulas.len()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.formulas.is_empty()
    }
}

impl Default for InstallQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_formula(name: &str, version: &str, deps: Vec<&str>) -> Formula {
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
            runtime = {:?}
            "#,
            name,
            version,
            name,
            deps.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );

        Formula::from_str_unchecked(&toml).unwrap()
    }

    #[test]
    fn test_empty_queue() {
        let queue = InstallQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_single_package() {
        let mut queue = InstallQueue::new();
        let formula = make_formula("curl", "8.5.0", vec![]);

        queue.add_root(formula).unwrap();

        let items = queue.resolve().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].formula.name(), "curl");
        assert!(!items[0].is_dependency);
    }

    #[test]
    fn test_linear_dependencies() {
        let mut queue = InstallQueue::new();

        // A depends on B, B depends on C
        let a = make_formula("a", "1.0", vec!["b"]);
        let b = make_formula("b", "1.0", vec!["c"]);
        let c = make_formula("c", "1.0", vec![]);

        queue.add_root(a).unwrap();
        queue.add_dependency(b).unwrap();
        queue.add_dependency(c).unwrap();

        let items = queue.resolve().unwrap();

        // Should be in dependency-first order: C, B, A
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].formula.name(), "c");
        assert_eq!(items[1].formula.name(), "b");
        assert_eq!(items[2].formula.name(), "a");

        // A is root, B and C are dependencies
        assert!(!items[2].is_dependency);
        assert!(items[1].is_dependency);
        assert!(items[0].is_dependency);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut queue = InstallQueue::new();

        // A -> B -> D
        // A -> C -> D
        let a = make_formula("a", "1.0", vec!["b", "c"]);
        let b = make_formula("b", "1.0", vec!["d"]);
        let c = make_formula("c", "1.0", vec!["d"]);
        let d = make_formula("d", "1.0", vec![]);

        queue.add_root(a).unwrap();
        queue.add_dependency(b).unwrap();
        queue.add_dependency(c).unwrap();
        queue.add_dependency(d).unwrap();

        let items = queue.resolve().unwrap();

        // D should come first, then B and C (order doesn't matter), then A
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].formula.name(), "d");
        assert_eq!(items[3].formula.name(), "a");

        // D should only appear once
        let d_count = items.iter().filter(|i| i.formula.name() == "d").count();
        assert_eq!(d_count, 1);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut queue = InstallQueue::new();

        // A -> B -> C -> A (cycle!)
        let a = make_formula("a", "1.0", vec!["b"]);
        let b = make_formula("b", "1.0", vec!["c"]);
        let c = make_formula("c", "1.0", vec!["a"]);

        queue.add_root(a).unwrap();
        queue.add_dependency(b).unwrap();
        queue.add_dependency(c).unwrap();

        let result = queue.resolve();
        assert!(result.is_err(), "Expected error for circular dependency");

        // Check the full error chain
        let err = result.unwrap_err();
        let err_chain = format!("{:?}", err);
        assert!(
            err_chain.contains("Circular") || err_chain.contains("circular"),
            "Expected circular dependency error in chain, got: {}",
            err_chain
        );
    }

    #[test]
    fn test_skip_installed() {
        let mut queue = InstallQueue::new();

        // A depends on B, but B is already installed
        let a = make_formula("a", "1.0", vec!["b"]);
        let b = make_formula("b", "1.0", vec![]);

        queue.add_root(a).unwrap();
        queue.add_dependency(b).unwrap();

        let mut installed = HashSet::new();
        installed.insert("b".to_string());
        queue.set_installed(installed);

        let items = queue.resolve().unwrap();

        // Only A should be in the queue (B is skipped)
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].formula.name(), "a");
    }

    #[test]
    fn test_dry_run_summary() {
        let mut queue = InstallQueue::new();

        let a = make_formula("a", "1.0", vec!["b"]);
        let b = make_formula("b", "2.0", vec![]);

        queue.add_root(a).unwrap();
        queue.add_dependency(b).unwrap();

        let summary = queue.dry_run_summary().unwrap();

        assert_eq!(summary.to_install.len(), 2);
        assert_eq!(summary.dependency_count, 1);
        assert!(summary.already_installed.is_empty());

        // Check display output
        let display = format!("{}", summary);
        assert!(display.contains("Will install 2 package(s)"));
        assert!(display.contains("a 1.0"));
        assert!(display.contains("b 2.0"));
    }

    #[test]
    fn test_multiple_roots() {
        let mut queue = InstallQueue::new();

        // Both A and B are roots, both depend on C
        let a = make_formula("a", "1.0", vec!["c"]);
        let b = make_formula("b", "1.0", vec!["c"]);
        let c = make_formula("c", "1.0", vec![]);

        queue.add_root(a).unwrap();
        queue.add_root(b).unwrap();
        queue.add_dependency(c).unwrap();

        let items = queue.resolve().unwrap();

        // C should come first, then A and B
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].formula.name(), "c");

        // A and B are roots
        let roots: Vec<_> = items.iter().filter(|i| !i.is_dependency).collect();
        assert_eq!(roots.len(), 2);
    }
}
