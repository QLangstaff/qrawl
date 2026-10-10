use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::utils::{absolute_url, canonical_domain, host_of_url, social_platform, TLD_LEN};

// Lazy static regex for whitespace normalization
static WHITESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

// HTML normalization regexes
static JSONLD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)^<script[^>]*type=["']?application/ld\+json["']?[^>]*>"#)
        .expect("valid regex")
});

static SCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("valid regex"));

static STYLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("valid regex"));

static NOSCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("valid regex"));

static IFRAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<iframe[^>]*>.*?</iframe>").expect("valid regex"));

static SVG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<svg[^>]*>.*?</svg>").expect("valid regex"));

static COMMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));

static START_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[a-zA-Z][^>]*>").expect("valid regex"));

/// One attribute of a start tag: its name, then its value — quoted values
/// first, so a space or `=` inside one never reads as another attribute.
static ATTR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\s+([^\s=>/"']+)(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?"#).expect("valid regex")
});

/// Attribute names that carry styling, scripting, or accessibility wiring,
/// not content.
static JUNK_ATTR_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^(?:
            class|id|style|
            data-[\w-]+|
            aria-[\w-]+|
            role|tabindex|
            xmlns(?::[\w-]+)?|
            version|viewBox|
            fill|fill-rule|stroke(?:-[\w-]+)?|
            on[\w-]+
        )$",
    )
    .expect("valid regex")
});

static NEWLINE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\n").expect("valid regex"));

/// Tracking / analytics query parameters stripped by `normalize_url` so the
/// same page reached via different campaigns/referrers canonicalizes identically.
///
/// Coverage is the top ~10 sources that dominate real-world noise; not exhaustive.
/// In addition to this exact-match list, any param whose name starts with `utm_`
/// is stripped (covers the full Urchin Tracking Module family).
const TRACKING_PARAM_PREFIXES: &[&str] = &["utm_"];
const TRACKING_PARAM_NAMES: &[&str] = &[
    "fbclid",  // Facebook click ID
    "gclid",   // Google Ads click ID
    "mc_eid",  // Mailchimp email ID
    "mc_cid",  // Mailchimp campaign ID
    "_ga",     // Google Analytics
    "igshid",  // Instagram share ID
    "ref_src", // Twitter/X referrer source
    "ref_url", // generic referrer URL
];

fn is_tracking_param(key: &str) -> bool {
    TRACKING_PARAM_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || TRACKING_PARAM_NAMES.contains(&key)
}

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
fn normalize_escaped_newlines(text: &str) -> String {
    NEWLINE_REGEX.replace_all(text, "\n").to_string()
}

/// Normalize each item, dropping the ones that normalize to nothing and the
/// repeats, in first-seen order.
pub(super) fn normalize_each(items: &[String], normalize: impl Fn(&str) -> String) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|item| normalize(item))
        .filter(|normalized| !normalized.is_empty() && seen.insert(normalized.clone()))
        .collect()
}

