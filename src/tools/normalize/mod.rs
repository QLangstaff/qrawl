//! Normalize Tools

mod tests;
mod types;
mod utils;

pub use types::CanonicalUrl;
pub use utils::normalize_url;

use crate::types::Html;

/// Normalize text
///
/// - Decode HTML entities
/// - Normalize unicode (NFC)
/// - Remove zero-width characters
/// - Remove control characters
/// - Normalize whitespace
pub fn normalize_text(text: &str) -> String {
    let mut result = utils::decode_html_entities(text);
    result = utils::normalize_unicode(&result);
    result = utils::remove_zero_width_chars(&result);
    result = utils::remove_control_chars(&result);
    result = utils::normalize_whitespace(&result);
    result
}

/// Normalize HTML
///
/// Parses the markup and writes back its content:
/// - without comments, scripts, styles, `<noscript>`, `<iframe>`, or `<svg>`
/// - without styling, scripting, and accessibility attributes
/// - with each run of whitespace in text as one space
/// - with each JSON-LD script in place, its text untouched
///
/// The output is re-serialized, so quoting and entities are normalized too; a
/// fragment stays a fragment, and a document keeps its doctype.
pub async fn normalize_html(html: &Html) -> Html {
    let html = html.to_string();
    let normalized = tokio::task::spawn_blocking(move || utils::normalize_markup(&html))
        .await
        .expect("normalize_html: spawn_blocking failed");
    Html::new(normalized)
}

/// Normalize email addresses
///
/// - Trim whitespace
/// - Strip trailing punctuation
/// - Extract email from display name (e.g. "Name <email@example.com>")
/// - URL decode
/// - Lowercase
/// - Deduplicate
pub fn normalize_emails(emails: &[String]) -> Vec<String> {
    utils::normalize_each(emails, utils::normalize_email)
}

/// Normalize phone numbers
///
/// - Trim whitespace
/// - Strip a space-separated extension (e.g. " ext. 123", " x123")
/// - Keep leading `+` for international numbers
/// - Remove non-digit characters
/// - Deduplicate
pub fn normalize_phones(phones: &[String]) -> Vec<String> {
    utils::normalize_each(phones, utils::normalize_phone)
}

/// Normalize URLs
///
/// - Add https:// if protocol is missing
/// - Normalize protocol to https
/// - Normalize domain (lowercase, IDNA, strip www)
/// - Normalize path (strip all trailing slashes)
/// - Strip tracking parameters, sort the rest
/// - Remove fragment
/// - On a social platform, strip the mobile host prefix and share tokens
/// - Deduplicate
pub fn normalize_urls(urls: &[String]) -> Vec<String> {
    utils::normalize_each(urls, utils::normalize_url)
}

/// Deduplicate URLs by their canonical form ([`normalize_url`]), keeping each
/// as written — unlike [`normalize_urls`], whose output is a key, not always
/// an address a server answers.
pub fn dedupe_urls(urls: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    urls.iter()
        .filter(|url| !url.trim().is_empty() && seen.insert(normalize_url(url)))
        .cloned()
        .collect()
}

/// The `http(s)` URL an input names, ready to fetch — with `https://` when it
/// names no scheme (`example.com/a`) — or `None` when it isn't one (`mailto:`,
/// `ftp:`, a relative path, an email address). Unlike [`normalize_url`], it
/// keeps the URL as written.
pub fn absolute_url(input: &str) -> Option<String> {
    crate::utils::absolute_url(input)
}
