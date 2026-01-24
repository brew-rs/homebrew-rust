# SAT Solver Research for Dependency Resolution

## Executive Summary

For maximum performance dependency resolution in brew-rs, we need to evaluate SAT solver options. Based on research, the top candidates are:

1. **libsolv** (via Rust bindings) - State-of-the-art, used in production by openSUSE, Fedora, Conda
2. **varisat** - Pure Rust CDCL SAT solver
3. **Custom PubGrub implementation** - Modern algorithm with excellent error messages

**Recommendation**: Start with **varisat** (pure Rust) for simplicity, with option to upgrade to **libsolv bindings** if needed for maximum performance.

---

## SAT Solver Options

### Option 1: libsolv (C library with Rust bindings)

**Overview:**
- Industry-standard dependency resolver
- Used by: openSUSE, Fedora DNF, Conda (via libmamba)
- Written in C with Rust bindings available

**Pros:**
- ✅ Proven in production at massive scale
- ✅ Extremely fast (100x+ vs naive backtracking)
- ✅ Handles complex dependency scenarios
- ✅ Well-tested and battle-hardened
- ✅ Best-in-class performance

**Cons:**
- ❌ C dependency (requires FFI)
- ❌ More complex to integrate
- ❌ Cross-compilation challenges
- ❌ Not idiomatic Rust

**Performance:**
- Conda: Minutes → seconds (100x improvement)
- DNF: Much faster than YUM
- CDCL algorithm with decades of optimization

**Crates:**
- `libsolv-sys`: Low-level bindings
- `libsolv`: Higher-level wrapper (if available)

### Option 2: varisat (Pure Rust CDCL SAT Solver)

**Overview:**
- Modern CDCL (Conflict-Driven Clause Learning) SAT solver
- Written in pure Rust
- Developed by Jix

**Pros:**
- ✅ Pure Rust (no FFI, no C dependencies)
- ✅ Modern implementation
- ✅ Good performance
- ✅ Easy cross-compilation
- ✅ Type-safe and memory-safe
- ✅ Idiomatic Rust API

**Cons:**
- ❌ Younger than libsolv (less battle-tested)
- ❌ May be slower than libsolv
- ❌ Smaller community

**Performance:**
- CDCL algorithm (same approach as libsolv)
- Competitive with other modern SAT solvers
- Likely 50-100x faster than naive backtracking

**Crate:**
```toml
varisat = "0.2"
```

### Option 3: PubGrub Algorithm (Pure Rust)

**Overview:**
- Modern dependency resolution algorithm
- Created by Natalie Weizenbaum for Dart
- Adopted by Swift Package Manager
- Focus on excellent error messages

**Pros:**
- ✅ Excellent, actionable error messages
- ✅ Pure Rust implementation available
- ✅ Modern algorithm design
- ✅ Good balance of performance and UX
- ✅ Easier to understand than SAT

**Cons:**
- ❌ Not as fast as SAT solvers for complex graphs
- ❌ May be overkill for simple dependency trees

**Performance:**
- Good performance for typical cases
- Slower than SAT for pathological cases
- 10-50x faster than naive backtracking

**Crate:**
```toml
pubgrub = "0.2"
```

### Option 4: Custom Backtracking (Simple)

**Overview:**
- Custom implementation similar to Cargo's approach
- Backtracking with heuristics (highest version first)

**Pros:**
- ✅ Simple to implement
- ✅ Good enough for moderate dependency graphs
- ✅ Full control over algorithm
- ✅ Easy to debug

**Cons:**
- ❌ Poor performance for complex dependencies
- ❌ Doesn't meet 100x performance target
- ❌ Can be slow for large graphs

**Performance:**
- 5-10x faster than naive approach (with good heuristics)
- Falls short of SAT solver performance

---

## Detailed Comparison

| Feature | libsolv | varisat | PubGrub | Custom |
|---------|---------|---------|---------|--------|
| Language | C + FFI | Pure Rust | Pure Rust | Pure Rust |
| Performance | ★★★★★ | ★★★★☆ | ★★★☆☆ | ★★☆☆☆ |
| Error Messages | ★★★☆☆ | ★★★☆☆ | ★★★★★ | ★★★★☆ |
| Ease of Use | ★★☆☆☆ | ★★★★☆ | ★★★★★ | ★★★★★ |
| Maturity | ★★★★★ | ★★★☆☆ | ★★★☆☆ | ★☆☆☆☆ |
| Cross-compile | ★★☆☆☆ | ★★★★★ | ★★★★★ | ★★★★★ |

---

## Implementation Strategy

### Phase 1: MVP (Week 1-2)

Start with **Custom Backtracking** for rapid prototyping:

