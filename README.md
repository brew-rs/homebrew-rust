# brew-rs 🦀⚡

A package manager written in Rust. Faster than Homebrew, with SAT-based dependency resolution.

[![CI](https://github.com/brew-rs/homebrew-rust/workflows/CI/badge.svg)](https://github.com/brew-rs/homebrew-rust/actions)
[![License](https://img.shields.io/badge/license-blue.svg)](LICENSE)

## Why

Homebrew takes about a second just to print its version — that's Ruby interpreter startup. Downloads are sequential. Dependency resolution uses backtracking.

brew-rs is a compiled binary that starts in under 100ms, downloads packages in parallel, and resolves dependencies with a SAT solver (varisat). The trade-off is maturity: Homebrew has 15+ years of edge cases handled. brew-rs is in early development.

## Performance targets

| Metric                | brew-rs target | Homebrew baseline        |
| --------------------- | -------------- | ------------------------ |
| Command startup       | <100ms         | ~1s (Ruby interpreter)   |
| Dependency resolution | <100ms         | Seconds (backtracking)   |
| Parallel downloads    | 50+ concurrent | Sequential               |
| Formula parsing       | <1ms           | ~100ms (Ruby eval)       |
| Update operations     | <5s            | ~30-60s                  |

## What's implemented

- Modular workspace (cli, core, solver, fetcher, formula, tap, config)
- CLI commands: init, install, search, list, tap (add/remove/update/list)
- TOML formula format with Serde parsing and validation
- Parallel download engine with SHA-256 checksum verification
- SQLite package database (WAL mode, migrations)
- Git-based tap system with FTS5 full-text search
- Install queue with topological sort and cycle detection
- SAT-based dependency resolution (varisat) with semver constraints
- Version conflict detection with clear error messages
- Property-based testing (proptest) and Criterion benchmarks

## Roadmap

- **Phase 1** (Weeks 1-4): Foundation -- Weeks 1-3 done, Week 4 (build from source) in progress
- **Phase 2** (Weeks 5-12): Binary bottles, uninstall, upgrade, rollback
- **Phase 3** (Weeks 13-16): GPG signatures, build provenance, CVE scanning
- **Phase 4** (Weeks 17-24): Snapshots, multi-platform, plugins
- **Phase 5** (Weeks 25-32): 90%+ test coverage, documentation, v1.0

See [ROADMAP.md](docs/ROADMAP.md) for the full 32-week plan.

## Installation

### From Source

```bash
git clone https://github.com/brew-rs/homebrew-rust.git
cd homebrew-rust
cargo build --release
sudo cp target/release/brew-rs /usr/local/bin/
```

### From Binary (Coming Soon)

```bash
curl -fsSL https://brew-rs.serendeep.tech/install.sh | sh
```

## Quick Start

```bash
# Initialize brew-rs directories
brew-rs init

# Add a tap (formula repository)
brew-rs tap add <name> <git-url>

# Search for a package
brew-rs search <query>

# Preview installation (dry-run)
brew-rs install --dry-run <package>

# Install a package
brew-rs install <package>

# List installed packages
brew-rs list

# List installed taps
brew-rs tap list

# Update tap repositories
brew-rs tap update

# Remove a tap
brew-rs tap remove <name>

# Uninstall a package
brew-rs uninstall <package>
```

## Architecture

Workspace layout:

```
homebrew-rust/
├── crates/
│   ├── cli/          # User-facing CLI (clap-based)
│   ├── core/         # Core package manager logic + SQLite database
│   ├── solver/       # Dependency resolution + install queue
│   ├── fetcher/      # Parallel download engine (Tokio)
│   ├── formula/      # TOML formula parsing (Serde)
│   ├── tap/          # Tap management + formula cache (FTS5)
│   └── config/       # Configuration + XDG paths
├── tests/            # Integration tests
├── docs/             # Documentation
└── examples/         # Example formulae
```

### Technology stack

Tokio (async runtime), Reqwest (HTTP), Serde (TOML/JSON parsing), Clap (CLI), varisat (CDCL SAT solver), rusqlite (SQLite), sha2 + ring (checksums).

## Formula format

Formulas are TOML files. See [FORMULA_SPEC.md](docs/FORMULA_SPEC.md) for the full spec. Here's a typical one:

```toml
[package]
name = "example"
version = "1.0.0"
description = "An example package"
homepage = "https://example.com"
license = "MIT"

[source]
url = "https://example.com/release-1.0.0.tar.gz"
sha256 = "abc123..."
mirrors = ["https://mirror1.com/release.tar.gz"]

[dependencies]
runtime = ["openssl ^3.0", "zlib >=1.2.11", "libssh2"]
build = ["cmake", "pkg-config"]
test = ["check"]

[build]
commands = [
    "./configure --prefix=$PREFIX",
    "make",
    "make install"
]

[build.env]
CC = "gcc"
CFLAGS = "-O2"
```

## Development

### Prerequisites

- Rust 1.75+ (MSRV)
- Git

### Building

```bash
# Build all crates
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run clippy (linter)
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Running

```bash
# Run CLI in development
cargo run --bin brew-rs -- --help

# Run with verbose logging
cargo run --bin brew-rs -- -v install example
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific crate tests
cargo test -p brew-core
```

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Run clippy (`cargo clippy`)
6. Format code (`cargo fmt`)
7. Commit changes (`git commit -m 'Add amazing feature'`)
8. Push to branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

## Security

SHA-256 checksum verification is mandatory for all downloads. GPG signatures, build provenance attestations, CVE scanning, and sandboxed builds are planned.

To report vulnerabilities, email serendeep10@gmail.com (not public issues).

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

Inspired by [Homebrew](https://brew.sh/), [Cargo](https://doc.rust-lang.org/cargo/), and [libsolv](https://github.com/openSUSE/libsolv).

## Status

Early development -- not ready for production use. Version 0.1.0-alpha, Week 3 of 32.

What works:

- `brew-rs init` sets up XDG-compliant directory structure
- `brew-rs tap add/remove/update/list` manages git-based formula repositories
- `brew-rs search` does FTS5 full-text search across all loaded taps
- `brew-rs install --dry-run curl` resolves the full dependency tree:
  ```
  Resolved 4 package(s) for curl:

    zlib 1.3.2 (satisfies >=1.2.11) (dependency)
    openssl 3.4.4 (satisfies ^3.0) (dependency)
    libssh2 1.11.1 (dependency)
    curl 8.18.0
  ```
- 98 tests passing, including property-based fuzzing of the resolver

What doesn't work yet:

- Actual installation (Week 4 -- build from source)
- Uninstall, upgrade, info commands
- Binary bottles
- Signature verification

---

**Built with ❤️ and 🦀 Rust**
