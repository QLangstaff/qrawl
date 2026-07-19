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

impl serde::Serialize for QrawlError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize as the message string, so the CLI's JSON keeps the
        // `{"Err": "…"}` shape.
        serializer.serialize_str(&self.message)
    }
}
