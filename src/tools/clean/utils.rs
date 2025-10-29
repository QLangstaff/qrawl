use once_cell::sync::Lazy;
/// Helper functions for text cleaning
use regex::Regex;
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;
use url::Url;

// Lazy static regex for whitespace normalization
static WHITESPACE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").expect("valid regex"));

// HTML cleaning regexes
static JSONLD_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<script[^>]*type=["']application/ld\+json["'][^>]*>.*?</script>"#)
        .expect("valid regex")
});

static SCRIPT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("valid regex"));

static STYLE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("valid regex"));

static NOSCRIPT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("valid regex"));

static IFRAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<iframe[^>]*>.*?</iframe>").expect("valid regex"));

static SVG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<svg[^>]*>.*?</svg>").expect("valid regex"));

static COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));

static JUNK_ATTR_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)
        \s+                                    # Leading whitespace
        (?:                                    # Attribute name (non-capturing group)
            class|id|style|                    # Common styling attributes
            data-[\w-]+|                       # All data-* attributes
            aria-[\w-]+|                       # All aria-* attributes
            role|tabindex|                     # Accessibility attributes
            xmlns(?::[\w-]+)?|                 # XML namespaces
            version|viewBox|                   # SVG attributes
            fill|fill-rule|stroke(?:-[\w-]+)?| # SVG styling
            onclick|onload|on[\w-]+            # Event handlers
        )
        \s*=\s*                                # Equals with optional whitespace
        (?:                                    # Value (non-capturing group)
            "[^"]*"|                           # Double-quoted value
            '[^']*'|                           # Single-quoted value
            [^\s>]+                            # Unquoted value
        )
        "#,
    )
    .expect("valid regex")
});

static NEWLINE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\\n").expect("valid regex"));

/// Decode HTML entities (named and numeric).
///
/// Examples:
/// - `&amp;` → `&`
/// - `&lt;` → `<`
/// - `&#39;` → `'`
/// - `&#x27;` → `'`
pub(super) fn decode_html_entities(text: &str) -> String {
    html_escape::decode_html_entities(text).to_string()
}

/// Normalize Unicode to NFC (Canonical Composition).
///
/// This ensures consistent representation of characters.
/// Example: `é` (U+00E9) and `é` (U+0065 U+0301) become the same.
pub(super) fn normalize_unicode(text: &str) -> String {
    text.nfc().collect::<String>()
}

/// Remove zero-width characters that are invisible but can cause issues.
///
/// Removes:
/// - Zero Width Space (U+200B)
/// - Zero Width Non-Joiner (U+200C)
/// - Zero Width Joiner (U+200D)
/// - Zero Width No-Break Space / BOM (U+FEFF)
pub(super) fn remove_zero_width_chars(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                *c,
                '\u{200B}' | // Zero width space
                '\u{200C}' | // Zero width non-joiner
                '\u{200D}' | // Zero width joiner
                '\u{FEFF}' // Zero width no-break space (BOM)
            )
        })
        .collect()
}

