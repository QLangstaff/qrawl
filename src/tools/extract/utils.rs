use regex::Regex;
use scraper::Html;
use std::sync::LazyLock;

use crate::utils::{JSONLD_SELECTOR, LINK_SELECTOR, PHONE_EXTENSION, TLD_LEN};

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
/// A North American number in text, in ASCII digits.
static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u)(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").expect("valid regex")
});
/// One phone number as written, in any country's format: 5 to 15 digits, up to
/// three grouping characters between any two (`555 - 1212`), then an extension
/// or RFC 3966 parameters (`;ext=12`) if any.
static ONE_PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^\+?(?:\(?\d\)?[\s.\-]{{0,3}}){{5,15}}(?:;.*|{PHONE_EXTENSION})?$"
    ))
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

/// The document's text: its text nodes, joined with spaces. Script and style
/// text is code, not content. The spaces keep adjacent elements apart:
/// `<div>9631</div>` beside `<a>contact@domain.com</a>` must not read
/// `9631contact@domain.com`.
fn document_text(doc: &Html) -> String {
    doc.root_element()
        .descendants()
        .filter_map(|node| {
            let text = node.value().as_text()?;
            let parent = node.parent()?.value().as_element()?.name();
            (!matches!(parent, "script" | "style")).then_some(&**text)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract values from text content using a regex pattern
fn extract_with_regex(doc: &Html, regex: &Regex) -> Vec<String> {
    regex
        .find_iter(&document_text(doc))
        .map(|found| found.as_str().to_string())
        .collect()
}

/// The phone numbers `PHONE_REGEX` finds in `text`. A number inside a longer
/// run of digits is a piece of that run, not a number. A match that begins on
/// the last digit of the token before (`10001 (212) 555-1212` read from its
/// `1`, as a country code) is retried from the next character.
fn find_phones(text: &str) -> Vec<String> {
    let is_digit_at = |index: Option<usize>| {
        index.is_some_and(|index| text.as_bytes().get(index).is_some_and(u8::is_ascii_digit))
    };
    let mut phones = Vec::new();
    let mut from = 0;
    while let Some(found) = PHONE_REGEX.find_at(text, from) {
        if is_digit_at(found.start().checked_sub(1)) {
            // The match is ASCII, so the next byte is a character boundary.
            from = found.start() + 1;
            continue;
        }
        if !is_digit_at(Some(found.end())) {
            phones.push(found.as_str().to_string());
        }
        from = found.end();
    }
    phones
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
    find_phones(value)
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
        find_phones(&document_text(doc)),
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
