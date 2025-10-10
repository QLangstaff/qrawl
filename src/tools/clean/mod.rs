mod utils;

use utils::*;

/// Clean text.
///
/// Performs the following operations in order:
/// 1. Decode HTML entities (`&amp;` → `&`, `&#39;` → `'`)
/// 2. Normalize Unicode to NFC (canonical composition)
/// 3. Remove zero-width characters
/// 4. Remove control characters (except newlines/tabs)
/// 5. Normalize whitespace (collapse multiple spaces, trim)
///
/// # Examples
/// ```
/// use qrawl::tools::clean::clean;
///
/// let dirty = "Hello &amp; &#39;world&#39;   with   spaces";
/// let result = clean(dirty);
/// assert_eq!(result, "Hello & 'world' with spaces");
/// ```
pub fn clean(text: &str) -> String {
    let mut result = text.to_string();

    // Step 1: Decode HTML entities
    result = decode_html_entities(&result);

    // Step 2: Normalize Unicode
    result = normalize_unicode(&result);

    // Step 3: Remove zero-width characters
    result = remove_zero_width_chars(&result);

    // Step 4: Remove control characters
    result = remove_control_chars(&result);

    // Step 5: Normalize whitespace
    result = normalize_whitespace(&result);

    result
}

/// Clean all text strings in a collection.
///
/// # Examples
/// ```
/// use qrawl::tools::clean::clean_all;
///
/// let texts = vec![
///     "Text &amp; stuff".to_string(),
///     "More &#39;text&#39;".to_string(),
/// ];
/// let cleaned = clean_all(&texts);
/// assert_eq!(cleaned[0], "Text & stuff");
/// assert_eq!(cleaned[1], "More 'text'");
/// ```
pub fn clean_all(texts: &[String]) -> Vec<String> {
    texts.iter().map(|t| clean(t)).collect()
}
