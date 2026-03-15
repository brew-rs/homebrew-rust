/// Quick visual test for the download progress bar.
/// Run with: cargo run -p brew-fetcher --example test_progress
use std::path::Path;

#[tokio::main]
async fn main() {
    let url = "https://static.crates.io/crates/serde/serde-1.0.210.crate";
    let dest = Path::new("/tmp/brew-rs-progress-test.crate");

    let fetcher = brew_fetcher::Fetcher::new().expect("failed to create fetcher");

    println!("Downloading {} ...", url);
    match fetcher.download(url, dest, None).await {
        Ok(()) => println!("Done! Saved to {}", dest.display()),
        Err(e) => eprintln!("Download failed: {}", e),
    }

    let _ = std::fs::remove_file(dest);
}
