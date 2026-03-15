//! Symlink management: link Cellar binaries into ~/.local/bin

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// One symlink: bin_dir entry pointing at a Cellar binary.
#[derive(Debug, Clone)]
pub struct LinkedFile {
    /// Symlink location (in bin_dir)
    pub source: PathBuf,
    /// Target (in Cellar bin/)
    pub target: PathBuf,
}

/// Symlink each file in `cellar_path/bin/` into `bin_dir`.
///
/// Existing symlinks that point into `cellar_root` (a brew-rs symlink) get
/// replaced. Symlinks pointing elsewhere are left alone with a warning.
/// Returns the list of symlinks created.
pub fn link_package(cellar_path: &Path, bin_dir: &Path, cellar_root: &Path) -> Result<Vec<LinkedFile>> {
    let bin_src = cellar_path.join("bin");
    if !bin_src.exists() {
        info!("No bin/ directory in {}", cellar_path.display());
        return Ok(vec![]);
    }

    std::fs::create_dir_all(bin_dir)
        .with_context(|| format!("Failed to create bin_dir: {}", bin_dir.display()))?;

    let mut linked = Vec::new();

    for entry in std::fs::read_dir(&bin_src)
        .with_context(|| format!("Failed to read bin dir: {}", bin_src.display()))?
    {
        let entry = entry.context("Failed to read bin entry")?;
        let file_name = entry.file_name();
        let symlink_path = bin_dir.join(&file_name);
        let target = entry.path();

        // If symlink already exists
        if symlink_path.exists() || symlink_path.is_symlink() {
            if symlink_path.is_symlink() {
                // Check if it points into a Cellar path (safe to replace)
                match std::fs::read_link(&symlink_path) {
                    Ok(existing_target) => {
                        if existing_target.starts_with(cellar_root) {
                            std::fs::remove_file(&symlink_path).with_context(|| {
                                format!("Failed to remove old symlink: {}", symlink_path.display())
                            })?;
                        } else {
                            warn!(
                                "Skipping {}: exists and is not a brew-rs symlink",
                                symlink_path.display()
                            );
                            continue;
                        }
                    }
                    Err(_) => {
                        // Dangling symlink — remove it
                        let _ = std::fs::remove_file(&symlink_path);
                    }
                }
            } else {
                warn!(
                    "Skipping {}: regular file exists at symlink target",
                    symlink_path.display()
                );
                continue;
            }
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink_path).with_context(|| {
            format!(
                "Failed to create symlink {} -> {}",
                symlink_path.display(),
                target.display()
            )
        })?;

        #[cfg(not(unix))]
        {
            return Err(anyhow::anyhow!("Symlink creation not supported on non-Unix platforms"));
        }

        info!(
            "Linked {} -> {}",
            symlink_path.display(),
            target.display()
        );

        linked.push(LinkedFile {
            source: symlink_path,
            target,
        });
    }

    Ok(linked)
}

