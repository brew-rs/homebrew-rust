//! Package installer
//!
//! Downloads a source tarball, extracts it, runs the build, links binaries
//! into ~/.local/bin, and records the result in the database.

use anyhow::{Context, Result};
use brew_config::Paths;
use brew_fetcher::Fetcher;
use brew_formula::Formula;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::database::{Database, DependencyType, FileType, InstallAction, InstallHistoryEntry, InstalledPackage, InstallType, PackageDependency, PackageFile};
use crate::{builder, extractor, linker};

/// What a successful install produces.
pub struct InstallResult {
    pub cellar_path: PathBuf,
    pub linked_files: Vec<linker::LinkedFile>,
}

/// Runs the download-extract-build-link pipeline for a single formula.
pub struct Installer {
    paths: Paths,
    fetcher: Fetcher,
}

impl Installer {
    pub fn new(paths: Paths, fetcher: Fetcher) -> Self {
        Self { paths, fetcher }
    }

    /// Download, extract, build, and link a formula.
    ///
    /// Returns early (Ok) if the package is already installed.
    /// Cleans up the cellar directory if any step fails.
    pub async fn install_formula(&self, formula: &Formula) -> Result<InstallResult> {
        let name = formula.name();
        let version = formula.version();

        let cellar_path = self.paths.package_cellar(name, version);

        // Skip if already installed
        if cellar_path.exists() {
            info!("{} {} already in Cellar, skipping", name, version);
            let linked_files = linker::link_package(&cellar_path, &self.paths.bin_dir, &self.paths.cellar_dir)
                .context("Failed to re-link existing package")?;
            return Ok(InstallResult { cellar_path, linked_files });
        }

        // Download tarball (reuse cached copy if SHA-256 matches)
        let tarball_path = self
            .paths
            .downloads_dir
            .join(format!("{}-{}.tar.gz", name, version));

        if tarball_path.exists() && sha256_matches(&tarball_path, &formula.source.sha256)? {
            info!("Using cached tarball: {}", tarball_path.display());
        } else {
            info!("Downloading {} {}...", name, version);
            fs::create_dir_all(&self.paths.downloads_dir)
                .context("Failed to create downloads dir")?;
            self.fetcher
                .download_with_mirrors(
                    &formula.source.url,
                    &formula.source.mirrors,
                    &tarball_path,
                    Some(&formula.source.sha256),
                )
                .await
                .with_context(|| format!("Failed to download {}", name))?;
        }

        // Extract to a temporary build directory
        let build_dir = self
            .paths
            .cache_dir
            .join("builds")
            .join(format!("{}-{}", name, version));
        fs::create_dir_all(&build_dir).context("Failed to create build dir")?;

        if let Err(e) = extractor::extract_tarball(&tarball_path, &build_dir) {
            let _ = fs::remove_dir_all(&build_dir);
            return Err(e.context(format!("Failed to extract {}", name)));
        }

        // Run the build; clean up on failure
        let build_result = builder::run_build(formula, &build_dir, &cellar_path, &self.paths.cellar_dir);
        if let Err(ref e) = build_result {
            // Remove partial cellar dir so a retry starts fresh
            if cellar_path.exists() {
                let _ = fs::remove_dir_all(&cellar_path);
            }
            return Err(anyhow::anyhow!("Build failed for {} {}: {}", name, version, e));
        }

        // Verify the build actually produced something
        if !cellar_path.exists() || is_dir_empty(&cellar_path)? {
            let _ = fs::remove_dir_all(&cellar_path);
            anyhow::bail!(
                "Build for {} {} produced no output in {}",
                name,
                version,
                cellar_path.display()
            );
        }

        // Link binaries into bin_dir
        let linked_files = linker::link_package(&cellar_path, &self.paths.bin_dir, &self.paths.cellar_dir)
            .with_context(|| format!("Failed to link {}", name))?;

        info!("{} {} installed to {}", name, version, cellar_path.display());

        Ok(InstallResult { cellar_path, linked_files })
    }

