//! Shared Errors

/// The crate-wide error: a single detail message describing what failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct QrawlError {
    message: String,
}

impl QrawlError {
    /// Build an error from a detail message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The detail message.
    pub fn message(&self) -> &str {
        &self.message
    }
}
