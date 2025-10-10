use serde::{Deserialize, Serialize};

/// Strategy for fetching content with different levels of bot evasion.
///
/// Use different strategies to balance speed vs reliability:
/// - `Minimal`: Fastest, single attempt with minimal headers (~100ms per URL)
/// - `Browser`: Medium speed, browser headers with retry (~500ms per URL)
/// - `Mobile`: Medium speed, mobile headers with retry (~500ms per URL)
/// - `Stealth`: Slowest, full bot evasion with retry (~1-2s per URL)
/// - `Extreme`: Maximum evasion with session building (~2-5s per URL)
/// - `Adaptive`: Tries all strategies until one succeeds (~1-6s per URL)
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStrategy {
    Minimal,
    Browser,
    Mobile,
    Stealth,
    Extreme,
    Adaptive,
}

/// Result of a fetch operation including telemetry metadata.
///
/// Contains the fetched HTML and metadata about the fetch operation:
/// - Which strategy succeeded
/// - How long the operation took
/// - How many strategies were attempted before success
///
/// # Examples
/// ```no_run
/// use qrawl::tools::fetch::{fetch, FetchResult};
///
/// # async fn example() -> Result<(), String> {
/// let result = fetch("https://example.com").await?;
/// println!("Fetched {} bytes with {:?} in {}ms (tried {} strategies)",
///     result.html.len(),
///     result.strategy_used,
///     result.duration_ms,
///     result.attempts
/// );
/// let html = result.html;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    /// The fetched HTML content
    pub html: String,
    /// The strategy that succeeded
    pub strategy_used: FetchStrategy,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Number of strategies attempted before success
    pub attempts: usize,
}

impl FetchResult {
    /// Consume the result and return just the HTML.
    ///
    /// Convenience method for when you don't need the metadata.
    pub fn into_html(self) -> String {
        self.html
    }
}