    /// Record a successful install in the database.
    pub fn record_install(
        &self,
        db: &Database,
        formula: &Formula,
        result: &InstallResult,
    ) -> Result<()> {
        let name = formula.name();
        let version = formula.version();
        let repo = db.packages();

        // Build the package record
        let mut pkg = InstalledPackage::new(
            name.to_string(),
            version.to_string(),
            result.cellar_path.clone(),
        );
        pkg.description = formula.package.description.clone().into();
        pkg.homepage = formula.package.homepage.clone();
        pkg.license = formula.package.license.clone();
        pkg.build_from_source = true;
        pkg.install_type = InstallType::Formula;
        pkg.source_sha256 = Some(formula.source.sha256.clone());
        pkg.linked = !result.linked_files.is_empty();

        let package_id = repo.insert(&pkg).context("Failed to insert package record")?;

        // Walk cellar and record each file
        record_cellar_files(db, package_id, &result.cellar_path, &result.linked_files)?;

        // Record runtime dependencies
        for dep in &formula.dependencies.runtime {
            let dep_record = PackageDependency {
                id: None,
                package_id,
                dependency_name: dep.name.clone(),
                dependency_type: DependencyType::Runtime,
                version_constraint: dep.version_req.as_ref().map(|r| r.to_string()),
                is_satisfied: true,
            };
            repo.add_dependency(&dep_record)
                .context("Failed to record dependency")?;
        }

        // Log the install action
        let history = InstallHistoryEntry::new(
            name.to_string(),
            version.to_string(),
            InstallAction::Install,
            true,
        );
        repo.record_history(&history).context("Failed to record history")?;

        info!("Recorded {} {} in database", name, version);
        Ok(())
    }

    /// Record a failed install attempt in the database history.
    pub fn record_failure(
        &self,
        db: &Database,
        formula: &Formula,
        error: &str,
    ) -> Result<()> {
        let mut history = InstallHistoryEntry::new(
            formula.name().to_string(),
            formula.version().to_string(),
            InstallAction::Install,
            false,
        );
        history.error_message = Some(error.to_string());
        db.packages()
            .record_history(&history)
            .context("Failed to record failure history")?;
        Ok(())
    }
}

// Walk the cellar directory and insert a PackageFile row for every file.
fn record_cellar_files(
    db: &Database,
    package_id: i64,
    cellar_path: &std::path::Path,
    linked_files: &[linker::LinkedFile],
) -> Result<()> {
    // Build a lookup: target path -> symlink path for linked binaries
    let linked_map: std::collections::HashMap<PathBuf, PathBuf> = linked_files
        .iter()
        .map(|lf| (lf.target.clone(), lf.source.clone()))
        .collect();

    for entry in walkdir_files(cellar_path) {
        let file_path = entry?;
        let file_type = classify_file(&file_path, cellar_path);
        let symlink_path = linked_map.get(&file_path).cloned();

        let record = PackageFile {
            id: None,
            package_id,
            file_path: file_path.clone(),
            file_type,
            symlink_path,
        };
        db.packages()
            .add_file(&record)
            .context("Failed to record file")?;
    }
    Ok(())
}

// Classify a file by the top-level subdirectory it lives under.
fn classify_file(file_path: &std::path::Path, cellar_path: &std::path::Path) -> FileType {
    if let Ok(rel) = file_path.strip_prefix(cellar_path) {
        match rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()) {
            Some(ref s) if s == "bin" => FileType::Bin,
            Some(ref s) if s == "lib" => FileType::Lib,
            Some(ref s) if s == "include" => FileType::Include,
            Some(ref s) if s == "share" => FileType::Share,
            Some(ref s) if s == "etc" => FileType::Etc,
            _ => FileType::Other,
        }
    } else {
        FileType::Other
    }
}

// Yield all regular files recursively under `dir`.
fn walkdir_files(
    dir: &std::path::Path,
) -> impl Iterator<Item = Result<PathBuf>> + '_ {
    WalkFiles::new(dir)
}

