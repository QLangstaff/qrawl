mod client;
mod headers;
mod strategies;
mod utils;

pub mod types;

// Re-export types for public use
pub use types::*;

/// Fetch HTML from URL using adaptive strategy (tries all strategies until one succeeds).
///
/// Automatically tries multiple strategies in order until one succeeds:
/// Minimal → Browser → Mobile → Stealth → Extreme
///
/// Returns `FetchResult` with HTML and metadata (strategy used, duration, attempts).
///
/// This is the most reliable option but can be slow (~1-6s per URL).
/// Use `fetch_with_strategy()` if you need more control over speed vs reliability.
///
/// # Examples
/// ```no_run
/// use qrawl::tools::fetch::fetch;
///
/// # async fn example() -> Result<(), String> {
/// // Get result with metadata
/// let result = fetch("https://example.com").await?;
/// println!("Used {:?} in {}ms", result.strategy_used, result.duration_ms);
/// let html = result.html;
///
/// // Or just get the HTML
/// let html = fetch("https://example.com").await?.html;
/// # Ok(())
/// # }
/// ```
pub async fn fetch(url: &str) -> Result<FetchResult, String> {
    strategies::fetch_with_adaptive_strategy(url).await
}

/// Fetch HTML from URL with a specific strategy.
///
/// Returns `FetchResult` with HTML and metadata (strategy used, duration, attempts).
///
/// Choose a strategy to balance speed vs reliability:
/// - `FetchStrategy::Minimal`: Fastest, single attempt (~100ms)
/// - `FetchStrategy::Browser`: Medium, 2 attempts with browser headers (~500ms)
/// - `FetchStrategy::Mobile`: Medium, 2 attempts with mobile headers (~500ms)
/// - `FetchStrategy::Stealth`: Slow, 2 attempts with full bot evasion (~1-2s)
/// - `FetchStrategy::Extreme`: Very slow, session building + external referrers (~2-5s)
/// - `FetchStrategy::Adaptive`: Slowest, tries all strategies (~1-6s, same as `fetch()`)
///
/// # Examples
/// ```no_run
/// use qrawl::tools::fetch::{fetch_with_strategy, FetchStrategy};
///
/// # async fn example() -> Result<(), String> {
/// // Fast: single attempt for bulk searches
/// let result = fetch_with_strategy("https://example.com", FetchStrategy::Minimal).await?;
/// println!("Minimal strategy took {}ms", result.duration_ms);
///
/// // Maximum evasion: for sites with PerimeterX/DataDome
/// let html = fetch_with_strategy("https://thekitchn.com/recipe", FetchStrategy::Extreme).await?.html;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_with_strategy(
    url: &str,
    strategy: FetchStrategy,
) -> Result<FetchResult, String> {
    if strategy == FetchStrategy::Adaptive {
        strategies::fetch_with_adaptive_strategy(url).await
    } else {
        strategies::fetch_with_single_strategy(url, strategy).await
    }
}

/// Fetch HTML from URL (convenience function that returns only the HTML).
///
/// This is a convenience wrapper around `fetch()` that discards metadata.
/// Use this when you don't need telemetry data.
///
/// # Examples
/// ```no_run
/// use qrawl::tools::fetch::fetch_html;
///
/// # async fn example() -> Result<(), String> {
/// let html = fetch_html("https://example.com").await?;
/// # Ok(())
/// # }
/// ```
pub async fn fetch_html(url: &str) -> Result<String, String> {
    fetch(url).await.map(|r| r.html)
}

// TODO: add partial fetch function (ranged sniff 64-128 kb / accept encoding gzip br - partial GET before full fetch)
// Use in classify function to speed up classification of jsonld schemas
