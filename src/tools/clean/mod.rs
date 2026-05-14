//! Clean Tools

mod tests;
pub mod utils;

use serde::{Deserialize, Deserializer, Serialize};

pub use utils::canonicalize_url;

/// A URL that is guaranteed to have been run through `canonicalize_url`.
///
/// Construct via `CanonicalUrl::new` (or `From<&str>` / `From<String>`); the
/// inner field is private so the invariant cannot be bypassed.
///
/// Serialization is transparent (a bare string). Deserialization is **strict**:
/// it re-runs `canonicalize_url` on incoming values so non-canonical strings
/// from databases / API payloads / hand-written JSON cannot sneak past the
/// invariant. (Re-canonicalizing canonical input is a no-op — `canonicalize_url`
/// is idempotent.)
///
/// **Note:** qrawl's own pipeline (`chain!`, `FETCH_CACHE`, `fetch_*`) currently
/// uses raw `String` and canonicalizes at the cache boundary instead — threading
/// this type through the generic chain macro would force conversions wherever
/// new URLs are produced (e.g. `map_children` parses fresh URLs out of HTML).
/// This type is exposed for downstream callers that want the invariant enforced
/// by the type system at their own boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalUrl(String);

impl CanonicalUrl {
    /// Canonicalize and wrap. Idempotent if the input is already canonical.
    pub fn new(raw: &str) -> Self {
        Self(canonicalize_url(raw))
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
        let raw = String::deserialize(deserializer)?;
        Ok(Self::new(&raw))
    }
}

/// Clean text
///
/// - Decode HTML entities
/// - Normalize unicode (NFC)
/// - Remove zero-width characters
/// - Remove control characters
/// - Normalize whitespace
pub fn clean_text(text: &str) -> String {
    let mut result = utils::decode_html_entities(text);
    result = utils::normalize_unicode(&result);
    result = utils::remove_zero_width_chars(&result);
    result = utils::remove_control_chars(&result);
    result = utils::normalize_whitespace(&result);
    result
}

/// Clean HTML
///
/// - Normalize escaped newlines
/// - Strip junk elements (comments, scripts, styles, etc.)
/// - Normalize whitespace
pub async fn clean_html(html: &str) -> String {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        let mut result = html;
        result = utils::normalize_escaped_newlines(&result);
        result = utils::strip_junk(&result);
        result = utils::normalize_whitespace(&result);
        result
    })
    .await
    .expect("clean_html: spawn_blocking failed")
}

/// Clean URLs
///
/// - Add https:// if protocol is missing
/// - Normalize protocol to https
/// - Canonicalize domain (lowercase, IDNA, strip www)
/// - Normalize path (strip all trailing slashes)
/// - Sort query parameters
/// - Remove fragment
/// - Deduplicate
pub fn clean_urls(urls: &[String]) -> Vec<String> {
    crate::dedupe!(urls, utils::canonicalize_url)
}

/// Clean email addresses
///
/// - Trim whitespace
/// - Strip surrounding punctuation
/// - Extract email from display name (e.g. "Name <email@example.com>")
/// - URL decode
/// - Lowercase
/// - Deduplicate
pub fn clean_emails(emails: &[String]) -> Vec<String> {
    crate::dedupe!(emails, utils::clean_email)
}

/// Clean phone numbers
///
/// - Trim whitespace
/// - Strip extensions (e.g. "ext. 123", "x123", "#123")
/// - Keep leading `+` for international numbers
/// - Remove non-digit characters
/// - Deduplicate
pub fn clean_phones(phones: &[String]) -> Vec<String> {
    crate::dedupe!(phones, utils::clean_phone)
}
