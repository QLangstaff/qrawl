use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use url::Url;

use html5ever::tokenizer::{TagKind, Token, TokenSink, TokenSinkResult};

use crate::utils::{
    absolute_url, canonical_domain, host_of_url, parse_document_bounded, parse_fragment_bounded,
    social_platform, tokenize_until, PHONE_EXTENSION, TLD_LEN,
};

// Lazy static regex for whitespace normalization
static WHITESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

/// A phone number's extension after its last digit, which is kept.
static PHONE_EXTENSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)[0-9]{PHONE_EXTENSION}")).expect("valid regex"));

// HTML normalization

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
/// 2. Cut RFC 3966 parameters (`;ext=123`, `;phone-context=…`)
/// 3. Strip an extension (` ext. 123`, `x123`, ` extension 123`)
/// 4. Keep international prefix (+) if present, and drop a trunk `(0)` after it
/// 5. Strip all other non-digit characters except leading +
pub(super) fn normalize_phone(phone: &str) -> String {
    let mut result = phone.trim().to_string();

    // RFC 3966 parameters (`;ext=12`, `;phone-context=…`) aren't the number.
    if let Some(pos) = result.find(';') {
        result.truncate(pos);
    }

    // The match starts at the number's last digit, which stays.
    if let Some(extension) = PHONE_EXTENSION_REGEX.find(&result) {
        result.truncate(extension.start() + 1);
    }

    // Keep international prefix, strip all other non-digits. After a country
    // code, a `(0)` is the trunk prefix a caller inside the country dials, not
    // part of the number.
    let has_plus = result.starts_with('+');
    if has_plus {
        result = result.replace("(0)", "");
    }
    let digits: String = result.chars().filter(|c| c.is_ascii_digit()).collect();

    if has_plus {
        format!("+{}", digits)
    } else {
        digits
    }
}

/// Elements that carry no content: code, styling, embedded frames, vector art.
const JUNK_ELEMENTS: [&str; 5] = ["script", "style", "noscript", "iframe", "svg"];

/// Elements that never have an end tag.
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Whether a `<script>` is JSON-LD: data, not code.
fn is_jsonld_script(element: &scraper::node::Element) -> bool {
    element.name() == "script"
        && element.attr("type").is_some_and(|script_type| {
            let essence = script_type.split(';').next().unwrap_or(script_type).trim();
            essence.eq_ignore_ascii_case("application/ld+json")
        })
}

/// What markup opens with, past a byte order mark, whitespace, comments, and
/// bogus comments like an XML prolog.
enum Opening {
    Doctype,
    StartTag(String),
    /// Text, or an end tag.
    Other,
}

/// A token sink that keeps the first token that opens the markup.
struct FirstToken(Option<Opening>);

impl TokenSink for FirstToken {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        if self.0.is_none() {
            self.0 = match token {
                Token::DoctypeToken(_) => Some(Opening::Doctype),
                Token::TagToken(tag) if tag.kind == TagKind::StartTag => {
                    Some(Opening::StartTag(tag.name.to_string()))
                }
                // Whitespace as the parser defines it: ASCII only.
                Token::CharacterTokens(text) if text.bytes().all(|b| b.is_ascii_whitespace()) => {
                    None
                }
                Token::CommentToken(_) | Token::ParseError(_) | Token::EOFToken => None,
                Token::TagToken(_) | Token::CharacterTokens(_) | Token::NullCharacterToken => {
                    Some(Opening::Other)
                }
            };
        }
        TokenSinkResult::Continue
    }
}

/// What `html` opens with, by the parser's own tokenizer, so the two never
/// disagree on where a comment ends.
fn opening(html: &str) -> Opening {
    tokenize_until(html, FirstToken(None), |first| first.0.is_some())
        .0
        .unwrap_or(Opening::Other)
}

/// The element a fragment's first tag belongs in, which decides how the
/// parser reads it: a `<tr>` outside a table is dropped, its cells' text run
/// together.
fn fragment_context(opening: &Opening) -> &'static str {
    let Opening::StartTag(name) = opening else {
        return "body";
    };
    match name.as_str() {
        "td" | "th" => "tr",
        "tr" => "tbody",
        "thead" | "tbody" | "tfoot" | "caption" | "colgroup" => "table",
        "col" => "colgroup",
        _ => "body",
    }
}

