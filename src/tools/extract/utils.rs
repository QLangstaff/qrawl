use regex::Regex;
use scraper::Html;
use std::sync::LazyLock;

use crate::utils::{JSONLD_SELECTOR, LINK_SELECTOR, TLD_LEN};

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
    // Script and style text is code, not content. Text nodes are joined with
    // spaces so adjacent elements don't run together: `<div>9631</div>` beside
    // `<a>contact@domain.com</a>` must not read `9631contact@domain.com`.
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

/// The string values a page's JSON-LD holds under the schema.org property
/// `key` (`email`, `telephone`), at any depth. JSON-LD is data, so the
/// property says what a value is; its raw text would read an SKU as a phone.
fn extract_jsonld_values(doc: &Html, key: &str) -> Vec<String> {
    let mut found = Vec::new();
    for script in doc.select(&JSONLD_SELECTOR) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&script.inner_html()) else {
            continue;
        };
        let mut pending = vec![&value];
        while let Some(value) = pending.pop() {
            match value {
                serde_json::Value::Array(items) => pending.extend(items),
                serde_json::Value::Object(fields) => {
                    for (name, value) in fields {
                        match value {
                            serde_json::Value::String(text) if name == key => {
                                found.push(text.trim().to_string());
                            }
                            nested => pending.push(nested),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// Extract all email addresses from HTML document.
pub(super) fn extract_email_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "mailto:"),
        extract_with_regex(doc, &EMAIL_REGEX),
        extract_jsonld_values(doc, "email"),
    ]
    .concat()
}

/// Extract all phone numbers from HTML document.
pub(super) fn extract_phone_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "tel:"),
        extract_with_regex(doc, &PHONE_REGEX),
        extract_jsonld_values(doc, "telephone"),
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
