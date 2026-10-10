use crate::types::Html;

/// A fetched page: its HTML and the URL that served it, which differs from
/// the URL requested when the server redirected. Relative links in `html`
/// resolve against `url`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub url: String,
    pub html: Html,
}

/// A browser a request presents itself as: its User-Agent and matching headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FetchProfile {
    /// Just a User-Agent.
    Minimal,
    /// Chrome on Windows.
    Windows,
    /// Safari on iPhone.
    Ios,
}
