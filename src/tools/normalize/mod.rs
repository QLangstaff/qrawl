//! Normalize Tools

mod tests;
pub mod utils;

pub use utils::{normalize_social, normalize_url};

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
/// - Normalize escaped newlines
/// - Strip junk elements (comments, scripts, styles, etc.)
/// - Normalize whitespace
pub async fn normalize_html(html: &Html) -> Html {
    let html = html.to_string();
    let normalized = tokio::task::spawn_blocking(move || {
        let mut result = html;
        result = utils::normalize_escaped_newlines(&result);
        result = utils::strip_junk(&result);
        result = utils::normalize_whitespace(&result);
        result
    })
    .await
    .expect("normalize_html: spawn_blocking failed");
    Html::new(normalized)
}

/// Normalize email addresses
///
/// - Trim whitespace
/// - Strip surrounding punctuation
/// - Extract email from display name (e.g. "Name <email@example.com>")
/// - URL decode
/// - Lowercase
/// - Deduplicate
pub fn normalize_emails(emails: &[String]) -> Vec<String> {
    crate::dedupe!(emails, utils::normalize_email)
}

/// Normalize phone numbers
///
/// - Trim whitespace
/// - Strip extensions (e.g. "ext. 123", "x123", "#123")
/// - Keep leading `+` for international numbers
/// - Remove non-digit characters
/// - Deduplicate
pub fn normalize_phones(phones: &[String]) -> Vec<String> {
    crate::dedupe!(phones, utils::normalize_phone)
}

/// Normalize URLs
///
/// - Add https:// if protocol is missing
/// - Normalize protocol to https
/// - Normalize domain (lowercase, IDNA, strip www)
/// - Normalize path (strip all trailing slashes)
/// - Sort query parameters
/// - Remove fragment
/// - Deduplicate
pub fn normalize_urls(urls: &[String]) -> Vec<String> {
    crate::dedupe!(urls, utils::normalize_url)
}

/// Normalize social URLs
///
/// - Deduplicate
///
/// Unlike [`normalize_emails`], this does **not** lowercase — social URLs carry
/// case-sensitive handles and ids (e.g. a YouTube id like `dQw4w9WgXcQ`); the
/// host is already lowercased in the canonical form.
pub fn normalize_social_urls(socials: &[String]) -> Vec<String> {
    crate::dedupe!(socials.iter().cloned())
}