/// Remove symlinks in `bin_dir` that point into `cellar_path`.
pub fn unlink_package(cellar_path: &Path, bin_dir: &Path) -> Result<()> {
    if !bin_dir.exists() {
        return Ok(());
    }

    let cellar_str = cellar_path.to_string_lossy().to_lowercase();

    for entry in std::fs::read_dir(bin_dir)
        .with_context(|| format!("Failed to read bin_dir: {}", bin_dir.display()))?
    {
        let entry = entry.context("Failed to read bin_dir entry")?;
        let path = entry.path();

        if path.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                if target
                    .to_string_lossy()
                    .to_lowercase()
                    .starts_with(&cellar_str)
                {
                    std::fs::remove_file(&path).with_context(|| {
                        format!("Failed to remove symlink: {}", path.display())
                    })?;
                    info!("Unlinked {}", path.display());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_cellar_bin(tmp: &TempDir, binaries: &[&str]) -> PathBuf {
        let bin = tmp.path().join("cellar/pkg/1.0/bin");
        fs::create_dir_all(&bin).unwrap();
        for name in binaries {
            let path = bin.join(name);
            fs::write(&path, b"#!/bin/sh\necho test").unwrap();
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        tmp.path().join("cellar/pkg/1.0")
    }

    #[test]
    fn test_link_creates_symlinks() {
        let tmp = TempDir::new().unwrap();
        let cellar = make_cellar_bin(&tmp, &["mypkg"]);
        let bin_dir = tmp.path().join("bin");

        let cellar_root = tmp.path().join("cellar");
        let linked = link_package(&cellar, &bin_dir, &cellar_root).unwrap();

        assert_eq!(linked.len(), 1);
        let link = bin_dir.join("mypkg");
        assert!(link.is_symlink(), "mypkg should be a symlink");
        let target = fs::read_link(&link).unwrap();
        assert!(target.ends_with("bin/mypkg"));
    }

    #[test]
    fn test_link_no_bin_dir() {
        let tmp = TempDir::new().unwrap();
        let cellar = tmp.path().join("cellar/empty/1.0");
        fs::create_dir_all(&cellar).unwrap();
        let bin_dir = tmp.path().join("bin");

        let cellar_root = tmp.path().join("cellar");
        let linked = link_package(&cellar, &bin_dir, &cellar_root).unwrap();
        assert!(linked.is_empty(), "no symlinks if no bin/ exists");
    }

    #[test]
    fn test_link_replaces_existing_cellar_symlink() {
        let tmp = TempDir::new().unwrap();
        // New cellar: cellar/pkg/1.0 (contains "pkg" in path)
        let cellar = make_cellar_bin(&tmp, &["tool"]);
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        // Pre-create a fake old Cellar symlink pointing to prev/1.0
        let symlink = bin_dir.join("tool");
        let prev_target = tmp.path().join("cellar/prev/1.0/bin/tool");
        fs::create_dir_all(prev_target.parent().unwrap()).unwrap();
        fs::write(&prev_target, b"prev").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&prev_target, &symlink).unwrap();

        let cellar_root = tmp.path().join("cellar");
        let linked = link_package(&cellar, &bin_dir, &cellar_root).unwrap();
        assert_eq!(linked.len(), 1, "should create one symlink");

        // Symlink should now point to the NEW cellar (pkg/1.0), not prev/1.0
        let new_target = fs::read_link(&symlink).unwrap();
        let new_target_str = new_target.to_string_lossy();
        assert!(
            new_target_str.contains("pkg") && new_target_str.contains("bin/tool"),
            "symlink should point to new cellar/pkg, got: {}",
            new_target_str
        );
    }

    #[test]
    fn test_unlink_removes_cellar_symlinks() {
        let tmp = TempDir::new().unwrap();
        let cellar = make_cellar_bin(&tmp, &["util"]);
        let bin_dir = tmp.path().join("bin");

        let cellar_root = tmp.path().join("cellar");
        link_package(&cellar, &bin_dir, &cellar_root).unwrap();
        assert!(bin_dir.join("util").is_symlink());

        unlink_package(&cellar, &bin_dir).unwrap();
        assert!(!bin_dir.join("util").exists(), "symlink should be removed");
    }

    #[test]
    fn test_unlink_ignores_non_cellar_symlinks() {
        let tmp = TempDir::new().unwrap();
        let cellar = make_cellar_bin(&tmp, &["pkg"]);
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        // Create a non-Cellar symlink
        let other = tmp.path().join("other_tool");
        fs::write(&other, b"other").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&other, bin_dir.join("other_tool")).unwrap();

        let cellar_root = tmp.path().join("cellar");
        link_package(&cellar, &bin_dir, &cellar_root).unwrap();
        unlink_package(&cellar, &bin_dir).unwrap();

        // non-Cellar symlink should survive
        assert!(
            bin_dir.join("other_tool").exists(),
            "non-cellar symlink should not be removed"
        );
    }
}
