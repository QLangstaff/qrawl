/// Private helper functions for text cleaning
use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

// Lazy static regex for whitespace normalization
static WHITESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

/// Decode HTML entities (named and numeric).
///
/// Examples:
/// - `&amp;` → `&`
/// - `&lt;` → `<`
/// - `&#39;` → `'`
/// - `&#x27;` → `'`
pub fn decode_html_entities(text: &str) -> String {
    html_escape::decode_html_entities(text).to_string()
}

/// Normalize Unicode to NFC (Canonical Composition).
///
/// This ensures consistent representation of characters.
/// Example: `é` (U+00E9) and `é` (U+0065 U+0301) become the same.
pub fn normalize_unicode(text: &str) -> String {
    text.nfc().collect::<String>()
}

/// Remove zero-width characters that are invisible but can cause issues.
///
/// Removes:
/// - Zero Width Space (U+200B)
/// - Zero Width Non-Joiner (U+200C)
/// - Zero Width Joiner (U+200D)
/// - Zero Width No-Break Space / BOM (U+FEFF)
pub fn remove_zero_width_chars(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                *c,
                '\u{200B}' | // Zero width space
                '\u{200C}' | // Zero width non-joiner
                '\u{200D}' | // Zero width joiner
                '\u{FEFF}'   // Zero width no-break space (BOM)
            )
        })
        .collect()
}

/// Remove control characters except newlines and tabs.
///
/// Control characters can cause issues in display/storage.
/// We keep \n and \t as they're commonly used for formatting.
pub fn remove_control_chars(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// Normalize whitespace by collapsing multiple spaces/newlines and trimming.
///
/// - Multiple spaces → single space
/// - Multiple newlines → single space
/// - Trim leading/trailing whitespace
pub fn normalize_whitespace(text: &str) -> String {
    WHITESPACE_REGEX
        .replace_all(text, " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;&gt;"), "<>");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), "\u{00A0}");
    }

    #[test]
    fn test_normalize_unicode() {
        // Precomposed vs decomposed é
        let precomposed = "\u{00E9}"; // é (single char)
        let decomposed = "\u{0065}\u{0301}"; // e + combining acute
        assert_eq!(
            normalize_unicode(precomposed),
            normalize_unicode(decomposed)
        );
    }

    #[test]
    fn test_remove_zero_width_chars() {
        assert_eq!(remove_zero_width_chars("hello\u{200B}world"), "helloworld");
        assert_eq!(remove_zero_width_chars("test\u{200C}ing"), "testing");
        assert_eq!(remove_zero_width_chars("\u{FEFF}text"), "text");
    }

    #[test]
    fn test_remove_control_chars() {
        assert_eq!(remove_control_chars("hello\x00world"), "helloworld");
        assert_eq!(remove_control_chars("keep\nnewline"), "keep\nnewline");
        assert_eq!(remove_control_chars("keep\ttab"), "keep\ttab");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello   world"), "hello world");
        assert_eq!(normalize_whitespace("  trim  me  "), "trim me");
        assert_eq!(
            normalize_whitespace("multiple\n\n\nlines"),
            "multiple lines"
        );
        assert_eq!(normalize_whitespace("  lots   of    spaces  "), "lots of spaces");
    }
}
