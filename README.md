# brew-rs 🦀⚡

> A blazing-fast, production-ready package manager written in Rust

[![CI](https://github.com/brew-rs/homebrew-rust/workflows/CI/badge.svg)](https://github.com/brew-rs/homebrew-rust/actions)
[![License](https://img.shields.io/badge/license-blue.svg)](LICENSE)

## Vision

**brew-rs** is a next-generation package manager built from the ground up in Rust, designed to be **10-100x faster** than traditional package managers while maintaining production-grade security and reliability.

### Why brew-rs?

Traditional package managers suffer from fundamental performance bottlenecks:

- **Slow startup**: Interpreter overhead (~1 second just to print version)
- **Sequential operations**: No parallelism in downloads or installations
- **Naive algorithms**: Inefficient dependency resolution
- **Legacy design**: Built before modern performance best practices

brew-rs solves these problems with:

✨ **Sub-100ms command startup** (compiled binary, zero runtime overhead)
⚡ **Aggressive parallelism** (50+ concurrent downloads by default)
🧠 **SAT-based dependency resolution** (100x+ faster than backtracking)
🔒 **Security-first design** (checksums, signatures, build provenance)
🎯 **Clean architecture** (no legacy baggage, pure Rust)

## Performance Targets

| Metric                | brew-rs Target | vs Traditional                |
| --------------------- | -------------- | ----------------------------- |
| Command startup       | <100ms         | **10x faster**                |
| Dependency resolution | <100ms         | **100x faster**               |
| Parallel downloads    | 50+ concurrent | **∞ faster** (was sequential) |
| Formula parsing       | <1ms           | **1000x faster**              |
| Update operations     | <5s            | **10x faster**                |

## Features

### Current (v0.1.0)

- ✅ Workspace architecture with modular crates
- ✅ CLI with core commands (init, install, search, list, tap)
- ✅ TOML-based formula format with Serde parsing
- ✅ Parallel download engine with checksum verification
- ✅ Package state tracking with SQLite (WAL mode, migrations)
- ✅ Tap system with Git repository support
- ✅ Formula cache with FTS5 full-text search
- ✅ Install queue with topological dependency sorting
- ✅ Circular dependency detection
- ✅ Dry-run mode for install planning

### Planned (Roadmap)

- 🚧 **Phase 1** (Weeks 1-4): Foundation
  - ✅ Week 1: Project setup, CLI, formula parsing
  - ✅ Week 2: Tap persistence, formula cache, database, install queue
  - 🚧 Week 3: SAT solver integration
  - 🚧 Week 4: Build from source support

- 🚧 **Phase 2** (Weeks 5-12): Core Features
  - Binary packages (bottles)
  - Upgrade/rollback functionality

- 🚧 **Phase 3** (Weeks 13-16): Security
  - GPG signature verification
  - Build provenance attestations
  - CVE scanning

- 🚧 **Phase 4** (Weeks 17-24): Advanced
  - Snapshot & rollback system
  - CI/CD for bottle building
  - Performance optimizations

- 🚧 **Phase 5** (Weeks 25-32): Production
  - Comprehensive testing (>90% coverage)
  - Full documentation
  - Migration tools

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

brew-rs uses a modular workspace architecture:

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

### Technology Stack

- **Runtime**: Tokio (async I/O, parallelism)
- **HTTP**: Reqwest (concurrent downloads)
- **Parsing**: Serde (TOML/JSON, 300-800 MB/s)
- **CLI**: Clap (modern argument parsing)
- **Solver**: SAT solver (libsolv or pure Rust)
- **Database**: SQLite (rusqlite)
- **Security**: sha2, ring (checksums, signatures)

## Formula Format

brew-rs uses a clean, declarative TOML format:

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
runtime = ["dep1", "dep2"]
build = ["cmake", "gcc"]
test = ["pytest"]

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

Security is a top priority. brew-rs implements:

- ✅ SHA-256 checksum verification (mandatory)
- 🚧 GPG signature verification
- 🚧 Build provenance attestations
- 🚧 CVE scanning
- 🚧 Sandboxed builds

To report security vulnerabilities, please email serendeep10@gmail.com (do not use public issues).

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

Inspired by:

- [Homebrew](https://brew.sh/) - The original package manager for macOS
- [Cargo](https://doc.rust-lang.org/cargo/) - Rust's excellent build system and package manager
- [libsolv](https://github.com/openSUSE/libsolv) - State-of-the-art SAT solver

Built with modern Rust ecosystem tools:

- Tokio, Reqwest, Serde, Clap, and many more excellent crates

## Roadmap

See [ROADMAP.md](docs/ROADMAP.md) for detailed development timeline and milestones.

## Status

🚧 **Early Development** - Not yet ready for production use

Current version: **0.1.0-alpha**
Progress: **Week 2 of 32 complete** (Foundation phase)

### Week 2 Completed Features
- Tap persistence with TOML registry
- Formula cache with SQLite FTS5 full-text search
- Package database with migration system
- Install queue with topological sorting
- Circular dependency detection
- `--dry-run` mode for install planning

---

**Built with ❤️ and 🦀 Rust**
