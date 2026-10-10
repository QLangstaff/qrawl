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
/// One phone number as written, in any country's format: 5 to 15 digits with
/// the punctuation that groups them, then an extension (` ext. 12`, ` x12`)
/// or RFC 3966 parameters (`;ext=12`) if any.
static ONE_PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\+?(?:[\s().\-/]*\d){5,15}[\s().\-/]*(?:;.*|(?:ext\.?|x)\s*\d+)?$")
        .expect("valid regex")
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

/// What `read` finds in the values a page's JSON-LD holds under the
/// schema.org property `key` (`email`, `telephone`), at any depth: in a
/// string, a number, an array, a typed literal's `@value`, an IRI's `@id`, or
/// the items of a `@list` or `@set`. JSON-LD is data, so the property says
/// what a value is: read as raw text, an SKU would pass for a phone number.
fn extract_from_jsonld(doc: &Html, key: &str, read: impl Fn(&str) -> Vec<String>) -> Vec<String> {
    let is_key = |name: &str| {
        name.rsplit([':', '/', '#'])
            .next()
            .is_some_and(|short| short.eq_ignore_ascii_case(key))
    };
    let mut found = Vec::new();
    for script in doc.select(&JSONLD_SELECTOR) {
        let text: String = script.text().collect();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        // `(value, under_key)`: whether the value sits under the property.
        let mut pending = vec![(&value, false)];
        while let Some((value, under_key)) = pending.pop() {
            match value {
                serde_json::Value::String(text) if under_key => found.extend(read(text)),
                serde_json::Value::Number(number) if under_key => {
                    found.extend(read(&number.to_string()));
                }
                serde_json::Value::Array(items) => {
                    pending.extend(items.iter().map(|item| (item, under_key)));
                }
                serde_json::Value::Object(fields) => {
                    pending.extend(fields.iter().map(|(name, value)| {
                        let is_value_of_key = under_key
                            && matches!(name.as_str(), "@value" | "@id" | "@list" | "@set");
                        (value, is_key(name) || is_value_of_key)
                    }));
                }
                _ => {}
            }
        }
    }
    found
}

/// The phone numbers in a JSON-LD `telephone` value: the value itself, without
/// a `tel:` scheme, when it is one number as written, in any country's format.
/// Otherwise — prose, two numbers, `"call us"` — whatever `PHONE_REGEX` finds
/// in it, as in page text.
fn read_jsonld_telephone(text: &str) -> Vec<String> {
    let text = text.trim();
    let scheme = text.get(.."tel:".len()).unwrap_or_default();
    let value = if scheme.eq_ignore_ascii_case("tel:") {
        text["tel:".len()..].trim()
    } else {
        text
    };
    if ONE_PHONE_REGEX.is_match(value) {
        return vec![value.to_string()];
    }
    PHONE_REGEX
        .find_iter(value)
        .map(|found| found.as_str().to_string())
        .collect()
}

/// Extract all email addresses from HTML document.
pub(super) fn extract_email_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "mailto:"),
        extract_with_regex(doc, &EMAIL_REGEX),
        extract_from_jsonld(doc, "email", |text| {
            EMAIL_REGEX
                .find_iter(text)
                .map(|found| found.as_str().to_string())
                .collect()
        }),
    ]
    .concat()
}

/// Extract all phone numbers from HTML document.
pub(super) fn extract_phone_elements(doc: &Html) -> Vec<String> {
    [
        extract_with_prefix(doc, "tel:"),
        extract_with_regex(doc, &PHONE_REGEX),
        extract_from_jsonld(doc, "telephone", read_jsonld_telephone),
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
