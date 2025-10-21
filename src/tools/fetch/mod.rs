//! Fetch Tools

mod client;
mod headers;
pub mod profile;
mod strategies;
mod utils;

pub mod cli;
pub mod tests;
pub mod types;

pub use profile::FetchProfile;
pub use types::*;

/// Fetch with fast strategy (single attempt with minimal profile)
pub async fn fetch_fast(url: &str) -> Result<String, String> {
    strategies::fetch_fast_with_client(url)
        .await
        .map(|r| r.html)
}

/// Fetch with auto strategy (multiple attempts with different profiles)
pub async fn fetch_auto(url: &str) -> Result<String, String> {
    strategies::fetch_auto_with_client(url)
        .await
        .map(|r| r.html)
}
