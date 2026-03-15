# SAT Solver Dependency Resolution Implementation Plan

Created: 2026-03-15
Status: VERIFIED
Approved: Yes
Iterations: 0
Worktree: No
Type: Feature

## Summary

**Goal:** Replace the stub `Resolver` with a varisat-powered SAT solver that handles semver version constraints, conflict detection, and optimal version selection — feeding resolved packages into the existing `InstallQueue` for ordering.

**Architecture:** New `Dependency` type in formula crate parses `"openssl ^3.0"` into structured `{name, constraint}` at load time. Solver crate gets a `SATResolver` that translates constraints to CNF clauses, solves with varisat, and produces a version-pinned package set. `InstallQueue` then handles topological ordering as before.

**Tech Stack:** varisat 0.2 (SAT solver), semver 1.0 (version parsing/matching — already in workspace)

## Scope

### In Scope
- Structured `Dependency` type with semver constraint parsing in formula crate
- Dependency validation at parse time
- SAT resolver translating version constraints to CNF clauses
- Version constraint types: `^`, `~`, `>=`, `<=`, `>`, `<`, `=`, ranges
- Conflict detection with actionable error messages
- Integration: resolver picks versions → InstallQueue orders them
- Unit tests with property-based testing (proptest)
- Benchmark scaffolding for resolver performance

### Out of Scope
- Multi-version coinstallation (only one version per package)
- Optional/recommended dependencies
- Platform-conditional dependencies
- Actual package installation (Week 4)
- libsolv upgrade path (future optimization)

## Context for Implementer

**Patterns to follow:**
- Error handling: use `thiserror` enums per brew-rs-rust-patterns.md (see `TapError` example). Note: `QueueError` at queue.rs:14 uses manual Display — new SATResolver errors should use thiserror instead for consistency
- Test helpers: `make_formula()` pattern in queue.rs:429 — build formulas from TOML strings
- Serde conventions: `#[serde(default)]`, `Default` impl for optional sections (formula/src/lib.rs:59)
- Validation pattern: `validate_formula()` chain in formula/src/validation.rs:41

**Conventions:**
- Leaf crates (`formula`, `config`) have no sibling deps — keep `formula` independent
- `semver` crate already in workspace deps — use `semver::VersionReq` for constraints
- `anyhow` at call sites, `thiserror` for crate errors

**Key files:**
- `crates/formula/src/lib.rs` — Formula struct, Dependencies (Vec<String> → needs Dependency type)
- `crates/formula/src/validation.rs` — validate_dependencies is a TODO no-op
- `crates/solver/src/lib.rs` — Resolver stub (formulas HashMap, resolve returns empty vec)
- `crates/solver/src/queue.rs` — InstallQueue (topological sort, cycle detection — keep as-is)
- `crates/solver/Cargo.toml` — needs `varisat` dependency added
- `crates/cli/src/main.rs:138-233` — Install command wiring (loads formula → queue → resolve)
- `examples/curl.toml:19` — Already uses versioned deps: `"openssl ^3.0"`, `"zlib >=1.2.11"`

**Gotchas:**
- curl.toml already has versioned dep strings — parser must handle both bare names (`"libssh2"`) and versioned (`"openssl ^3.0"`)
- `Formula::from_str_unchecked()` skips validation — tests use this; new Dependency type must still deserialize from plain strings
- InstallQueue.add_formula() reads `formula.dependencies.runtime` as `Vec<String>` — needs updating after Dependency type change
- `semver::VersionReq::parse("^3.0")` works but `semver::Version::parse("3.0")` fails (needs 3 components). VersionReq is more lenient.

**Domain context:**
- SAT solving maps package selection to boolean satisfiability: each (package, version) pair = boolean variable
- "If curl is selected, at least one openssl version matching ^3.0 must be selected" = implication clause
- "At most one version of openssl can be selected" = at-most-one constraint (pairwise negation)
- varisat uses DIMACS-style variables (positive int = true, negative = false)

## Assumptions

