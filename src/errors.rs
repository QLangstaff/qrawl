//! Shared Errors

/// What went wrong, so a caller can tell a bad input from a blocked host from
/// a page that didn't answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QrawlError {
    /// The input isn't an HTTP URL.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    /// The `Context`'s domain lists reject this URL or a redirect from it.
    #[error("blocked by domain filter: {0}")]
    BlockedDomain(String),
    /// The server answered with a non-2xx status.
    #[error("HTTP status {0}")]
    Status(u16),
    /// The body isn't a page: too short, or no HTML markers.
    #[error("invalid content - {0}")]
    NotHtml(&'static str),
    /// The body is a bot challenge; the marker found.
    #[error("suspicious content - {0}")]
    BotChallenge(&'static str),
    /// The request never completed: connection, TLS, timeout, or a body that
    /// couldn't be read.
    #[error("HTTP request failed: {0}")]
    Request(String),
    /// Every fetch profile failed; one error per attempt, in order.
    #[error("all {} profiles failed: [{}]", .0.len(), .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    AllProfilesFailed(Vec<QrawlError>),
}

impl QrawlError {
    /// Whether another fetch profile could not change the outcome.
    pub(crate) fn is_final(&self) -> bool {
        matches!(
            self,
            Self::InvalidUrl(_) | Self::BlockedDomain(_) | Self::Status(404 | 410)
        )
    }
}
