//! Shared Utils
//!
//! What more than one tool needs. A tool never calls another tool; it calls
//! this.

use once_cell::sync::Lazy;
use scraper::Selector;

use crate::types::{Html, SocialPlatform};

mod social;

pub(crate) use social::social_parts;

/// Selector for anchor elements with hrefs.
pub(crate) static LINK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("a[href]").expect("valid link selector"));

/// Selector for JSON-LD script tags.
pub(crate) static JSONLD_SELECTOR: Lazy<Selector> = Lazy::new(|| {
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
/// a `www.` prefix (`WWW.Example.COM` → `example.com`).
pub(crate) fn canonical_domain(host: &str) -> String {
    let lower = host.to_ascii_lowercase();
    let idna = idna::domain_to_ascii(&lower).unwrap_or(lower);
    let mut apex = idna.as_str();
    while let Some(stripped) = apex.strip_prefix("www.").filter(|rest| !rest.is_empty()) {
        apex = stripped;
    }
    apex.to_string()
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
        // Pinterest ships many ccTLDs: a `pinterest` label followed only by
        // a country suffix (`pinterest.de`, `pinterest.co.uk`,
        // `pinterest.com.au`), which `pinterest.evil.com` isn't.
        .or_else(|| {
            let labels: Vec<&str> = host.split('.').collect();
            let suffix = &labels[labels.iter().position(|label| *label == "pinterest")? + 1..];
            let is_country_suffix = match suffix {
                [country] => country.len() == 2,
                [second_level, country] => {
                    matches!(*second_level, "co" | "com") && country.len() == 2
                }
                _ => false,
            };
            is_country_suffix.then_some(SocialPlatform::Pinterest)
        })
}

/// The schema.org types an entity declares in `@type`, as short names: a full
/// IRI (`https://schema.org/Recipe`), a CURIE (`schema:Recipe`), and a bare
/// term all yield `Recipe`, whether `@type` is one string or an array.
pub(crate) fn schema_types(entity: &serde_json::Value) -> Vec<String> {
    let short = |declared: &str| {
        declared
            .rsplit([':', '/', '#'])
            .find(|segment| !segment.is_empty())
            .map(str::to_string)
    };
    match entity.get("@type") {
        Some(serde_json::Value::String(declared)) => short(declared).into_iter().collect(),
        Some(serde_json::Value::Array(declared)) => declared
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(short)
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

/// Parse `html` and read it with `read`, off the async runtime: building a DOM
/// is CPU-bound, and a parsed document can't cross an `await`.
pub(crate) async fn with_document<T: Send + 'static>(
    html: &Html,
    read: impl FnOnce(&scraper::Html) -> T + Send + 'static,
) -> T {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || read(&scraper::Html::parse_document(&html)))
        .await
        .expect("reading a parsed document panicked")
}