/// The general canonicalization every URL gets.
///
/// Performs:
/// 1. Add https:// if protocol is missing
/// 2. Normalize protocol to https
/// 3. Canonicalize domain (lowercase, IDNA, strip www)
/// 4. Normalize path (strip all trailing slashes)
/// 5. Strip tracking / analytics query params (utm_*, fbclid, gclid, ...)
/// 6. Sort remaining query parameters
/// 7. Remove fragment
///
/// Examples:
/// - `example.com` → `https://example.com`
/// - `HTTP://Example.com/path/` → `https://example.com/path`
/// - `https://www.example.com?b=2&a=1` → `https://example.com/?a=1&b=2`
/// - `https://example.com/page#section` → `https://example.com/page`
/// - `https://example.com?utm_source=x&id=7` → `https://example.com/?id=7`
fn normalize_any_url(url: &str) -> String {
    let Some(url_with_protocol) = absolute_url(url) else {
        return url.to_string();
    };

    let mut parsed = match Url::parse(&url_with_protocol) {
        Ok(u) => u,
        Err(_) => return url.to_string(), // Keep malformed URLs as-is
    };

    // 1. Normalize protocol to https
    let _ = parsed.set_scheme("https");

    // 2. Canonicalize domain
    if let Some(host) = parsed.host_str() {
        let canonical_host = canonical_domain(host);
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

    // 4. Strip tracking params, then sort the remaining query parameters by
    // key. The pairs stay percent-encoded, so `?q=a%26b` keeps its one value.
    let query = kept_query(&parsed, |key| !is_tracking_param(key), true);
    parsed.set_query(query.as_deref());

    // 5. Remove fragment
    parsed.set_fragment(None);

    // url crate adds trailing slash for empty path, strip it
    serialize(&parsed)
}

/// Canonicalize a URL: the one form two URLs of the same page share.
///
/// Performs the general pass (`normalize_any_url`), then — for a recognized
/// social platform only — strips a mobile host prefix (`m.`/`mobile.`) and the
/// share / referrer tokens (`si`, `s`, `feature`, …) that would otherwise
/// split one item across look-alike URLs. A plain web page keeps a functional
/// `?s=`.
pub fn normalize_url(url: &str) -> String {
    let normalized = normalize_any_url(url);
    let Ok(mut parsed) = Url::parse(&normalized) else {
        return normalized;
    };
    let Some(host) = host_of_url(&parsed) else {
        return normalized;
    };
    // Social rules apply only to recognized platforms (so a plain web page keeps
    // a functional `?s=` / `?ref=` and its `m.` host).
    if social_platform(&host).is_none() {
        return normalized;
    }

    // Strip a mobile host prefix (the `vm.`/`vt.` shorteners are kept on purpose).
    let mut desktop_host = host.as_str();
    while let Some(stripped) = desktop_host
        .strip_prefix("m.")
        .or_else(|| desktop_host.strip_prefix("mobile."))
        .or_else(|| desktop_host.strip_prefix("www."))
    {
        desktop_host = stripped;
    }
    let _ = parsed.set_host(Some(desktop_host));

    let query = kept_query(&parsed, |key| !is_social_tracker(key), false);
    parsed.set_query(query.as_deref());

    serialize(&parsed)
}

/// `url` as a string, without the `/` the `url` crate writes for an empty
/// path. A query keeps it (`https://example.com/?a=1`), so a slash inside a
/// query value is never mistaken for it.
fn serialize(url: &Url) -> String {
    let serialized = url.to_string();
    match (url.path(), url.query()) {
        ("/", None) => serialized.trim_end_matches('/').to_string(),
        _ => serialized,
    }
}

/// The query of `url` with only the pairs whose key `keep` accepts, as written
/// (still percent-encoded), sorted by key when `sort`. `None` when no pair is
/// left.
fn kept_query(url: &Url, keep: impl Fn(&str) -> bool, sort: bool) -> Option<String> {
    let key_of = |pair: &str| pair.split('=').next().unwrap_or(pair).to_string();
    let mut pairs: Vec<&str> = url
        .query()?
        .split('&')
        .filter(|pair| !pair.is_empty() && keep(&key_of(pair)))
        .collect();
    if sort {
        pairs.sort_by_key(|pair| key_of(pair));
    }
    (!pairs.is_empty()).then(|| pairs.join("&"))
}

/// Social share / referrer tokens [`normalize_url`] strips on top of the
/// general trackers, for recognized platforms only.
fn is_social_tracker(key: &str) -> bool {
    matches!(
        key,
        "si" | "igsh"
            | "s"
            | "_t"
            | "_r"
            | "ref"
            | "share_id"
            | "share_app_id"
            | "is_from_webapp"
            | "sender_device"
            | "feature"
            | "rdt"
            | "spm"
    )
}

/// Normalize a single email address.
///
/// Performs:
/// 1. Trim whitespace
/// 2. Strip trailing punctuation (comma, semicolon, period)
/// 3. Extract from display name format: "Name" <email> or Name <email>
/// 4. URL decode (%40 → @)
/// 5. Lowercase
/// 6. Validate format (returns empty string if invalid)
pub(super) fn normalize_email(email: &str) -> String {
    let mut result = email.trim().to_string();

    // Strip trailing punctuation (common in comma-separated lists)
    result = result.trim_end_matches(&[',', ';', '.'][..]).to_string();

    // Extract from display name format: "Name" <email@example.com> or Name <email@example.com>
    if let Some((_, after_open)) = result.split_once('<') {
        if let Some((address, _)) = after_open.split_once('>') {
            result = address.to_string();
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
            if !TLD_LEN.contains(&tld.len()) || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
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

/// Normalize a single phone number.
///
/// Performs:
/// 1. Trim whitespace
/// 2. Strip a space-separated extension (` ext. 123`, ` x123`, ` extension 123`)
/// 3. Keep international prefix (+) if present
/// 4. Strip all other non-digit characters except leading +
pub(super) fn normalize_phone(phone: &str) -> String {
    let mut result = phone.trim().to_string();

    // Strip extension patterns
    // ASCII lowercasing keeps byte offsets, so `pos` indexes `result` too.
    let lower = result.to_ascii_lowercase();
    if let Some(pos) = lower.find(" ext").or_else(|| lower.find(" x")) {
        result.truncate(pos);
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

/// Normalize HTML: drop comments, scripts, styles, and junk attributes, and
/// collapse whitespace. JSON-LD scripts are data: each stays where it is,
/// byte for byte.
pub(super) fn normalize_markup(html: &str) -> String {
    // Comments go first, so a `<script>` inside one can't pair with a real
    // `</script>` further down.
    let html = COMMENT_REGEX.replace_all(html, "");
    let mut normalized = String::new();
    let mut rest = 0;
    for script in SCRIPT_REGEX.find_iter(&html) {
        normalized.push_str(&strip_junk(&html[rest..script.start()]));
        if JSONLD_REGEX.is_match(script.as_str()) {
            normalized.push_str(script.as_str());
        }
        rest = script.end();
    }
    normalized.push_str(&strip_junk(&html[rest..]));
    normalized.trim().to_string()
}

/// Strip junk from script-free HTML and collapse its whitespace.
fn strip_junk(html: &str) -> String {
    let mut stripped = normalize_escaped_newlines(html);
    stripped = STYLE_REGEX.replace_all(&stripped, "").to_string();
    stripped = NOSCRIPT_REGEX.replace_all(&stripped, "").to_string();
    stripped = IFRAME_REGEX.replace_all(&stripped, "").to_string();
    stripped = SVG_REGEX.replace_all(&stripped, "").to_string();
    stripped = START_TAG_REGEX
        .replace_all(&stripped, |tag: &regex::Captures| {
            ATTR_REGEX
                .replace_all(&tag[0], |attr: &regex::Captures| {
                    let has_value = attr[0].contains('=');
                    if has_value && JUNK_ATTR_NAME_REGEX.is_match(&attr[1]) {
                        String::new()
                    } else {
                        attr[0].to_string()
                    }
                })
                .to_string()
        })
        .to_string();
    WHITESPACE_REGEX.replace_all(&stripped, " ").to_string()
}
