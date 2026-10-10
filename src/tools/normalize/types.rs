use serde::{Deserialize, Deserializer, Serialize};

use super::normalize_url;

/// A URL in its canonical form ([`normalize_url`]): the key two URLs of the
/// same page share. It identifies a page; it isn't the address to fetch, since
/// a server can answer `http://`, `www.`, and a trailing slash differently.
///
/// Construct via [`CanonicalUrl::new`] (or `From<&str>` / `From<String>`); the
/// inner string is private so the invariant can't be bypassed. Serialization is
/// transparent (a bare string); **deserialization re-canonicalizes**, so a
/// non-canonical string from a DB / API payload / hand-written JSON can't sneak
/// past the invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalUrl(String);

impl CanonicalUrl {
    /// Canonicalize and wrap. Idempotent if the input is already canonical.
    pub fn new(raw: &str) -> Self {
        Self(normalize_url(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for CanonicalUrl {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CanonicalUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CanonicalUrl {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CanonicalUrl {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl<'de> Deserialize<'de> for CanonicalUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(&String::deserialize(deserializer)?))
    }
}
