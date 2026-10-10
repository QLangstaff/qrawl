//! Shared Types

use dashmap::DashMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::hosts::host_matches;

/// Fetch strategy for pipeline `fetch_*` steps.
///
/// - `Auto` (default): Minimal → Windows → iOS fetch strategy cascade.
/// - `Fast`: Minimal fetch strategy only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStrategy {
    Auto,
    Fast,
}

/// Default concurrency.
pub const DEFAULT_CONCURRENCY: NonZeroUsize = match NonZeroUsize::new(1000) {
    Some(concurrency) => concurrency,
    None => panic!("default concurrency is nonzero"),
};

/// Context to chain tools
#[derive(Debug, Clone)]
pub struct Context {
    pub fetch_strategy: FetchStrategy,
    /// Max fetches in flight. Nonzero, because a crawl with no slots never
    /// finishes.
    pub concurrency: NonZeroUsize,
    pub depth: usize,
    /// Max results to return. `None` = unbounded.
    pub limit: Option<usize>,
    /// Allow domains pre-fetch. Empty = allow all.
    pub allow_domains: Vec<String>,
    /// Block domains pre-fetch. Empty = block none.
    pub block_domains: Vec<String>,
    /// Allow URLs pre-fetch. Empty = allow all.
    pub allow_urls: Vec<String>,
    /// Block URLs pre-fetch. Empty = block none.
    pub block_urls: Vec<String>,
    /// Include schema.org types post-fetch. Empty = include all.
    pub include_schemas: Vec<String>,
    /// Exclude schema.org types post-fetch. Empty = exclude none.
    pub exclude_schemas: Vec<String>,
}

impl Context {
    /// Base context: all defaults, `fetch_strategy` chosen by the caller.
    fn with_strategy(fetch_strategy: FetchStrategy) -> Self {
        Self {
            fetch_strategy,
            concurrency: DEFAULT_CONCURRENCY,
            depth: 0,
            limit: None,
            allow_domains: Vec::new(),
            block_domains: Vec::new(),
            allow_urls: Vec::new(),
            block_urls: Vec::new(),
            include_schemas: Vec::new(),
            exclude_schemas: Vec::new(),
        }
    }

    /// Context with the Minimal→Windows→iOS fetch strategy cascade.
    pub fn auto() -> Self {
        Self::with_strategy(FetchStrategy::Auto)
    }

    /// Context with the Minimal fetch strategy only.
    pub fn fast() -> Self {
        Self::with_strategy(FetchStrategy::Fast)
    }

    pub fn with_concurrency(mut self, concurrency: NonZeroUsize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_limit(mut self, limit: impl Into<Option<usize>>) -> Self {
        self.limit = limit.into();
        self
    }

    pub fn with_allow_domains(mut self, domains: Vec<String>) -> Self {
        self.allow_domains = domains;
        self
    }

    pub fn with_block_domains(mut self, domains: Vec<String>) -> Self {
        self.block_domains = domains;
        self
    }

    pub fn with_allow_urls(mut self, urls: Vec<String>) -> Self {
        self.allow_urls = urls;
        self
    }

    pub fn with_block_urls(mut self, urls: Vec<String>) -> Self {
        self.block_urls = urls;
        self
    }

    pub fn with_include_schemas(mut self, schemas: Vec<String>) -> Self {
        self.include_schemas = schemas;
        self
    }

    pub fn with_exclude_schemas(mut self, schemas: Vec<String>) -> Self {
        self.exclude_schemas = schemas;
        self
    }

    /// Run `future` as one crawl: every fetch inside it reads this context and
    /// shares one fetch cache, so a URL fetched again after its first fetch
    /// completed costs no request. A fetch outside any scope uses the defaults
    /// and no cache.
    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        CTX.scope(
            Arc::new(self),
            FETCH_CACHE.scope(Arc::new(DashMap::new()), future),
        )
        .await
    }
}

tokio::task_local! {
    static CTX: Arc<Context>;
    /// Fetched HTML by canonical URL, for the crawl [`Context::scope`] runs.
    static FETCH_CACHE: Arc<DashMap<String, String>>;
}

