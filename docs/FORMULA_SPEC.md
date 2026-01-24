# Formula Format Specification

## Overview

brew-rs uses a declarative TOML-based formula format for package definitions. This format is designed to be:

- **Human-readable**: Easy to write and understand
- **Fast to parse**: Leverages Serde's performance (300-800 MB/s)
- **Type-safe**: Validates at parse time
- **Extensible**: Easy to add new fields

## Basic Structure

Every formula is a TOML file with the following top-level sections:

```toml
[package]      # Required: Package metadata
[source]       # Required: Source information
[dependencies] # Optional: Package dependencies
[build]        # Optional: Build instructions
[bottle]       # Optional: Pre-built binary information
```

## Package Section

**Required fields:**
- `name`: Package name (lowercase, alphanumeric + hyphens)
- `version`: Semantic version string
- `description`: Short package description

**Optional fields:**
- `homepage`: Project homepage URL
- `license`: SPDX license identifier
- `maintainers`: List of maintainer emails

### Example

```toml
[package]
name = "example"
version = "1.2.3"
description = "An example package for demonstration"
homepage = "https://example.com"
license = "MIT"
maintainers = ["alice@example.com", "bob@example.com"]
```

## Source Section

Defines where to download the package source code.

**Required fields:**
- `url`: Primary download URL
- `sha256`: SHA-256 checksum of the archive

**Optional fields:**
- `mirrors`: Array of mirror URLs for redundancy

### Example

```toml
[source]
url = "https://github.com/example/example/releases/download/v1.2.3/example-1.2.3.tar.gz"
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
mirrors = [
    "https://mirror1.com/example-1.2.3.tar.gz",
    "https://mirror2.com/example-1.2.3.tar.gz"
]
```

## Dependencies Section

Specifies package dependencies with different types:

- `runtime`: Required at runtime (default)
- `build`: Required only during build
- `test`: Required only for testing

### Example

```toml
[dependencies]
runtime = ["openssl", "zlib"]
build = ["cmake", "gcc", "pkg-config"]
test = ["pytest", "coverage"]
```

### Version Constraints

Dependencies can specify version constraints using semver syntax:

```toml
[dependencies]
runtime = [
    "openssl ^3.0",      # Compatible with 3.x
    "zlib >=1.2.11",     # At least 1.2.11
    "curl ~1.7",         # 1.7.x only
]
```

## Build Section

Defines how to build the package from source.

**Fields:**
- `commands`: Array of shell commands to execute
- `env`: Environment variables for build process
- `parallel`: Whether build can run in parallel (default: true)

### Example

```toml
[build]
commands = [
    "./configure --prefix=$PREFIX --with-ssl",
    "make -j$NCPU",
    "make install"
]

[build.env]
CC = "gcc"
CFLAGS = "-O2 -march=native"
LDFLAGS = "-L$PREFIX/lib"
```

### Build Variables

The following variables are available in build commands and env:

- `$PREFIX`: Installation prefix
- `$NCPU`: Number of CPU cores
- `$VERSION`: Package version
- `$NAME`: Package name

## Bottle Section

Pre-built binaries for faster installation.

### Example

```toml
[bottle.macos-arm64]
url = "https://bottles.brew-rs.dev/example-1.2.3.arm64.bottle.tar.gz"
sha256 = "..."

[bottle.macos-x86_64]
url = "https://bottles.brew-rs.dev/example-1.2.3.x86_64.bottle.tar.gz"
sha256 = "..."

[bottle.linux-x86_64]
url = "https://bottles.brew-rs.dev/example-1.2.3.linux.bottle.tar.gz"
sha256 = "..."
```

## Complete Example

```toml
[package]
name = "curl"
version = "8.5.0"
description = "Command-line tool for transferring data with URLs"
homepage = "https://curl.se"
license = "MIT"
maintainers = ["curl-maintainers@brew-rs.dev"]

[source]
url = "https://curl.se/download/curl-8.5.0.tar.gz"
sha256 = "ce4b6a6655431147624aaf582632a36fe1ade262d5fab385c60f78942dd8d87b"
mirrors = [
    "https://github.com/curl/curl/releases/download/curl-8_5_0/curl-8.5.0.tar.gz"
]

[dependencies]
runtime = ["openssl ^3.0", "zlib >=1.2.11", "libssh2"]
build = ["pkg-config", "autoconf", "automake"]
test = ["stunnel"]

[build]
commands = [
    "./configure --prefix=$PREFIX --with-ssl --with-libssh2",
    "make -j$NCPU",
    "make install"
]

[build.env]
PKG_CONFIG_PATH = "$PREFIX/lib/pkgconfig"

[bottle.macos-arm64]
url = "https://bottles.brew-rs.dev/curl-8.5.0.arm64.bottle.tar.gz"
sha256 = "abc123..."

[bottle.macos-x86_64]
url = "https://bottles.brew-rs.dev/curl-8.5.0.x86_64.bottle.tar.gz"
sha256 = "def456..."

[bottle.linux-x86_64]
url = "https://bottles.brew-rs.dev/curl-8.5.0.linux.bottle.tar.gz"
sha256 = "ghi789..."
```

## Validation Rules

Formulae must pass the following validation:

1. **Required fields**: `package.name`, `package.version`, `package.description`, `source.url`, `source.sha256`
2. **Name format**: Lowercase alphanumeric + hyphens only
3. **Version format**: Valid semantic version (x.y.z)
4. **SHA-256 format**: 64 hexadecimal characters
5. **URL format**: Valid HTTP/HTTPS URL
6. **Dependency cycles**: No circular dependencies allowed

## Best Practices

### Checksums

Always provide SHA-256 checksums for security:

```bash
# Generate checksum
sha256sum example-1.2.3.tar.gz
# or on macOS
shasum -a 256 example-1.2.3.tar.gz
```

### Mirrors

Provide multiple mirrors for reliability:

```toml
mirrors = [
    "https://primary-mirror.com/file.tar.gz",
    "https://backup-mirror.org/file.tar.gz",
    "https://github.com/project/repo/releases/download/v1.0.0/file.tar.gz"
]
```

### Build Commands

Use portable shell commands:

```toml
# Good: Portable
commands = ["./configure --prefix=$PREFIX", "make", "make install"]

# Bad: Bash-specific
commands = ["./configure --prefix=${PREFIX}", "make -j$(nproc)"]
```

### Environment Variables

Set necessary env vars for reproducible builds:

```toml
[build.env]
CC = "gcc"
CFLAGS = "-O2"
MAKEFLAGS = "-j$NCPU"
```

## Migration from Homebrew

To convert a Homebrew Ruby formula to brew-rs TOML:

1. **Package info**: `name`, `desc` → `package.name`, `package.description`
2. **URLs**: `url`, `sha256` → `source.url`, `source.sha256`
3. **Dependencies**: `depends_on` → `dependencies.runtime`
4. **Build**: Ruby `install` method → `build.commands`
5. **Bottles**: `bottle do` → `[bottle.*]` sections

### Conversion Tool

A conversion tool is planned for the future to automate this process.

## Future Extensions

Planned additions to the format:

- `[test]` section: Commands to run for package testing
- `[patches]` section: Patches to apply before building
- `[conflicts]` section: Packages that conflict with this one
- `[provides]` section: Virtual packages this provides
- `[cask]` section: GUI application metadata

## Format Version

Current format version: **1.0**

The format version will be incremented for breaking changes, with backward compatibility maintained where possible.
