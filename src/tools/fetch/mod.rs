mod client;
mod headers;
mod retry;
mod strategies;

/// Fetch HTML from URL.
///
/// Automatically tries multiple strategies until one pass or all fail:
/// Minimal → Browser → Mobile → Stealth
pub async fn fetch(url: &str) -> Result<String, String> {
    strategies::adaptive_fetch(url).await
}
