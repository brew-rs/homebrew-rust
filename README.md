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

## What works

- Full build-from-source pipeline: download tarball, verify SHA-256, extract, configure/make/cmake, install to Cellar, symlink to ~/.local/bin
- Dependencies built first in topological order from the SAT resolver output
- Mirror fallback when primary download URL fails
- TOML formula format with validation (name, version, URLs, checksums)
- SAT-based dependency resolution (varisat) with semver constraints and conflict detection
- SQLite package database (WAL mode, migrations, install history)
- Git-based tap system with FTS5 full-text search
- CLI commands: init, install, search, list, tap (add/remove/update/list)
- 118 tests, including property-based fuzzing of the resolver

## Roadmap

- **Phase 1** (Weeks 1-4): Foundation -- done. `brew-rs install curl` builds from source end-to-end.
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
│   ├── cli/          # User-facing CLI (clap)
│   ├── core/         # Installer, builder, extractor, linker, SQLite database
│   ├── solver/       # SAT resolver (varisat) + install queue
│   ├── fetcher/      # HTTP downloads with SHA-256 and mirror fallback
│   ├── formula/      # TOML formula parsing and validation
│   ├── tap/          # Git-based taps + formula cache (FTS5)
│   └── config/       # XDG paths and TOML settings
├── tests/            # Integration tests
├── docs/             # Specs and roadmap
└── examples/         # Symlink to core tap formulas
```

### Stack

Tokio (async), Reqwest (HTTP), Serde (TOML), Clap (CLI), varisat (SAT solver), rusqlite (SQLite), sha2 (checksums), flate2 + tar (extraction).

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

Early development -- not ready for production use. Phase 1 complete (Week 4 of 32).

```
$ brew-rs install curl
Installing zlib 1.3.2...
  zlib 1.3.2 installed
Installing openssl 3.4.4...
  openssl 3.4.4 installed
Installing libssh2 1.11.1...
  libssh2 1.11.1 installed
Installing curl 8.18.0...
  curl 8.18.0 installed

Installed 4 package(s)

$ ~/.local/bin/curl --version
curl 8.18.0 (aarch64-apple-darwin24.6.0) libcurl/8.18.0 OpenSSL/3.4.4 zlib/1.3.2 libssh2/1.11.1
```

Not implemented yet: uninstall, upgrade, info, binary bottles, signature verification.
