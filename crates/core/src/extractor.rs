//! Archive extraction for source tarballs
//!
//! Supports tar.gz format. Strips the top-level directory that most
//! source tarballs include (e.g., `curl-8.5.0/` prefix becomes the root).

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tar::{Archive, EntryType};

/// Extract a tar.gz to `dest_dir`, stripping the common top-level directory.
///
/// Most source tarballs nest everything under one folder (e.g. `curl-8.5.0/`).
/// This function detects that and strips it, so `configure` ends up at
/// `dest_dir/configure` instead of `dest_dir/curl-8.5.0/configure`.
pub fn extract_tarball(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create destination: {}", dest_dir.display()))?;

    // ── First pass: collect all entry paths to detect common prefix ──────────
    let prefix = detect_common_prefix(archive_path)?;

    // ── Second pass: extract, stripping the common prefix ────────────────────
    let file = fs::File::open(archive_path)
        .with_context(|| format!("Cannot open archive: {}", archive_path.display()))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    for entry in archive
        .entries()
        .context("Failed to read archive entries")?
    {
        let mut entry = entry.context("Failed to read archive entry")?;

        if is_pax_header(&entry.header().entry_type()) {
            continue;
        }

        let entry_path = entry.path().context("Invalid path in archive")?.into_owned();

        // Strip the common prefix (e.g., "curl-8.5.0/") from each entry
        let relative_path = if let Some(ref pfx) = prefix {
            match entry_path.strip_prefix(pfx) {
                Ok(p) => p.to_path_buf(),
                Err(_) => entry_path.clone(),
            }
        } else {
            entry_path.clone()
        };

        // Skip the prefix directory itself (would create an empty dir at dest_dir root)
        if relative_path.as_os_str().is_empty() {
            continue;
        }

        let dest = dest_dir.join(&relative_path);

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir: {}", parent.display()))?;
        }

        entry
            .unpack(&dest)
            .with_context(|| format!("Failed to extract: {}", relative_path.display()))?;
    }

    Ok(())
}

/// Scan entries and return the shared top-level directory, if there is exactly one.
/// Returns `None` when entries live under multiple roots (no stripping needed).
fn detect_common_prefix(archive_path: &Path) -> Result<Option<PathBuf>> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("Cannot open archive: {}", archive_path.display()))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    let mut prefixes: HashSet<PathBuf> = HashSet::new();

    for entry in archive
        .entries()
        .context("Failed to read archive entries")?
    {
        let entry = entry.context("Failed to read archive entry")?;

        // Skip pax global/extended headers (GitHub tarballs include these)
        if is_pax_header(&entry.header().entry_type()) {
            continue;
        }

        let path = entry.path().context("Invalid path in archive")?.into_owned();

        // Get first component
        if let Some(first) = path.components().next() {
            prefixes.insert(PathBuf::from(first.as_os_str()));
        }
    }

    // If exactly one prefix, strip it
    if prefixes.len() == 1 {
        Ok(prefixes.into_iter().next())
    } else {
        Ok(None)
    }
}

fn is_pax_header(entry_type: &EntryType) -> bool {
    matches!(*entry_type, EntryType::XGlobalHeader | EntryType::XHeader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tempfile::TempDir;

    /// Create an in-memory tar.gz with a top-level `pkg-1.0/` directory
    fn make_tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let gz = GzEncoder::new(buf, Compression::default());
        let mut tar = tar::Builder::new(gz);

        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, *content).unwrap();
        }

        let gz = tar.into_inner().unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn test_extract_strips_top_level_dir() {
        let tmp = TempDir::new().unwrap();
        let archive_path = tmp.path().join("pkg.tar.gz");
        let dest = tmp.path().join("out");

        let tarball = make_tarball(&[
            ("pkg-1.0/configure", b"#!/bin/sh\n"),
            ("pkg-1.0/src/main.c", b"int main() { return 0; }"),
        ]);
        fs::write(&archive_path, &tarball).unwrap();

        extract_tarball(&archive_path, &dest).unwrap();

        assert!(dest.join("configure").exists(), "configure should be at dest root");
        assert!(dest.join("src/main.c").exists(), "src/main.c should be at dest/src/");
    }

    #[test]
    fn test_extract_no_common_prefix() {
        let tmp = TempDir::new().unwrap();
        let archive_path = tmp.path().join("flat.tar.gz");
        let dest = tmp.path().join("out");

        // No common prefix — entries at different roots
        let tarball = make_tarball(&[
            ("fileA.txt", b"hello"),
            ("subdir/fileB.txt", b"world"),
        ]);
        fs::write(&archive_path, &tarball).unwrap();

        extract_tarball(&archive_path, &dest).unwrap();

        assert!(dest.join("fileA.txt").exists());
        assert!(dest.join("subdir/fileB.txt").exists());
    }

    #[test]
    fn test_extract_creates_dest_dir() {
        let tmp = TempDir::new().unwrap();
        let archive_path = tmp.path().join("pkg.tar.gz");
        let dest = tmp.path().join("deeply/nested/output");

        let tarball = make_tarball(&[("pkg-1.0/file.txt", b"data")]);
        fs::write(&archive_path, &tarball).unwrap();

        extract_tarball(&archive_path, &dest).unwrap();

        assert!(dest.exists(), "dest dir should be created");
        assert!(dest.join("file.txt").exists());
    }

    #[test]
    fn test_extract_nonexistent_archive() {
        let tmp = TempDir::new().unwrap();
        let result = extract_tarball(&tmp.path().join("nope.tar.gz"), tmp.path());
        assert!(result.is_err(), "should fail for missing archive");
    }
}
