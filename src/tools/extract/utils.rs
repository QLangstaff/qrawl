use regex::Regex;
use scraper::Html;
use std::sync::LazyLock;

use crate::utils::{LINK_SELECTOR, TLD_LEN};

// Lazy static regex patterns
static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // More strict email pattern that excludes common false positives:
    // - Word boundaries (\b) ensure we match complete emails, not parts of concatenated text
    // - Local part: alphanumeric, dots, underscores, percent, plus, hyphens
    // - Domain: alphanumeric segments separated by dots or hyphens
    // - TLD: letters only, `TLD_LEN` of them
    Regex::new(&format!(
        r"(?i)\b[a-z0-9._%+-]+@[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.[a-z]{{{},{}}}\b",
        TLD_LEN.start(),
        TLD_LEN.end()
    ))
    .expect("valid regex")
});
static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").expect("valid regex")
});

/// Extract values from links with a specific href prefix (e.g., "mailto:", "tel:")
fn extract_with_prefix(doc: &Html, prefix: &str) -> Vec<String> {
    let mut results = Vec::new();
    for link in doc.select(&LINK_SELECTOR) {
        if let Some(href) = link.value().attr("href") {
            // A scheme is case-insensitive (`MAILTO:`).
            let scheme = href.get(..prefix.len()).unwrap_or_default();
            if scheme.eq_ignore_ascii_case(prefix) {
                // Trim whitespace and remove query parameters
                let value = &href[prefix.len()..];
                let normalized = value.split('?').next().unwrap_or(value).trim();
                if !normalized.is_empty() {
                    results.push(normalized.to_string());
                }
            }
        }
    }
    results
}

/// Extract values from text content using a regex pattern
fn extract_with_regex(doc: &Html, regex: &Regex) -> Vec<String> {
    // Join text nodes with spaces to prevent adjacent elements from creating false patterns
    // e.g., <div>9631</div><a>contact@domain.com</a> becomes "9631 contact@domain.com"
    // instead of "9631contact@domain.com"
    // Script and style text is code, not content.
    let text = doc
        .root_element()
        .descendants()
        .filter_map(|node| {
            let text = node.value().as_text()?;
            let parent = node.parent()?.value().as_element()?.name();
            (!matches!(parent, "script" | "style")).then_some(&**text)
        })
        .collect::<Vec<_>>()
        .join(" ");
    regex
        .captures_iter(&text)
        .filter_map(|cap| cap.get(0))
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Extract all email addresses from HTML document.
pub(super) fn extract_email_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "mailto:"),
        extract_with_regex(doc, &EMAIL_REGEX),
    ]
    .concat()
}

/// Extract all phone numbers from HTML document.
pub(super) fn extract_phone_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "tel:"),
        extract_with_regex(doc, &PHONE_REGEX),
    ]
    .concat()
}

/// Find the first non-empty value for any of the given keys in metadata pairs.
pub(super) fn find_metadata_value(pairs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        for (k, v) in pairs {
            if k.eq_ignore_ascii_case(key) {
                let normalized = v.trim().to_string();
                if !normalized.is_empty() {
                    return Some(normalized);
                }
            }
        }
    }
    None
}