struct WalkFiles {
    stack: Vec<std::fs::ReadDir>,
}

impl WalkFiles {
    fn new(dir: &std::path::Path) -> Self {
        let stack = match fs::read_dir(dir) {
            Ok(rd) => vec![rd],
            Err(_) => vec![],
        };
        Self { stack }
    }
}

impl Iterator for WalkFiles {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(rd) = self.stack.last_mut() {
            match rd.next() {
                None => {
                    self.stack.pop();
                }
                Some(Err(e)) => return Some(Err(e.into())),
                Some(Ok(entry)) => {
                    let path = entry.path();
                    let ft = match entry.file_type() {
                        Ok(ft) => ft,
                        Err(e) => return Some(Err(e.into())),
                    };
                    if ft.is_dir() {
                        if let Ok(rd2) = fs::read_dir(&path) {
                            self.stack.push(rd2);
                        }
                    } else if ft.is_file() {
                        return Some(Ok(path));
                    }
                    // skip symlinks from walkdir (they're already tracked via linked_files)
                }
            }
        }
        None
    }
}

fn sha256_matches(path: &std::path::Path, expected: &str) -> Result<bool> {
    let data = fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = format!("{:x}", hasher.finalize());
    Ok(hash == expected)
}

fn is_dir_empty(path: &std::path::Path) -> Result<bool> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("Cannot read dir {}", path.display()))?;
    Ok(entries.next().is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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

    #[test]
    fn test_classify_file() {
        let cellar = PathBuf::from("/cellar/pkg/1.0");
        assert_eq!(classify_file(&PathBuf::from("/cellar/pkg/1.0/bin/tool"), &cellar), FileType::Bin);
        assert_eq!(classify_file(&PathBuf::from("/cellar/pkg/1.0/lib/libfoo.a"), &cellar), FileType::Lib);
        assert_eq!(classify_file(&PathBuf::from("/cellar/pkg/1.0/include/foo.h"), &cellar), FileType::Include);
        assert_eq!(classify_file(&PathBuf::from("/cellar/pkg/1.0/share/man"), &cellar), FileType::Share);
        assert_eq!(classify_file(&PathBuf::from("/cellar/pkg/1.0/other/x"), &cellar), FileType::Other);
    }

    #[test]
    fn test_record_install_in_database() {
        let tmp = TempDir::new().unwrap();
        let paths = make_test_paths(&tmp);
        fs::create_dir_all(&paths.db_dir).unwrap();

        let db = Database::open(&paths).unwrap();

        // Create a fake cellar with one binary
        let cellar = paths.package_cellar("test-pkg", "1.0.0");
        let bin_dir = cellar.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("test-pkg"), b"#!/bin/sh").unwrap();

        let formula_toml = r#"
[package]
name = "test-pkg"
version = "1.0.0"
description = "Test package"

[source]
url = "https://example.com/test.tar.gz"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#;
        let formula = Formula::from_str_unchecked(formula_toml).unwrap();

        let fetcher = Fetcher::new().unwrap();
        let installer = Installer::new(paths.clone(), fetcher);

        let result = InstallResult {
            cellar_path: cellar,
            linked_files: vec![],
        };

        installer.record_install(&db, &formula, &result).unwrap();

        assert!(db.packages().is_installed("test-pkg").unwrap());
        let pkg = db.packages().find_by_name("test-pkg").unwrap().unwrap();
        assert_eq!(pkg.version, "1.0.0");
        assert!(pkg.build_from_source);
        assert_eq!(pkg.source_sha256.as_deref(), Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn test_sha256_matches() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("file.bin");
        let content = b"hello world";
        fs::write(&path, content).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected = format!("{:x}", hasher.finalize());

        assert!(sha256_matches(&path, &expected).unwrap());
        assert!(!sha256_matches(&path, "deadbeef").unwrap());
    }
}
