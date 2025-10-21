use once_cell::sync::Lazy;
use regex::Regex;
use scraper::Html;

use crate::selectors::LINK_SELECTOR;

// Lazy static regex patterns
static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").expect("valid regex")
});
static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").expect("valid regex")
});

/// Extract values from links with a specific href prefix (e.g., "mailto:", "tel:")
fn extract_with_prefix(doc: &Html, prefix: &str) -> Vec<String> {
    let mut results = Vec::new();
    for link in doc.select(&LINK_SELECTOR) {
        if let Some(href) = link.value().attr("href") {
            if let Some(value) = href.strip_prefix(prefix) {
                let clean = value.split('?').next().unwrap_or(value);
                if !clean.is_empty() {
                    results.push(clean.to_string());
                }
            }
        }
    }
    results
}

/// Extract values from text content using a regex pattern
fn extract_with_regex(doc: &Html, regex: &Regex) -> Vec<String> {
    let text = doc.root_element().text().collect::<String>();
    regex
        .captures_iter(&text)
        .filter_map(|cap| cap.get(0))
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Extract all email addresses from HTML document.
pub(super) fn extract_email_elements(html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(html);
    crate::merge!(
        extract_with_prefix(&doc, "mailto:"),
        extract_with_regex(&doc, &EMAIL_REGEX)
    )
}

/// Extract all phone numbers from HTML document.
pub(super) fn extract_phone_elements(html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(html);
    crate::merge!(
        extract_with_prefix(&doc, "tel:"),
        extract_with_regex(&doc, &PHONE_REGEX)
    )
}

/// Find the first non-empty value for any of the given keys in metadata pairs.
pub(super) fn find_metadata_value(pairs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        for (k, v) in pairs {
            if k.eq_ignore_ascii_case(key) {
                let cleaned = v.trim().to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

pub(super) fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|existing| existing == &value) {
        items.push(value);
    }
}
