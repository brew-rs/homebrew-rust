# Contributing to brew-rs

Thank you for your interest in contributing to brew-rs! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful, inclusive, and professional. We're all here to build something great together.

## Getting Started

### Prerequisites

- Rust 1.75 or higher
- Git
- Familiarity with Rust and package managers

### Development Setup

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/homebrew-rust.git
   cd homebrew-rust
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/Serendeep/homebrew-rust.git
   ```
4. Build the project:
   ```bash
   cargo build
   ```
5. Run tests:
   ```bash
   cargo test
   ```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/bug-description
```

Use descriptive branch names:

- `feature/` for new features
- `fix/` for bug fixes
- `refactor/` for code refactoring
- `docs/` for documentation changes
- `perf/` for performance improvements

### 2. Make Your Changes

- Write clean, idiomatic Rust code
- Follow the existing code style
- Add tests for new functionality
- Update documentation as needed
- Keep commits focused and atomic

### 3. Test Your Changes

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p brew-core

# Run with verbose output
cargo test -- --nocapture

# Run clippy (linter)
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### 4. Commit Your Changes

Use clear, descriptive commit messages:

```bash
git commit -m "feat: add parallel download support"
git commit -m "fix: resolve dependency resolution bug"
git commit -m "docs: update installation instructions"
```

Follow conventional commits format:

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation changes
- `refactor:` code refactoring
- `perf:` performance improvements
- `test:` test additions or modifications
- `chore:` maintenance tasks

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a pull request on GitHub with:

- Clear title and description
- Reference any related issues
- Screenshots for UI changes (if applicable)
- Performance benchmarks (if applicable)

## Code Guidelines

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for consistent formatting
- Address all `cargo clippy` warnings
- Prefer idiomatic Rust patterns

### Documentation

- Add doc comments (`///`) for public APIs
- Include examples in doc comments when helpful
- Update README.md for significant changes
- Keep CHANGELOG.md up to date

### Testing

- Write unit tests for new functionality
- Add integration tests for end-to-end workflows
- Aim for >90% code coverage
- Use property-based testing (proptest) for complex logic

### Performance

- Profile performance-critical code
- Add benchmarks for optimization work
- Document performance characteristics
- Consider memory usage and allocations

## Project Structure

```
homebrew-rust/
├── crates/
│   ├── cli/          # User-facing CLI
│   ├── core/         # Core package manager logic
│   ├── solver/       # Dependency resolution
│   ├── fetcher/      # Parallel downloads
│   └── formula/      # Formula parsing
├── tests/            # Integration tests
├── docs/             # Documentation
├── examples/         # Example formulae
└── .github/          # GitHub Actions CI/CD
```

## Pull Request Process

1. **Ensure CI passes**: All tests, clippy, and formatting checks must pass
2. **Update documentation**: Keep docs in sync with code changes
3. **Add changelog entry**: Document user-facing changes
4. **Request review**: Tag relevant maintainers
5. **Address feedback**: Respond to review comments promptly
6. **Squash commits**: Clean up commit history before merging (if requested)

## Reporting Bugs

Use GitHub Issues with:

- Clear, descriptive title
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, etc.)
- Error messages and logs
- Minimal reproducible example

## Feature Requests

Use GitHub Issues with:

- Clear description of the feature
- Use cases and motivation
- Proposed implementation (if you have ideas)
- Willingness to contribute the implementation

## Security Issues

**DO NOT** create public issues for security vulnerabilities.

Email security@brew-rs.dev with:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

## License

By contributing, you agree that your contributions will be licensed under the MIT license.

## Questions?

- Open a discussion on GitHub Discussions
- Join our community chat (coming soon)
- Check existing issues and documentation

Thank you for contributing to brew-rs!