// Keys are `CanonicalUrl`'s form, so `m.` and tracking-param variants of one
// URL share an entry.
pub(crate) fn fetch_cache_get(url: &str) -> Option<String> {
    let key = crate::tools::normalize::normalize_social(url);
    FETCH_CACHE
        .try_with(|cache| cache.get(&key).map(|html| html.clone()))
        .ok()
        .flatten()
}

pub(crate) fn fetch_cache_put(url: &str, html: &str) {
    let key = crate::tools::normalize::normalize_social(url);
    let _ = FETCH_CACHE.try_with(|cache| {
        cache.insert(key, html.to_string());
    });
}

/// Read a field of the enclosing [`Context`], or `None` outside a
/// [`Context::scope`].
pub(crate) fn with_context<T>(read: impl FnOnce(&Context) -> T) -> Option<T> {
    CTX.try_with(|ctx| read(ctx)).ok()
}

pub fn get_fetch_strategy() -> FetchStrategy {
    with_context(|ctx| ctx.fetch_strategy).unwrap_or(FetchStrategy::Auto)
}

pub fn get_concurrency() -> NonZeroUsize {
    with_context(|ctx| ctx.concurrency).unwrap_or(DEFAULT_CONCURRENCY)
}

/// Raw HTML content — the substrate every page-scraping tool consumes.
///
/// A newtype over `String` (not the parsed `scraper::Html`, which is `!Send` and
/// can't cross the async / `spawn_blocking` boundary): tools take `&Html`, call
/// `.as_str()`, and parse internally. Threading this — instead of bare `String`
/// — makes the tool graph legible: `fetch` yields `Html`, every scraper/extractor
/// consumes `&Html`, so the compiler rejects feeding body text or Markdown where
/// HTML is expected.
///
/// Construct via [`Html::new`] / `From<String>`. There is deliberately **no**
/// `From<&str>` in production — that free wrap would bypass the barrier; a
/// `#[cfg(test)]` impl gives tests the terse `.into()` form only. Serialization
/// is transparent (a bare string).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Html(String);

