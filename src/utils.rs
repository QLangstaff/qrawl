//! Shared Utils
//!
//! What more than one tool needs. A tool never calls another tool; it calls
//! this.

use scraper::Selector;
use std::sync::LazyLock;

use crate::types::{Html, SocialPlatform};

mod social;

pub(crate) use social::social_parts;

/// Selector for anchor elements with hrefs.
pub(crate) static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid link selector"));

/// Selector for JSON-LD script tags.
pub(crate) static JSONLD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("script[type='application/ld+json']").expect("valid jsonld selector")
});

/// The lowercased host of `url`, without the root dot of a fully qualified
/// name (`example.com.`), or `None` when `url` has no host (`mailto:`,
/// `file:`).
pub(crate) fn host_of_url(url: &url::Url) -> Option<String> {
    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

/// [`host_of_url`] for a URL string, or `None` when it doesn't parse.
pub(crate) fn host_of(url: &str) -> Option<String> {
    host_of_url(&url::Url::parse(url).ok()?)
}

/// Whether `host` is `domain` or a subdomain of it (`old.reddit.com` matches
/// `reddit.com`; `notreddit.com` doesn't).
pub(crate) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// A host in the form two hosts compare in: lowercased, IDNA-encoded, without
/// the root dot of a fully qualified name or a `www.` prefix
/// (`WWW.Example.COM.` → `example.com`). `www.com` keeps its `www.`: without it
/// the host would be a bare TLD.
pub(crate) fn canonical_domain(host: &str) -> String {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    let idna = idna::domain_to_ascii(&lower).unwrap_or(lower);
    let mut apex = idna.as_str();
    while let Some(stripped) = apex.strip_prefix("www.").filter(|rest| rest.contains('.')) {
        apex = stripped;
    }
    apex.to_string()
}

/// The domain a list entry names, in [`canonical_domain`] form. An entry may
/// be written as a host, a wildcard (`*.example.com`), a host with a port or
/// a path, or a whole URL.
pub(crate) fn listed_domain(entry: &str) -> String {
    let entry = entry.trim();
    let host = host_of(entry).unwrap_or_else(|| {
        let without_wildcard = entry.trim_start_matches("*.").trim_start_matches('.');
        let without_path = without_wildcard
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(without_wildcard);
        without_path
            .rsplit_once(':')
            .filter(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
            .map_or(without_path, |(host, _port)| host)
            .to_string()
    });
    canonical_domain(&host)
}

/// The `http(s)` URL `input` names — with `https://` when it names no scheme
/// (`example.com/a`) — or `None` when it isn't one (`mailto:`, `tel:911`,
/// `ftp:`, a relative path, an email address, a bare word or number).
///
/// With an explicit `http(s)://`, any host is taken as written (`intranet`,
/// an IDN, an IP address). Without one, the input has to look like a host for
/// `https://` to be put before it.
pub(crate) fn absolute_url(input: &str) -> Option<String> {
    let input = input.trim();
    let http_url = |candidate: &str| {
        url::Url::parse(candidate)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
    };
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return http_url(input).map(|_| input.to_string());
    }
    // `https:example.com` is a URL the `url` crate completes.
    if let Some(rest) = lower
        .strip_prefix("https:")
        .or_else(|| lower.strip_prefix("http:"))
    {
        return http_url(input)
            .filter(|_| names_a_host(rest.trim_start_matches('/')))
            .map(String::from);
    }
    if !names_a_host(input) {
        return None;
    }
    let candidate = format!("https://{input}");
    http_url(&candidate).map(|_| candidate)
}

/// Whether a scheme-less input starts with a host: a name under a TLD,
/// `localhost`, or an IP address, with an optional port — not a scheme
/// (`tel:911`), a path, an email address, or a number (`3.14`).
fn names_a_host(input: &str) -> bool {
    let authority = input.split(['/', '?', '#']).next().unwrap_or(input);
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        // A bracketed IPv6 address, with an optional port after it.
        Some(bracketed) => return bracketed.contains(']'),
        None => match authority.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
                host
            }
            Some(_) => return false,
            None => authority,
        },
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    // A label is letters, digits, hyphens and underscores; an IDN's letters
    // aren't ASCII.
    let is_label = |label: &&str| {
        !label.is_empty()
            && label
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    };
    let is_ipv4 = labels.len() == 4 && labels.iter().all(|label| label.parse::<u8>().is_ok());
    // A TLD is letters, or punycode (`xn--p1ai`).
    let has_tld = labels.len() >= 2
        && labels.iter().all(is_label)
        && labels.last().is_some_and(|tld| {
            tld.len() >= 2 && (tld.starts_with("xn--") || tld.chars().all(char::is_alphabetic))
        });
    is_ipv4 || has_tld
}

