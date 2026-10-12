use crate::types::Html;

/// A fetched page: its HTML and the URL that served it, which differs from
/// the URL requested when the server redirected. Relative links in `html`
/// resolve against `address`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub address: String,
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

impl FetchProfile {
    /// Whether the profile's client keeps cookies, as a browser does: the one
    /// way out of a redirect that sets a cookie and waits for it back.
    pub(crate) fn keeps_cookies(self) -> bool {
        self != Self::Minimal
    }
}
