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
    /// A domain list entry isn't a domain name; the entry as typed.
    #[error("Invalid domain `{0}`")]
    InvalidDomain(String),
    /// The server answered with a non-2xx status.
    #[error("HTTP status {0}")]
    Status(u16),
    /// The body isn't a page: too short, or no HTML markers.
    #[error("invalid content - {0}")]
    NotHtml(&'static str),
    /// The body is larger than [`MAX_RESPONSE_BYTES`](crate::tools::fetch::MAX_RESPONSE_BYTES).
    #[error("response larger than {0} bytes")]
    TooLarge(usize),
    /// The URL redirects more times than a browser would follow.
    #[error("too many redirects: {0}")]
    TooManyRedirects(String),
    /// The body is a bot challenge; the marker found.
    #[error("suspicious content - {0}")]
    BotChallenge(&'static str),
    /// A redirect led to a consent interstitial, not the page; the address
    /// that served it.
    #[error("consent interstitial: {0}")]
    ConsentInterstitial(String),
    /// The request never completed: connection, TLS, timeout, or a body that
    /// couldn't be read.
    #[error("HTTP request failed: {0}")]
    Request(String),
    /// Every profile of a multi-profile strategy failed; one error per
    /// attempt, in order.
    #[error("all {} profiles failed: [{}]", .0.len(), .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    AllProfilesFailed(Vec<QrawlError>),
}

impl QrawlError {
    /// Whether another fetch profile could not change the outcome. A 3xx
    /// status is one the client couldn't follow (an invalid `Location`), which
    /// no profile can. A redirect loop or a consent interstitial may be a
    /// cookie the client didn't keep, so it's final only after a profile that
    /// keeps cookies met it.
    pub(crate) fn is_final(&self) -> bool {
        matches!(
            self,
            Self::BlockedDomain(_) | Self::Status(300..=399 | 404 | 410) | Self::TooLarge(_)
        )
    }
}
