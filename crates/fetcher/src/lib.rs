//! Download engine
//!
//! Downloads files over HTTP with SHA-256 checksum verification.
//! Falls back to mirror URLs when the primary source fails.
//! Supports concurrent downloads via `download_many`.

use anyhow::Result;
use reqwest::Client;
use sha2::{Sha256, Digest};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

pub struct Fetcher {
    client: Client,
    max_concurrent: usize,
}

impl Fetcher {
    /// Create a fetcher with default concurrency (50).
    pub fn new() -> Result<Self> {
        Self::with_concurrency(50)
    }

    /// Create a fetcher with a custom concurrency limit.
    pub fn with_concurrency(max_concurrent: usize) -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("brew-rs/", env!("CARGO_PKG_VERSION")))
            .no_proxy()
            .build()?;

        Ok(Self {
            client,
            max_concurrent,
        })
    }

    /// Download a file and optionally verify its SHA-256 checksum.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<()> {
        info!("Downloading: {} -> {}", url, dest.display());

        // Download file
        let response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {} for {}", status, url);
        }
        let bytes = response.bytes().await?;

        // Verify checksum if provided
        if let Some(expected) = expected_sha256 {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let result = format!("{:x}", hasher.finalize());

            if result != expected {
                anyhow::bail!(
                    "Checksum mismatch! Expected: {}, Got: {}",
                    expected,
                    result
                );
            }
            info!("✓ Checksum verified");
        }

        // Write to file
        let mut file = File::create(dest).await?;
        file.write_all(&bytes).await?;

        info!("✓ Downloaded: {}", dest.display());
        Ok(())
    }

    /// Try the primary URL first, then each mirror in order until one works.
    pub async fn download_with_mirrors(
        &self,
        url: &str,
        mirrors: &[String],
        dest: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<()> {
        match self.download(url, dest, expected_sha256).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                if mirrors.is_empty() {
                    return Err(e);
                }
                warn!("Primary URL failed ({}), trying mirrors...", e);
            }
        }

        for (i, mirror) in mirrors.iter().enumerate() {
            match self.download(mirror, dest, expected_sha256).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!("Mirror {} failed: {}", i + 1, e);
                }
            }
        }

        anyhow::bail!(
            "All download sources failed for {} ({} mirrors tried)",
            url,
            mirrors.len()
        )
    }

    /// Download multiple files concurrently, up to `max_concurrent` at once.
    pub async fn download_many(
        &self,
        downloads: Vec<(&str, &Path, Option<&str>)>,
    ) -> Result<()> {
        use futures::stream::{self, StreamExt};

        let results: Vec<Result<()>> = stream::iter(downloads)
            .map(|(url, dest, checksum)| async move {
                self.download(url, dest, checksum).await
            })
            .buffer_unordered(self.max_concurrent)
            .collect()
            .await;

        // Check if any downloads failed
        for result in results {
            result?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_creation() {
        let fetcher = Fetcher::new().unwrap();
        assert_eq!(fetcher.max_concurrent, 50);
    }

    #[test]
    fn test_fetcher_custom_concurrency() {
        let fetcher = Fetcher::with_concurrency(10).unwrap();
        assert_eq!(fetcher.max_concurrent, 10);
    }
}
