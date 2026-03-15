//! End-to-end install pipeline test.
//!
//! Runs the full Installer pipeline against a synthetic formula and a
//! programmatically-generated tarball, so no network access or real build
//! tools are needed. Everything lives under a TempDir — the real system
//! directories are never touched.
//!
//! The trick for bypassing the network: the Installer skips the download step
//! when the tarball already exists at `downloads_dir/<name>-<version>.tar.gz`
//! and its SHA-256 matches. We pre-place the tarball there before calling
//! `install_formula`, so the Fetcher is never invoked.

use brew_config::Paths;
use brew_core::{database::Database, installer::Installer};
use brew_fetcher::Fetcher;
use brew_formula::Formula;
use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::TempDir;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn make_test_paths(tmp: &TempDir) -> Paths {
    let mut paths = Paths::new().unwrap();
    paths.cellar_dir = tmp.path().join("cellar");
    paths.bin_dir = tmp.path().join("bin");
    paths.downloads_dir = tmp.path().join("downloads");
    paths.cache_dir = tmp.path().join("cache");
    paths.db_dir = tmp.path().join("db");
    paths.db_file = tmp.path().join("db").join("packages.db");
    paths.data_dir = tmp.path().join("data");
    paths
}

/// Create a minimal tar.gz containing a single placeholder file under the
/// standard `<name>-<version>/` prefix, then place it at the path the Installer
/// expects so the download step is skipped.
///
/// Returns the sha256 hex string of the tarball bytes.
fn plant_tarball(paths: &Paths, name: &str, version: &str) -> String {
    fs::create_dir_all(&paths.downloads_dir).unwrap();

    let buf: Vec<u8> = Vec::new();
    let gz = GzEncoder::new(buf, Compression::default());
    let mut tar = tar::Builder::new(gz);

    let content = b"# placeholder source";
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(
        &mut header,
        format!("{}-{}/placeholder.txt", name, version),
        &content[..],
    )
    .unwrap();

    let gz = tar.into_inner().unwrap();
    let bytes = gz.finish().unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = format!("{:x}", hasher.finalize());

    // Place it exactly where the Installer looks for a cached download
    let tarball_path = paths
        .downloads_dir
        .join(format!("{}-{}.tar.gz", name, version));
    fs::write(&tarball_path, &bytes).unwrap();

    sha256
}

/// Build a Formula whose `[build]` commands create a tiny shell script binary
/// under `$PREFIX/bin/`. The url and sha256 are filled in but the url is never
/// fetched because the tarball is already at the expected cache path.
fn make_formula(sha256: &str) -> Formula {
    let toml = format!(
        r#"
[package]
name = "test-pkg"
version = "1.0.0"
description = "Integration test package"

[source]
url = "https://example.com/test-pkg-1.0.0.tar.gz"
sha256 = "{}"

[build]
commands = [
    "mkdir -p $PREFIX/bin",
    "printf '#!/bin/sh\\necho hello from test-pkg\\n' > $PREFIX/bin/test-pkg",
    "chmod +x $PREFIX/bin/test-pkg",
]
"#,
        sha256
    );
    Formula::from_str_unchecked(&toml).unwrap()
}

// ─── tests ────────────────────────────────────────────────────────────────────

/// Full pipeline: extract → build → link → record.
/// Checks cellar layout, symlink, and database state.
#[tokio::test]
#[ignore] // needs filesystem access; run with: cargo test --test install_integration -p brew-core -- --ignored
async fn test_full_install_pipeline() {
    let tmp = TempDir::new().unwrap();
    let paths = make_test_paths(&tmp);
    fs::create_dir_all(&paths.db_dir).unwrap();

    // Plant a pre-built tarball so the Installer skips the network download
    let sha256 = plant_tarball(&paths, "test-pkg", "1.0.0");
    let formula = make_formula(&sha256);

    let fetcher = Fetcher::new().unwrap();
    let installer = Installer::new(paths.clone(), fetcher);
    let db = Database::open(&paths).unwrap();

    // Run the full pipeline — this must not error
    let result = installer.install_formula(&formula).await.unwrap();

    // Cellar directory must exist with the built binary inside it
    let cellar = paths.package_cellar("test-pkg", "1.0.0");
    assert!(cellar.exists(), "cellar dir should exist after install");

    let binary_in_cellar = cellar.join("bin/test-pkg");
    assert!(
        binary_in_cellar.exists(),
        "binary should be present in cellar at {}",
        binary_in_cellar.display()
    );

    // Symlink must exist in bin_dir and point into the cellar
    let symlink = paths.bin_dir.join("test-pkg");
    assert!(symlink.is_symlink(), "symlink should be created in bin_dir");
    let target = fs::read_link(&symlink).unwrap();
    assert!(
        target.ends_with("bin/test-pkg"),
        "symlink should point to cellar bin/test-pkg, got: {}",
        target.display()
    );

    // install_formula returns the list of created symlinks
    assert!(
        !result.linked_files.is_empty(),
        "install result should include at least one linked file"
    );

    // Record in database, then verify via the query API
    installer.record_install(&db, &formula, &result).unwrap();

    assert!(
        db.packages().is_installed("test-pkg").unwrap(),
        "database should report test-pkg as installed"
    );

    let pkg = db.packages().find_by_name("test-pkg").unwrap().unwrap();
    assert_eq!(pkg.version, "1.0.0");
    assert!(pkg.build_from_source, "package should be recorded as built from source");
    assert_eq!(
        pkg.source_sha256.as_deref(),
        Some(sha256.as_str()),
        "source sha256 should be stored"
    );
}

/// A second call to install_formula for a package that's already in the Cellar
/// should succeed immediately without rebuilding anything.
#[tokio::test]
#[ignore]
async fn test_skip_already_installed() {
    let tmp = TempDir::new().unwrap();
    let paths = make_test_paths(&tmp);
    fs::create_dir_all(&paths.db_dir).unwrap();

    let sha256 = plant_tarball(&paths, "test-pkg", "1.0.0");
    let formula = make_formula(&sha256);

    let fetcher = Fetcher::new().unwrap();
    let installer = Installer::new(paths.clone(), fetcher);

    // First install — builds and links
    installer.install_formula(&formula).await.unwrap();

    // Second install — cellar dir already exists, so it should return Ok
    // without running any build commands
    let result = installer.install_formula(&formula).await;
    assert!(result.is_ok(), "second install should succeed without error");
}