/// Normalize HTML by parsing it and writing back what is content: no comments,
/// no junk elements or attributes, text whitespace collapsed. JSON-LD scripts
/// are data, so each stays where it is with its text untouched. A parser
/// decides what is a comment, a script, or an attribute, so text that merely
/// looks like one is left alone.
pub(super) fn normalize_markup(html: &str) -> String {
    use ego_tree::iter::Edge;
    use scraper::node::Node;

    // A fragment (`<p>a</p>`) stays a fragment: no `<html>` is added to it.
    let opening = opening(html);
    let is_document = match &opening {
        Opening::Doctype => true,
        Opening::StartTag(name) => name == "html",
        Opening::Other => false,
    };
    let parsed = if is_document {
        parse_document_bounded(html)
    } else {
        parse_fragment_bounded(html, fragment_context(&opening))
    };
    // Too deep or too costly to parse: nothing, as every other reader has it.
    let Some(parsed) = parsed else {
        return String::new();
    };
    let root = if is_document {
        parsed.tree.root()
    } else {
        *parsed.root_element()
    };

    let mut out = String::new();
    // The junk element whose subtree is being skipped.
    let mut skipping = None;
    for edge in root.traverse() {
        match edge {
            Edge::Open(node) if skipping.is_none() && node != root => match node.value() {
                Node::Element(element)
                    if JUNK_ELEMENTS.contains(&element.name()) && !is_jsonld_script(element) =>
                {
                    skipping = Some(node.id());
                }
                Node::Element(element) => {
                    out.push('<');
                    out.push_str(element.name());
                    // In source order: `scraper`'s `deterministic` feature.
                    for (qualified, value) in &element.attrs {
                        // A foreign element's `xmlns` gets an empty prefix.
                        let name = match qualified.prefix.as_deref().filter(|p| !p.is_empty()) {
                            Some(prefix) => format!("{prefix}:{}", qualified.local),
                            None => qualified.local.to_string(),
                        };
                        if !is_junk_attribute(&name) {
                            out.push_str(&format!(" {name}=\"{}\"", escape(value, true)));
                        }
                    }
                    out.push('>');
                }
                Node::Text(text) => {
                    let in_jsonld = node
                        .parent()
                        .and_then(|parent| parent.value().as_element())
                        .is_some_and(is_jsonld_script);
                    if in_jsonld {
                        out.push_str(text);
                    } else {
                        push_collapsed(&mut out, &escape(text, false));
                    }
                }
                Node::Doctype(doctype) => match doctype.name() {
                    "" => out.push_str("<!DOCTYPE>"),
                    name => out.push_str(&format!("<!DOCTYPE {name}>")),
                },
                _ => {}
            },
            Edge::Close(node) if skipping == Some(node.id()) => skipping = None,
            Edge::Close(node) if skipping.is_none() && node != root => {
                if let Node::Element(element) = node.value() {
                    if !VOID_ELEMENTS.contains(&element.name()) {
                        out.push_str(&format!("</{}>", element.name()));
                    }
                }
            }
            _ => {}
        }
    }
    out.trim_matches(' ').to_string()
}

/// Whether an attribute is left out: one that carries styling, scripting, or
/// accessibility wiring, not content, or one whose name a tag's stray `<`,
/// quote, `=`, or `>` made, which would re-parse as something else.
fn is_junk_attribute(name: &str) -> bool {
    JUNK_ATTR_NAME_REGEX.is_match(name) || name.contains(['<', '>', '"', '\'', '='])
}

/// Append `text` with each run of whitespace as one space, and none doubled
/// where it meets the whitespace already written. A non-breaking space is
/// content, not layout, and stays.
fn push_collapsed(out: &mut String, text: &str) {
    for c in text.chars() {
        if !c.is_ascii_whitespace() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
}

fn escape(text: &str, in_attribute: bool) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if in_attribute {
        escaped.replace('"', "&quot;")
    } else {
        escaped
    }
}
