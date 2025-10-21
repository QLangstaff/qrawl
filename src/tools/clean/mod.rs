//! Clean Tools

mod tests;
mod utils;

/// Clean text
///
/// - Decode HTML entities
/// - Normalize unicode (NFC)
/// - Remove zero-width characters
/// - Remove control characters
/// - Normalize whitespace
pub async fn clean_text(text: &str) -> String {
    let text = text.to_string();
    tokio::task::spawn_blocking(move || {
        let mut result = text;
        result = utils::decode_html_entities(&result);
        result = utils::normalize_unicode(&result);
        result = utils::remove_zero_width_chars(&result);
        result = utils::remove_control_chars(&result);
        result = utils::normalize_whitespace(&result);
        result
    })
    .await
    .expect("clean_text: spawn_blocking failed")
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
pub async fn clean_urls(urls: &[String]) -> Vec<String> {
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
pub async fn clean_emails(emails: &[String]) -> Vec<String> {
    crate::dedupe!(emails, utils::clean_email)
}

/// Clean phone numbers
///
/// - Trim whitespace
/// - Strip extensions (e.g. "ext. 123", "x123", "#123")
/// - Keep leading `+` for international numbers
/// - Remove non-digit characters
/// - Deduplicate
pub async fn clean_phones(phones: &[String]) -> Vec<String> {
    crate::dedupe!(phones, utils::clean_phone)
}