- Each tap provides at most one version per package name — supported by current formula structure (one TOML file = one version). Tasks 3-4 depend on this.
- The `semver` crate's `VersionReq` handles all constraint syntaxes we need (`^`, `~`, `>=`, etc.) — supported by semver crate docs. Tasks 1-2 depend on this.
- varisat 0.2 API is stable and provides `Solver`, `Lit`, `Var` types — supported by SAT_SOLVER_RESEARCH.md. Tasks 3-4 depend on this.
- Formulas in taps will gradually add version constraints; unversioned deps (bare name) must remain valid and mean "any version" — supported by existing curl.toml mixing both formats. Task 1 depends on this.

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| varisat API changed since research doc | Low | High | Pin varisat version; check docs in Task 3 before coding |
| Single-version-per-package limits real-world use | Medium | Medium | Design Dependency type to support version lists; defer multi-version taps to future |
| Constraint parsing edge cases (partial semver like "3.0") | Medium | Low | Use semver::VersionReq which handles partial versions; add proptest fuzzing |
| SAT encoding produces too many clauses for large dep graphs | Low | Medium | Benchmark with synthetic 100+ package graph; optimize clause generation if needed |

## Goal Verification

### Truths
1. `brew-rs install --dry-run curl` resolves versioned dependencies and shows version-pinned install plan
2. Circular dependencies produce a clear error message naming the cycle
3. Conflicting version constraints produce a clear error message naming the conflict
4. Formulas with no version constraints resolve identically to current behavior
5. All 64+ existing tests continue to pass (backward compatibility)
6. Resolver benchmarks complete in <100ms for a 50-package dependency graph

### Artifacts
1. `crates/formula/src/lib.rs` — Dependency struct with VersionReq parsing
2. `crates/formula/src/validation.rs` — validate_dependencies implementation
3. `crates/solver/src/resolver.rs` — SATResolver with varisat integration
4. `crates/solver/src/lib.rs` — Updated Resolver using SATResolver
5. `crates/solver/src/queue.rs` — Updated to accept resolved Dependency types
6. `crates/solver/benches/resolver_bench.rs` — Performance benchmarks

## Progress Tracking

- [x] Task 1: Add Dependency type to formula crate
- [x] Task 2: Implement dependency validation
- [x] Task 3: Build SAT resolver with varisat
- [x] Task 4: Wire SAT resolver into Resolver and InstallQueue
- [x] Task 5: Update CLI install command for resolved versions
- [x] Task 6: Add benchmarks and property-based tests

**Total Tasks:** 6 | **Completed:** 6 | **Remaining:** 0

## Implementation Tasks

### Task 1: Add Dependency Type to Formula Crate

**Objective:** Replace `Vec<String>` in `Dependencies` with `Vec<Dependency>` where `Dependency` parses `"openssl ^3.0"` into structured `{name, version_req}`.

**Dependencies:** None

**Files:**
- Modify: `crates/formula/src/lib.rs` — Add `Dependency` struct, update `Dependencies` fields
- Test: `crates/formula/src/lib.rs` (inline `#[cfg(test)]` module)

**Key Decisions / Notes:**
- `Dependency` implements custom `Deserialize` to parse both `"libssh2"` (bare name) and `"openssl ^3.0"` (name + constraint)
- Bare names get `version_req: None` meaning "any version"
- Use `semver::VersionReq` for constraint parsing
- Implement `Serialize` to round-trip back to string format
- Keep `from_str_unchecked` working — Serde deserialization doesn't validate, validation is separate

**Definition of Done:**
- [ ] `Dependency` struct with `name: String` and `version_req: Option<semver::VersionReq>`
- [ ] Custom Serde impl parses `"openssl ^3.0"` → `Dependency { name: "openssl", version_req: Some(^3.0) }`
- [ ] Custom Serde impl parses `"libssh2"` → `Dependency { name: "libssh2", version_req: None }`
- [ ] All existing formula tests pass
- [ ] New tests for Dependency parsing (bare name, caret, tilde, range, exact)
- [ ] Example formulas (curl.toml, jq.toml, simple.toml) parse without error

