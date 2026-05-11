use serde::{Deserialize, Serialize};

pub use super::profile::FetchProfile;

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
