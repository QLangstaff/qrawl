use serde::{Deserialize, Serialize};

pub use super::profile::FetchProfile;

/// Batteries included presets for fetching HTML.
///
/// Most callers only need to choose between raw speed and reliability. The
/// presets hide profile juggling while [`FetchResult::profile_used`] still tells
/// you what eventually worked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FetchStrategy {
    /// Fastest option: one attempt using the Minimal profile.
    Fast,

    /// Reliable option: Minimal → Windows → IOS with brief delays in between.
    Adaptive,
}

impl FetchStrategy {
    /// Convenience constructor for [`FetchStrategy::Fast`].
    pub fn fast() -> Self {
        Self::Fast
    }

    /// Convenience constructor for [`FetchStrategy::Adaptive`].
    pub fn adaptive() -> Self {
        Self::Adaptive
    }
}

impl Default for FetchStrategy {
    fn default() -> Self {
        Self::Adaptive
    }
}

/// Result of a fetch operation including telemetry metadata.
///
/// Contains the fetched HTML and metadata about the fetch operation:
/// - Which profile succeeded
/// - How long the operation took
/// - How many attempts before success
///
/// # Examples
/// ```no_run
/// use qrawl::tools::fetch::{fetch, FetchResult};
///
/// # async fn example() -> Result<(), String> {
/// let result = fetch("https://example.com").await?;
/// println!("Fetched {} bytes with {:?} in {}ms ({} attempts)",
///     result.html.len(),
///     result.profile_used,
///     result.duration_ms,
///     result.attempts
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    /// The fetched HTML content
    pub html: String,
    /// The profile that succeeded
    pub profile_used: FetchProfile,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Number of attempts before success
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