**Verify:**
- `cargo test -p brew-formula`
- `cargo build` (check no compile errors in dependent crates)

---

### Task 2: Implement Dependency Validation

**Objective:** Replace the no-op `validate_dependencies()` with real validation that checks dependency name format and version constraint syntax.

**Dependencies:** Task 1

**Files:**
- Modify: `crates/formula/src/validation.rs` — Implement `validate_dependencies()`
- Test: `crates/formula/src/validation.rs` (inline tests)

**Key Decisions / Notes:**
- Validate dep names use same rules as package names (lowercase, alphanumeric + hyphens)
- Validate version_req by attempting `semver::VersionReq::parse()` if present
- Add `ValidationError` variants: `InvalidDependencyName`, `InvalidVersionConstraint`
- Validation runs on `Formula::from_str()` but NOT on `from_str_unchecked()` (existing pattern)

**Definition of Done:**
- [ ] `validate_dependencies()` checks each dependency name and version constraint
- [ ] New `ValidationError` variants for bad dep names and constraints
- [ ] Tests: valid deps pass, invalid dep name fails, invalid constraint fails
- [ ] `cargo test -p brew-formula` — all pass

**Verify:**
- `cargo test -p brew-formula`

---

### Task 3: Build SAT Resolver with Varisat

**Objective:** Create `SATResolver` that translates version constraints to SAT clauses and solves with varisat.

**Dependencies:** Task 1

**Files:**
- Modify: `Cargo.toml` (root) — Add `varisat = "0.2"` to `[workspace.dependencies]`
- Modify: `crates/solver/Cargo.toml` — Add `varisat.workspace = true`
- Create: `crates/solver/src/resolver.rs` — SATResolver implementation
- Modify: `crates/solver/src/lib.rs` — Export new resolver, update Resolver to use it
- Test: `crates/solver/src/resolver.rs` (inline tests)

**Key Decisions / Notes:**
- Each (package_name, version) pair maps to a SAT variable
- Clause types:
  - **Root requirement:** root package variable must be true
  - **Dependency implication:** if package P is selected, at least one version of each dep matching constraint must be selected
  - **At-most-one:** for each package name, at most one version variable is true (pairwise negation)
- `SATResolver` takes a "package universe" (all available packages+versions) and a set of root requirements
- Returns `Resolution` = `HashMap<String, semver::Version>` (package → chosen version)
- Error types: `UnresolvableDependency`, `ConflictingConstraints`, `PackageNotFound`
- For now, each package has one version (from tap). Multi-version support is future work but the SAT encoding naturally supports it.

**Definition of Done:**
- [ ] `varisat` added to solver Cargo.toml
- [ ] `SATResolver` struct with `add_package()`, `require()`, `resolve()` methods
- [ ] Translates constraints to CNF clauses correctly
- [ ] Solves simple dep trees (A→B→C)
- [ ] Solves diamond deps (A→B→D, A→C→D)
- [ ] Detects unsatisfiable constraints with clear error message
- [ ] Tests: linear deps, diamond deps, conflict detection, unresolvable deps
- [ ] `cargo test -p brew-solver` — all pass

**Verify:**
- `cargo test -p brew-solver`

---

### Task 4: Wire SAT Resolver into Resolver and InstallQueue

**Objective:** Update the public `Resolver` to use `SATResolver` internally, and update `InstallQueue` to work with the new `Dependency` type.

**Dependencies:** Task 1, Task 3

**Files:**
- Modify: `crates/solver/src/lib.rs` — Resolver uses SATResolver
- Modify: `crates/solver/src/queue.rs` — Update to handle `Dependency` type in formula deps
- Test: existing tests in both files must pass