/// The social platform a host belongs to, or `None` for a non-social host.
/// Subdomain-aware (`old.reddit.com`, `m.facebook.com`, `vm.tiktok.com`) and
/// handles the alt short-domains (`youtu.be`, `fb.me`, …).
pub(crate) fn social_platform(host: &str) -> Option<SocialPlatform> {
    const DOMAINS: &[(&str, SocialPlatform)] = &[
        ("reddit.com", SocialPlatform::Reddit),
        ("redd.it", SocialPlatform::Reddit),
        ("tiktok.com", SocialPlatform::TikTok),
        ("instagram.com", SocialPlatform::Instagram),
        ("instagr.am", SocialPlatform::Instagram),
        ("facebook.com", SocialPlatform::Facebook),
        ("fb.me", SocialPlatform::Facebook),
        ("fb.com", SocialPlatform::Facebook),
        ("fb.watch", SocialPlatform::Facebook),
        ("youtube.com", SocialPlatform::YouTube),
        ("youtube-nocookie.com", SocialPlatform::YouTube),
        ("youtu.be", SocialPlatform::YouTube),
        ("twitter.com", SocialPlatform::Twitter),
        ("x.com", SocialPlatform::Twitter),
        ("t.co", SocialPlatform::Twitter),
        ("linkedin.com", SocialPlatform::LinkedIn),
        ("lnkd.in", SocialPlatform::LinkedIn),
        ("vimeo.com", SocialPlatform::Vimeo),
        ("pinterest.com", SocialPlatform::Pinterest),
        ("pin.it", SocialPlatform::Pinterest),
    ];
    DOMAINS
        .iter()
        .find(|(domain, _)| host_matches(host, domain))
        .map(|(_, platform)| *platform)
        // Pinterest's country hosts, each verified to redirect to
        // `<country>.pinterest.com`. A `pinterest` label alone proves nothing:
        // `pinterest.io` and `pinterest.evil.com` aren't Pinterest.
        .or_else(|| {
            const COUNTRY_SUFFIXES: [&str; 28] = [
                "at", "be", "ca", "ch", "cl", "co.in", "co.kr", "co.nz", "co.uk", "com.au",
                "com.mx", "de", "dk", "es", "fr", "id", "ie", "in", "it", "jp", "kr", "mx", "nl",
                "nz", "ph", "pt", "ru", "se",
            ];
            let suffix = host
                .strip_prefix("pinterest.")
                .or_else(|| host.split_once(".pinterest.").map(|(_, suffix)| suffix))?;
            COUNTRY_SUFFIXES
                .contains(&suffix)
                .then_some(SocialPlatform::Pinterest)
        })
}

/// The short name of a schema.org type: a full IRI
/// (`https://schema.org/Recipe`), a CURIE (`schema:Recipe`), and a bare term
/// all yield `Recipe`.
pub(crate) fn short_schema_type(declared: &str) -> Option<String> {
    declared
        .rsplit([':', '/', '#'])
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

/// The schema.org types an entity declares in `@type`, as
/// [`short_schema_type`] names, whether `@type` is one string or an array.
pub(crate) fn schema_types(entity: &serde_json::Value) -> Vec<String> {
    match entity.get("@type") {
        Some(serde_json::Value::String(declared)) => {
            short_schema_type(declared).into_iter().collect()
        }
        Some(serde_json::Value::Array(declared)) => declared
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(short_schema_type)
            .collect(),
        _ => Vec::new(),
    }
}

/// The metadata keys an embed is scraped into (`scrape_embed`) and read back
/// from (`extract_preview`).
pub(crate) mod embed_keys {
    pub(crate) const OEMBED_TITLE: &str = "oembed:title";
    pub(crate) const OEMBED_AUTHOR_NAME: &str = "oembed:author_name";
    pub(crate) const OEMBED_THUMBNAIL_URL: &str = "oembed:thumbnail_url";
    pub(crate) const REDDIT_TITLE: &str = "reddit:title";
    pub(crate) const REDDIT_TEXT: &str = "reddit:text";
    pub(crate) const REDDIT_AUTHOR: &str = "reddit:author";
    pub(crate) const REDDIT_IMAGE: &str = "reddit:image";
    pub(crate) const REDDIT_VIDEO: &str = "reddit:video";
    /// A link post's destination: the page the post is about.
    pub(crate) const REDDIT_URL: &str = "reddit:url";
    /// The prefixes only an embed may use, so a page's own `<meta>` tags can't
    /// pose as one.
    pub(crate) const PREFIXES: [&str; 2] = ["oembed:", "reddit:"];
}

/// Nodes nested deeper than this are not a page. The walkers that read a
/// document recurse once per level, so the bound and [`READ_STACK_BYTES`]
/// together keep them from overflowing the stack, which would abort the
/// process.
const MAX_DOM_DEPTH: usize = 512;

/// The stack a document is read on: enough for every walker at
/// [`MAX_DOM_DEPTH`] in a debug build, whatever stack size the caller's
/// runtime gives its blocking threads.
const READ_STACK_BYTES: usize = 32 * 1024 * 1024;

fn is_too_deep(document: &scraper::Html) -> bool {
    let mut depth = 0usize;
    document.tree.root().traverse().any(|edge| {
        match edge {
            ego_tree::iter::Edge::Open(_) => depth += 1,
            ego_tree::iter::Edge::Close(_) => depth -= 1,
        }
        depth > MAX_DOM_DEPTH
    })
}

/// Parse `html` and read it with `read`, off the async runtime: building a DOM
/// is CPU-bound, and a parsed document can't cross an `await`. A document
/// nested deeper than [`MAX_DOM_DEPTH`] is read as an empty one.
pub(crate) async fn with_document<T: Send + 'static>(
    html: &Html,
    read: impl FnOnce(&scraper::Html) -> T + Send + 'static,
) -> T {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(READ_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    let document = scraper::Html::parse_document(&html);
                    if is_too_deep(&document) {
                        log::warn!(
                            "document nested deeper than {MAX_DOM_DEPTH} nodes read as empty"
                        );
                        return read(&scraper::Html::parse_document(""));
                    }
                    read(&document)
                })
                .expect("spawning the document reader")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    })
    .await
    .expect("reading a parsed document panicked")
}

/// How long a top-level domain is: `.io` to `.international` and beyond. An
/// email is found (`extract`) and kept (`normalize`) under the same bound.
pub(crate) const TLD_LEN: std::ops::RangeInclusive<usize> = 2..=24;