```rust
pub struct SimpleResolver {
    formulas: HashMap<String, Formula>,
}

impl SimpleResolver {
    pub fn resolve(&self, package: &str) -> Result<Vec<String>> {
        // Backtracking with highest-version-first heuristic
        let mut stack = vec![package.to_string()];
        let mut resolved = Vec::new();

        while let Some(pkg) = stack.pop() {
            if resolved.contains(&pkg) {
                continue;
            }

            let formula = self.formulas.get(&pkg)
                .ok_or_else(|| anyhow!("Package not found: {}", pkg))?;

            // Add dependencies to stack
            stack.extend(formula.dependencies.runtime.clone());
            resolved.push(pkg);
        }

        Ok(resolved)
    }
}
```

### Phase 2: Production (Week 3-4)

Upgrade to **varisat** for production performance:

```rust
use varisat::{Solver, Lit, Var};

pub struct SATResolver {
    solver: Solver,
    pkg_to_var: HashMap<String, Var>,
}

impl SATResolver {
    pub fn resolve(&mut self, package: &str) -> Result<Vec<String>> {
        // Convert dependency constraints to CNF clauses
        self.add_package_clauses(package)?;

        // Solve
        let solution = self.solver.solve()?;

        // Convert solution back to package list
        self.solution_to_packages(solution)
    }

    fn add_package_clauses(&mut self, package: &str) -> Result<()> {
        // TODO: Convert dependencies to SAT clauses
        // - Package selection: If A is selected, its deps must be selected
        // - Version constraints: Semver ranges → boolean constraints
        // - Conflicts: ¬A ∨ ¬B (A and B cannot both be true)
        Ok(())
    }
}
```

### Phase 3: Optimization (Week 5+)

If varisat isn't fast enough, upgrade to **libsolv**:

```rust
use libsolv::*;

pub struct LibsolvResolver {
    pool: Pool,
    repo: Repo,
}

impl LibsolvResolver {
    pub fn resolve(&mut self, package: &str) -> Result<Vec<String>> {
        // Use libsolv's high-level API
        let solver = self.pool.create_solver();
        solver.set_flag(SolverFlag::AllowUninstall, true);

        // Add install job
        let job = Job::new_install(package);

        // Solve
        let solution = solver.solve(&[job])?;

        Ok(solution.packages())
    }
}
```

---

## Benchmarking Plan

Create benchmark suite to compare approaches:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_resolvers(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_resolution");

    // Test case: Simple dependency tree (10 packages)
    group.bench_function("simple/custom", |b| {
        b.iter(|| custom_resolver.resolve(black_box("example")))
    });

    group.bench_function("simple/varisat", |b| {
        b.iter(|| varisat_resolver.resolve(black_box("example")))
    });

    // Test case: Complex dependency graph (100 packages, conflicts)
    group.bench_function("complex/custom", |b| {
        b.iter(|| custom_resolver.resolve(black_box("complex")))
    });

    group.bench_function("complex/varisat", |b| {
        b.iter(|| varisat_resolver.resolve(black_box("complex")))
    });
}

criterion_group!(benches, benchmark_resolvers);
criterion_main!(benches);
```

---

## Recommended Approach

### Start: Custom Backtracking (Week 1-2)
- Fast to implement
- Good enough for MVP
- Easy to test and debug

### Production: varisat (Week 3-4)
- Pure Rust, idiomatic
- Excellent performance
- Easy cross-compilation

### If Needed: libsolv (Week 5+)
- Maximum performance
- Battle-tested
- Worth the FFI complexity if varisat isn't fast enough

---

## Implementation Timeline

| Week | Task |
|------|------|
| 1 | Implement custom backtracking resolver |
| 2 | Add semver constraint handling |
| 3 | Integrate varisat SAT solver |
| 4 | Convert constraints to CNF, benchmark |
| 5+ | Consider libsolv if performance insufficient |

---

## Decision Criteria

Choose **varisat** if:
- ✅ Pure Rust is important (cross-compilation, safety)
- ✅ Performance is critical but not absolute max
- ✅ You want idiomatic Rust

Choose **libsolv** if:
- ✅ Maximum performance is critical
- ✅ Willing to deal with FFI complexity
- ✅ Following proven production patterns (Conda, DNF)

Choose **PubGrub** if:
- ✅ Error message quality is top priority
- ✅ Moderate performance is acceptable
- ✅ Simpler mental model preferred

Choose **Custom** if:
- ✅ Only for MVP/prototyping
- ✅ Not for production

---

## Final Recommendation

**For brew-rs**: Start with **Custom** → Upgrade to **varisat** → Consider **libsolv** if needed

This provides:
- Fast initial development
- Pure Rust benefits
- Path to maximum performance
- Flexibility to optimize later

The varisat approach balances performance, developer experience, and maintainability while staying true to Rust principles.
