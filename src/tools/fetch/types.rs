use serde::{Deserialize, Serialize};

pub use super::profile::FetchProfile;

/// Fetch Strategy
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

/// Fetch Result
///
/// Contains the fetched HTML and metadata about the fetch operation:
/// - Which profile succeeded
/// - How long the operation took
/// - How many attempts before success
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