**Key Decisions / Notes:**
- `Resolver.resolve()` now returns `Vec<(String, semver::Version)>` (resolved packages with versions)
- `InstallQueue.add_formula()` reads `formula.dependencies.runtime` which is now `Vec<Dependency>` — extract `.name` for the dependency graph
- Keep `InstallQueue` focused on ordering (it doesn't care about versions, just names)
- `Resolver` builds the package universe from loaded formulas, runs SAT, returns pinned versions

**Definition of Done:**
- [ ] `Resolver.resolve()` uses `SATResolver` internally
- [ ] `InstallQueue` compiles and works with `Vec<Dependency>` fields
- [ ] All existing queue tests pass (update `make_formula` helper if needed)
- [ ] Integration: Resolver resolves → results feed into InstallQueue → correct order
- [ ] `cargo test -p brew-solver` — all pass (both resolver and queue tests)
- [ ] `cargo build` — entire workspace compiles (no CLI breakage from API change)

**Verify:**
- `cargo test -p brew-solver`
- `cargo build` (full workspace — catches CLI breakage from Resolver API change)

---

### Task 5: Update CLI Install Command for Resolved Versions

**Objective:** Update the CLI install/dry-run flow to use the new resolver, showing resolved version info.

**Dependencies:** Task 4

**Files:**
- Modify: `crates/cli/src/main.rs` — Update install command handler (lines 138-233)
- Test: manual CLI test with `cargo run --bin brew-rs -- install --dry-run curl`

**Key Decisions / Notes:**
- Current flow: find formula → add to queue → resolve order → display
- New flow: find formula → collect all available formulas → SAT resolve versions → add resolved to queue → resolve order → display
- Dry-run output should show resolved versions: `openssl 3.2.0 (satisfies ^3.0)`
- Error output should show constraint conflicts clearly

**Definition of Done:**
- [ ] `brew-rs install --dry-run curl` shows version-resolved install plan
- [ ] Unresolvable constraints show clear error message
- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` — all tests pass (full suite)

**Verify:**
- `cargo build`
- `cargo test`
- `cargo run --bin brew-rs -- install --dry-run curl` (with a tap loaded)

---

### Task 6: Add Benchmarks and Property-Based Tests

**Objective:** Add performance benchmarks and proptest-based fuzzing for the resolver.

**Dependencies:** Task 3, Task 4

**Files:**
- Modify: `Cargo.toml` (root) — Add `criterion = { version = "0.5", features = ["html_reports"] }` to `[workspace.dependencies]`
- Create: `crates/solver/benches/resolver_bench.rs` — Criterion benchmarks
- Modify: `crates/solver/Cargo.toml` — Add `criterion.workspace = true` under `[dev-dependencies]` and `[[bench]]` with `harness = false`
- Modify: `crates/solver/src/resolver.rs` — Add proptest tests

**Key Decisions / Notes:**
- Benchmark cases: 10-package simple tree, 50-package diamond graph, 100-package stress test
- Use `criterion` for benchmarks (standard Rust benchmarking)
- Proptest: generate random dependency graphs, verify resolver either succeeds or returns a valid error
- Target: <100ms for 50-package graph, <1s for 100-package graph

**Definition of Done:**
- [ ] Criterion benchmark with 3 test cases runs successfully
- [ ] Resolver completes 50-package graph in <100ms
- [ ] Proptest generates random dep graphs and resolver handles them without panic
- [ ] `cargo bench -p brew-solver` runs without errors
- [ ] `cargo test -p brew-solver` — all pass including proptests

**Verify:**
- `cargo test -p brew-solver`
- `cargo bench -p brew-solver`

## Open Questions

None — all decisions made during planning.

## Deferred Ideas

- **Multi-version taps:** Allow taps to provide multiple versions of the same package (e.g., python@3.11, python@3.12). SAT encoding naturally supports this but formula/tap infrastructure doesn't yet.
- **Optional dependencies:** `[dependencies.optional]` section with feature flags.
- **Platform-conditional deps:** `[dependencies.macos]`, `[dependencies.linux]` sections.
- **libsolv upgrade:** If varisat benchmarks show insufficient performance for 1000+ package graphs, consider libsolv FFI bindings.