impl Html {
    /// Wrap fetched/loaded HTML. (No parsing — `scraper::Html` is `!Send`.)
    pub fn new(html: String) -> Self {
        Self(html)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for Html {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Html {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Html {
    fn from(html: String) -> Self {
        Self(html)
    }
}

/// Test-only terse construction (`"<html>".into()`); absent from production
/// builds so the type barrier holds everywhere it matters.
#[cfg(test)]
impl From<&str> for Html {
    fn from(html: &str) -> Self {
        Self(html.to_string())
    }
}

/// LLM-ready Markdown — produced by
/// [`transform_markdown`](crate::tools::transform::transform_markdown).
///
/// A *terminal* substrate: once HTML is rendered to Markdown, the
/// `scrape`/`extract`/`map` tools can no longer operate on it. Follows the same
/// string-newtype conventions as [`Html`] (construct via `new`; read via
/// `as_str` / `into_inner`; `AsRef<str>` + `Display`, no `Deref`; transparent
/// JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Markdown(String);

impl Markdown {
    pub fn new(markdown: String) -> Self {
        Markdown(markdown)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for Markdown {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// JSON-LD array of schema.org objects.
pub type Jsonld = Vec<Value>;

/// Metadata key-value pairs.
pub type Metadata = Vec<(String, String)>;

/// Microformats2 items, each in canonical mf2 shape:
/// `{"type": ["h-card"], "properties": {"name": ["…"], …}, "children": [...]}`.
///
/// Deliberately **not** [`Jsonld`]: mf2 is a distinct vocabulary (`h-card`/
/// `h-entry`, not schema.org), keyed on `type` (not `@type`) with array-valued
/// properties — so it is kept distinct from the native encodings. To consume
/// it as schema.org (normalized and merged with the native encodings), use
/// `scrape_jsonld`, which folds mf2 in.
pub(crate) type Microformats = Vec<Value>;

/// A URL in qrawl's canonical form — the social-aware canonicalization
/// ([`normalize_social`](crate::tools::normalize::normalize_social)) that
/// [`PageContent`], the URL templates, and the fetch cache key on.
///
/// Construct via [`CanonicalUrl::new`] (or `From<&str>` / `From<String>`); the
/// inner string is private so the invariant can't be bypassed. Serialization is
/// transparent (a bare string); **deserialization re-canonicalizes**, so a
/// non-canonical string from a DB / API payload / hand-written JSON can't sneak
/// past the invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalUrl(String);

impl CanonicalUrl {
    /// Canonicalize and wrap. Idempotent if the input is already canonical.
    pub fn new(raw: &str) -> Self {
        Self(crate::tools::normalize::normalize_social(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for CanonicalUrl {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CanonicalUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CanonicalUrl {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for CanonicalUrl {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl<'de> Deserialize<'de> for CanonicalUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(&String::deserialize(deserializer)?))
    }
}

// ---------------------------------------------------------------------------
// Social vocabulary
//
// Data shapes for URL classification. `classify_is_social_url` /
// `extract_social_parts` answer "is this social?" and extract a social URL's
// `SocialParts`. The verbs live in their tools; this is just the vocabulary,
// alongside `Jsonld`/`Metadata`/`Context`.
// ---------------------------------------------------------------------------

/// The social platform a URL belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialPlatform {
    Reddit,
    TikTok,
    Instagram,
    Facebook,
    Pinterest,
    YouTube,
    Twitter,
    LinkedIn,
    Vimeo,
}

impl SocialPlatform {
    /// Recognize the social platform a host belongs to, or `None` for a
    /// non-social host. Subdomain-aware (`old.reddit.com`, `m.facebook.com`,
    /// `vm.tiktok.com`) and handles the alt short-domains (`youtu.be`, `fb.me`,
    /// …).
    pub fn from_host(host: &str) -> Option<SocialPlatform> {
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
}

/// The social facts of a URL — its platform, item kind, and identifiers —
/// extracted by [`extract_social_parts`](crate::tools::extract::extract_social_parts).
/// The URL string itself is a [`CanonicalUrl`], threaded alongside this by the
/// pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialParts {
    pub platform: SocialPlatform,
    /// The item kind, or `None` when the platform is recognized but the URL
    /// shape is opaque (e.g. an unexpanded short link).
    pub kind: Option<SocialKind>,
    /// Post / video / pin id, where the URL carries one.
    pub id: Option<String>,
    /// Account identifier: `@user` (TikTok/IG/X/YouTube), `r/sub` or `u/user`
    /// (Reddit), or a profile/board/company slug.
    pub handle: Option<String>,
}

/// The kind of social item a URL points at — the classifier paired with
/// [`SocialPlatform`] in [`SocialParts`].
///
/// Platform-independent *within* the social domain (a `Video` is a `Video`
/// whether it's TikTok, YouTube, or Vimeo) and aligned with schema.org /
/// ActivityStreams vocabulary. A caller that wants a branded label (e.g.
/// "Subreddit") derives it from `(platform, kind)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialKind {
    /// An entity page — a person, account, or channel (TikTok `@user`, Reddit
    /// `u/user`, a YouTube channel, a web author page).
    Profile,
    /// A single text or link post.
    Post,
    /// A single video. (Instagram reels classify here too.)
    Video,
    /// A single image.
    Photo,
    /// A reply within a thread.
    Comment,
    /// A collection / listing of items — a subreddit, a Pinterest board, a
    /// channel feed, a web index page (schema.org `CollectionPage`,
    /// ActivityStreams `Collection`).
    Collection,
}

/// A social door's response body, tagged with the door that served it —
/// fetched by [`fetch_social`](crate::tools::fetch::fetch_social), parsed by
/// [`extract_social_content`](crate::tools::extract::extract_social_content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocialPayload {
    /// Reddit's `.json` representation of a URL.
    RedditJson(String),
    /// A provider's oEmbed JSON.
    Oembed(String),
}

/// Provenance of the content fields in a [`PageContent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ContentSource {
    RedditJson,
    Oembed,
    OpenGraph,
    /// The page itself was fetched and scraped (structured data + body) — the
    /// spine of [`qrawl_any_url`](crate::templates::qrawl_any_url) for any
    /// non-social URL.
    Html,
    /// No side door produced anything — fields come only from the share payload.
    SharePayloadOnly,
}

/// Everything qrawl could pull from a single URL — qrawl's one content shape,
/// produced by [`qrawl_any_url`](crate::templates::qrawl_any_url) /
/// [`qrawl_many_urls`](crate::templates::qrawl_many_urls) /
/// [`qrawl_share`](crate::templates::qrawl_share). Domain-agnostic: it carries
/// generic web signals and leaves domain mapping — turning a schema.org
/// `Recipe` into the caller's recipe type, say — to the caller.
///
/// [`source`](PageContent::source) tells you which path produced it: a social
/// side door fills only the content fields (`title` … `video_url`), leaving
/// the page extras (`structured`/`metadata`/`socials`/`body`) empty; the page
/// path ([`ContentSource::Html`]) fills both.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageContent {
    /// Canonical URL.
    pub url: CanonicalUrl,
    /// The social platform, or `None` for a non-social (web) URL.
    pub platform: Option<SocialPlatform>,
    /// The item kind, or `None` for a web URL or an opaque social shortlink.
    pub kind: Option<SocialKind>,
    /// Which path produced the content.
    pub source: ContentSource,
    pub title: Option<String>,
    /// Body / caption / short description.
    pub text: Option<String>,
    pub author: Option<String>,
    /// Hero image / thumbnail (`og:image` or a side-door thumbnail).
    pub image_url: Option<String>,
    /// Playable video URL discovered by a side door (an oEmbed thumbnail is
    /// `image_url`, not this; this is e.g. a Reddit hosted video or `og:video`).
    pub video_url: Option<String>,
    /// schema.org entities — the merged `scrape_jsonld` view (JSON-LD +
    /// Microdata + RDFa + Microformats2, deduped). Empty on the social side-door
    /// paths.
    pub structured: Jsonld,
    /// Raw `<meta>` tags (Open Graph / Twitter Card / standard). Empty on the
    /// social side-door paths.
    pub metadata: Metadata,
    /// Social profile/content links found on the page
    /// ([`extract_social_urls`](crate::tools::extract::extract_social_urls),
    /// canonical + deduped). Empty on the social side-door paths (no page HTML
    /// to scan).
    pub socials: Vec<String>,
    /// The page `<body>` as [`Html`] (`scrape_body`). Transform to Markdown
    /// terminally (`transform_markdown`) when LLM-ready text is wanted. `None` on
    /// side-door paths and the lean bulk path.
    pub body: Option<Html>,
}

impl PageContent {
    /// An empty bundle for `url` tagged with the path that produced it — the
    /// seed every extractor fills via struct update syntax.
    pub fn new(url: CanonicalUrl, source: ContentSource) -> Self {
        Self {
            url,
            platform: None,
            kind: None,
            source,
            title: None,
            text: None,
            author: None,
            image_url: None,
            video_url: None,
            structured: Vec::new(),
            metadata: Vec::new(),
            socials: Vec::new(),
            body: None,
        }
    }

    /// True if at least one content field was extracted — used to decide
    /// whether a payload result is worth keeping or the caller falls through
    /// to the next path. `url` is always present, so it doesn't count.
    pub fn has_content(&self) -> bool {
        self.title.is_some()
            || self.text.is_some()
            || self.image_url.is_some()
            || self.video_url.is_some()
    }

    /// Thread a URL's social facts through: `platform`/`kind` come from the
    /// classification, and a missing `author` falls back to the classified
    /// handle.
    pub fn with_social(mut self, social: Option<&SocialParts>) -> Self {
        self.platform = social.map(|s| s.platform);
        self.kind = social.and_then(|s| s.kind);
        if self.author.is_none() {
            self.author = social.and_then(|s| s.handle.clone());
        }
        self
    }
}
