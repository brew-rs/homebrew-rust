//! Download engine
//!
//! Downloads files over HTTP with SHA-256 checksum verification.
//! Falls back to mirror URLs when the primary source fails.
//! Supports concurrent downloads via `download_many`.
//! Shows a progress bar with percentage, speed, and ETA via indicatif.

use anyhow::Result;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
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
    ///
    /// Shows a progress bar on stderr. If the server doesn't send
    /// Content-Length, falls back to a spinner with a byte counter.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<()> {
        info!("Downloading: {} -> {}", url, dest.display());

        let response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {} for {}", status, url);
        }

        // Set up progress bar based on whether we know the total size.
        let content_length = response.content_length();
        let pb = match content_length {
            Some(total) => {
                let pb = ProgressBar::new(total);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                        .unwrap()
                        .progress_chars("=>-"),
                );
                pb
            }
            None => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner} {msg} {bytes} ({bytes_per_sec})")
                        .unwrap(),
                );
                pb
            }
        };

        // Use the filename from the URL as the progress bar message.
        let filename = url
            .rsplit('/')
            .next()
            .unwrap_or(url)
            .to_string();
        pb.set_message(filename);

        // Stream chunks, update progress bar, hash incrementally.
        let mut hasher = Sha256::new();
        let mut file = File::create(dest).await?;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            pb.inc(chunk.len() as u64);
        }

        pb.finish_and_clear();

        // Verify checksum after all bytes received.
        if let Some(expected) = expected_sha256 {
            let result = format!("{:x}", hasher.finalize());
            if result != expected {
                anyhow::bail!(
                    "Checksum mismatch for {}! Expected: {}, Got: {}",
                    url,
                    expected,
                    result
                );
            }
            info!("✓ Checksum verified");
        }

        file.flush().await?;
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

        for result in results {
            result?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

    fn sha256_hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        format!("{:x}", h.finalize())
    }

    /// Verify that streaming chunks produce the same SHA-256 as hashing the
    /// full buffer — this is the invariant the download loop depends on.
    #[test]
    fn test_sha256_incremental_matches_single_shot() {
        let data = b"hello brew-rs incremental hashing test data";
        let expected = sha256_hex(data);

        let mut hasher = Sha256::new();
        for chunk in data.chunks(7) {
            hasher.update(chunk);
        }
        let incremental = format!("{:x}", hasher.finalize());

        assert_eq!(expected, incremental, "incremental hash must match single-shot hash");
        assert_eq!(expected.len(), 64, "SHA-256 hex must be 64 chars");
    }

    #[tokio::test]
    async fn test_download_correct_checksum_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let body = b"hello brew-rs streaming test";
        let expected_hash = sha256_hex(body);

        let _m = server
            .mock("GET", "/test.tar.gz")
            .with_status(200)
            .with_header("content-length", &body.len().to_string())
            .with_body(body.as_ref())
            .create_async()
            .await;

        let fetcher = Fetcher::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("out.tar.gz");
        let url = format!("{}/test.tar.gz", server.url());

        fetcher
            .download(&url, &dest, Some(&expected_hash))
            .await
            .expect("download with correct checksum should succeed");

        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    #[tokio::test]
    async fn test_download_checksum_mismatch_returns_error() {
        let mut server = mockito::Server::new_async().await;
        let body = b"some downloadable content";

        let _m = server
            .mock("GET", "/bad.tar.gz")
            .with_status(200)
            .with_header("content-length", &body.len().to_string())
            .with_body(body.as_ref())
            .create_async()
            .await;

        let fetcher = Fetcher::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("bad.tar.gz");
        let url = format!("{}/bad.tar.gz", server.url());

        let result = fetcher
            .download(
                &url,
                &dest,
                Some("0000000000000000000000000000000000000000000000000000000000000000"),
            )
            .await;

        assert!(result.is_err(), "download with wrong checksum must fail");
        assert!(
            result.unwrap_err().to_string().contains("Checksum mismatch"),
            "error must mention 'Checksum mismatch'"
        );
    }

    #[tokio::test]
    async fn test_download_without_checksum_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let body = b"no checksum needed for this test";

        let _m = server
            .mock("GET", "/nocheck.tar.gz")
            .with_status(200)
            .with_body(body.as_ref())
            .create_async()
            .await;

        let fetcher = Fetcher::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("nocheck.tar.gz");
        let url = format!("{}/nocheck.tar.gz", server.url());

        fetcher
            .download(&url, &dest, None)
            .await
            .expect("download without checksum should succeed");

        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    #[tokio::test]
    async fn test_download_http_error_returns_err() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("GET", "/notfound.tar.gz")
            .with_status(404)
            .create_async()
            .await;

        let fetcher = Fetcher::new().unwrap();
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("notfound.tar.gz");
        let url = format!("{}/notfound.tar.gz", server.url());

        let result = fetcher.download(&url, &dest, None).await;
        assert!(result.is_err(), "HTTP 404 must return an error");
        assert!(
            result.unwrap_err().to_string().contains("404"),
            "error must mention the HTTP status code"
        );
    }
}