/// Remove control characters except newlines and tabs.
///
/// Control characters can cause issues in display/storage.
/// We keep \n and \t as they're commonly used for formatting.
pub(super) fn remove_control_chars(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\r' { '\n' } else { c })
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// Normalize whitespace by collapsing multiple spaces/newlines and trimming.
///
/// - Multiple spaces → single space
/// - Multiple newlines → single space
/// - Trim leading/trailing whitespace
pub(super) fn normalize_whitespace(text: &str) -> String {
    WHITESPACE_REGEX.replace_all(text, " ").trim().to_string()
}

/// Normalize escaped newlines (\\n) to actual newlines (\n).
pub(super) fn normalize_escaped_newlines(text: &str) -> String {
    NEWLINE_REGEX.replace_all(text, "\n").to_string()
}

/// Canonicalize a domain name for comparison.
///
/// Performs:
/// 1. Convert to lowercase
/// 2. IDNA/Punycode normalization
/// 3. Strip www. prefix
///
/// Examples:
/// - `Example.com` → `example.com`
/// - `WWW.Example.COM` → `example.com`
/// - `www.GitHub.com` → `github.com`
pub fn canonicalize_domain(host: &str) -> String {
    let lower = host.to_ascii_lowercase();
    let idna = idna::domain_to_ascii(&lower).unwrap_or(lower);

    // Strip www. prefix to normalize domains
    if idna.starts_with("www.") && idna.len() > 4 {
        idna[4..].to_string()
    } else {
        idna
    }
}

/// Canonicalize a URL for comparison.
///
/// Performs:
/// 1. Add https:// if protocol is missing
/// 2. Normalize protocol to https
/// 3. Canonicalize domain (lowercase, IDNA, strip www)
/// 4. Normalize path (strip all trailing slashes)
/// 5. Sort query parameters
/// 6. Remove fragment
///
/// Examples:
/// - `example.com` → `https://example.com`
/// - `HTTP://Example.com/path/` → `https://example.com/path`
/// - `https://www.example.com?b=2&a=1` → `https://example.com?a=1&b=2`
/// - `https://example.com/page#section` → `https://example.com/page`
pub(super) fn canonicalize_url(url: &str) -> String {
    // Prepend https:// if protocol is missing (case-insensitive check)
    // Only prepend if it looks like a domain (contains a dot)
    let url_lower = url.to_ascii_lowercase();
    let url_with_protocol = if url_lower.starts_with("http://") || url_lower.starts_with("https://")
    {
        url.to_string()
    } else if url.contains('.') {
        format!("https://{}", url)
    } else {
        url.to_string()
    };

    let mut parsed = match Url::parse(&url_with_protocol) {
        Ok(u) => u,
        Err(_) => return url.to_string(), // Keep malformed URLs as-is
    };

    // 1. Normalize protocol to https
    let _ = parsed.set_scheme("https");

    // 2. Canonicalize domain
    if let Some(host) = parsed.host_str() {
        let canonical_host = canonicalize_domain(host);
        let _ = parsed.set_host(Some(&canonical_host));
    }

    // 3. Normalize path (strip all trailing slashes)
    let path = parsed.path().to_string();
    let normalized = path.trim_end_matches('/');
    let new_path = if normalized.is_empty() {
        ""
    } else {
        normalized
    };
    parsed.set_path(new_path);

    // 4. Sort query parameters
    if parsed.query().is_some() {
        let params: BTreeMap<_, _> = parsed.query_pairs().collect();
        if !params.is_empty() {
            let sorted_query = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            parsed.set_query(Some(&sorted_query));
        } else {
            parsed.set_query(None);
        }
    }

    // 5. Remove fragment
    parsed.set_fragment(None);

    // url crate adds trailing slash for empty path, strip it
    parsed.to_string().trim_end_matches('/').to_string()
}

/// Clean a single email address.
///
/// Performs:
/// 1. Trim whitespace
/// 2. Strip trailing punctuation (comma, semicolon, period)
/// 3. Extract from display name format: "Name" <email> or Name <email>
/// 4. URL decode (%40 → @)
/// 5. Lowercase
/// 6. Validate format (returns empty string if invalid)
pub(super) fn clean_email(email: &str) -> String {
    let mut result = email.trim().to_string();

    // Strip trailing punctuation (common in comma-separated lists)
    result = result.trim_end_matches(&[',', ';', '.'][..]).to_string();

    // Extract from display name format: "Name" <email@example.com> or Name <email@example.com>
    if let Some(start) = result.find('<') {
        if let Some(end) = result.find('>') {
            result = result[start + 1..end].to_string();
        }
    }

    // URL decode (handle %40 and other encoded characters like %20 for space)
    result = urlencoding::decode(&result)
        .unwrap_or(std::borrow::Cow::Borrowed(&result))
        .to_string();

    // Trim again after URL decoding (in case %20 or other encoded whitespace was decoded)
    result = result.trim().to_string();

    // Lowercase (treat emails as case-insensitive)
    result = result.to_ascii_lowercase();

    // Validate: must have exactly one @, and domain must have valid TLD
    if let Some(at_pos) = result.find('@') {
        // Must have exactly one @
        if result.matches('@').count() != 1 {
            return String::new();
        }

        let (_local, domain) = result.split_at(at_pos);
        let domain = &domain[1..]; // Skip the @

        // Domain must have at least one dot
        if !domain.contains('.') {
            return String::new();
        }

        // Get TLD (last segment after final dot)
        if let Some(tld) = domain.split('.').next_back() {
            // TLD must be 2-10 letters only (real TLDs are typically short)
            if tld.len() < 2 || tld.len() > 10 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
                return String::new();
            }

            // Reject common file extensions that might slip through
            let file_extensions = [
                "js", "css", "jpg", "jpeg", "png", "gif", "svg", "webp", "ico", "pdf", "doc",
                "docx", "xls", "xlsx", "zip", "tar", "gz", "mp3", "mp4", "avi", "mov", "prod",
            ];
            if file_extensions.contains(&tld) {
                return String::new();
            }
        }
    } else {
        // No @ found
        return String::new();
    }

    result
}

/// Clean a single phone number.
///
/// Performs:
/// 1. Trim whitespace
/// 2. Strip extension patterns (ext., x, extension)
/// 3. Keep international prefix (+) if present
/// 4. Strip all other non-digit characters except leading +
pub(super) fn clean_phone(phone: &str) -> String {
    let mut result = phone.trim().to_string();

    // Strip extension patterns
    if let Some(pos) = result
        .to_lowercase()
        .find(" ext")
        .or_else(|| result.to_lowercase().find(" x"))
        .or_else(|| result.to_lowercase().find(" extension"))
    {
        result = result[..pos].to_string();
    }

    // Keep international prefix, strip all other non-digits
    let has_plus = result.starts_with('+');
    let digits: String = result.chars().filter(|c| c.is_ascii_digit()).collect();

    if has_plus {
        format!("+{}", digits)
    } else {
        digits
    }
}

/// Strip junk from HTML (scripts, styles, comments, junk attributes).
///
/// Implementation for clean_html. Contains all the messy regex logic.
pub(super) fn strip_junk(html: &str) -> String {
    // Extract and protect JSON-LD scripts before removing all scripts
    let jsonld_scripts: Vec<String> = JSONLD_REGEX
        .captures_iter(html)
        .map(|cap| cap.get(0).unwrap().as_str().to_string())
        .collect();

    // Remove non-content elements (including all scripts)
    let mut cleaned = SCRIPT_REGEX.replace_all(html, "").to_string();

    // Restore JSON-LD scripts after removing JavaScript
    for jsonld in jsonld_scripts {
        cleaned = format!("{}{}", cleaned, jsonld);
    }

    cleaned = STYLE_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = NOSCRIPT_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = IFRAME_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = SVG_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = COMMENT_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = JUNK_ATTR_REGEX.replace_all(&cleaned, "").to_string();

    cleaned
}
