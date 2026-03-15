# Roadmap

32-week development plan. Each week has a concrete deliverable.

## Phase 1: Foundation (Weeks 1-4)

| Week | Deliverable | Status |
|------|-------------|--------|
| 1 | Project setup, CLI skeleton, TOML formula parsing, validation | Done |
| 2 | Tap system (git repos, TOML registry, FTS5 cache), SQLite package DB, install queue with topological sort | Done |
| 3 | SAT solver dependency resolution (varisat), semver version constraints, conflict detection, benchmarks | Done |
| 4 | Build from source — run configure/make/cmake, install to Cellar, symlink to bin | |

At the end of Phase 1, `brew-rs install curl` should download the source tarball, verify its SHA-256, build it, and link the binary. Dependencies get built first in topological order.

## Phase 2: Core features (Weeks 5-12)

| Week | Deliverable |
|------|-------------|
| 5 | Binary bottles — download pre-built packages instead of compiling |
| 6 | Uninstall and cleanup — remove packages, prune orphaned deps |
| 7 | Upgrade — check for newer versions, rebuild or re-download |
| 8 | Rollback — keep previous versions in Cellar, switch symlinks |
| 9 | Package info and doctor — show metadata, diagnose broken installs |
| 10 | Parallel downloads — fetch multiple tarballs concurrently with progress bars |
| 11 | Build environment isolation — restrict env vars, sandbox filesystem access |
| 12 | Formula linting — validate formulas in CI before merge |

## Phase 3: Security (Weeks 13-16)

| Week | Deliverable |
|------|-------------|
| 13 | GPG signature verification for tarballs and bottles |
| 14 | Build provenance attestations (SLSA Level 2) |
| 15 | CVE scanning — check installed packages against known vulnerabilities |
| 16 | Audit logging — record every install, upgrade, and uninstall |

## Phase 4: Advanced (Weeks 17-24)

| Week | Deliverable |
|------|-------------|
| 17 | Snapshot and restore — save/load full system state |
| 18 | CI/CD pipeline for bottle building |
| 19 | Multi-platform support (macOS ARM, macOS Intel, Linux x86_64) |
| 20 | Optional and conditional dependencies |
| 21 | Multi-version coinstallation (python@3.11, python@3.12) |
| 22 | Plugin system for custom build backends |
| 23 | Performance optimization pass (profiling, memory, startup time) |
| 24 | Self-update mechanism |

## Phase 5: Production (Weeks 25-32)

| Week | Deliverable |
|------|-------------|
| 25-26 | Test coverage to 90%+, integration tests against real formulas |
| 27-28 | User documentation, man pages, shell completions |
| 29-30 | Migration tools — import from Homebrew, export formula conversions |
| 31 | Beta release, community feedback |
| 32 | v1.0 release |

## Design decisions

These are locked in and won't change unless something breaks:

- **TOML formulas, not Ruby.** Declarative, parseable without an interpreter, version-controllable.
- **varisat for SAT solving.** Pure Rust, no C FFI, fast enough for our scale (sub-millisecond for 100-package graphs). If we ever hit performance walls at 10k+ packages, libsolv FFI is the escape hatch.
- **SQLite for state.** WAL mode for concurrent reads, FTS5 for search. One file, no daemon.
- **XDG Base Directory.** Config in `~/.config/brew-rs/`, data in `~/.local/share/brew-rs/`, cache in `~/.cache/brew-rs/`. No cluttering `$HOME`.
- **One version per package (for now).** The SAT encoding supports multiple versions per package, but tap infrastructure doesn't. Week 21 addresses this.

## Non-goals

Things we're deliberately not building:

- Cask support (GUI apps). Different problem, different tool.
- Linux distribution package management. We install from source or bottles into a user-local prefix.
- Replacing system libraries. brew-rs installs alongside, never overwrites `/usr/lib`.
