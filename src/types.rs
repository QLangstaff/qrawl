//! Shared Types

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;

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
    /// Fetch only these domains, redirects included. Empty = allow all.
    pub allow_domains: Vec<String>,
    /// Never request these domains, redirects included. Empty = block none.
    pub block_domains: Vec<String>,
    /// `qrawl_many_urls` crawls only these URLs. Empty = allow all.
    pub allow_urls: Vec<String>,
    /// `qrawl_many_urls` skips these URLs. Empty = block none.
    pub block_urls: Vec<String>,
    /// `qrawl_many_urls` returns only pages with one of these schema.org types. Empty = include all.
    pub include_schemas: Vec<String>,
    /// `qrawl_many_urls` drops pages with one of these schema.org types. Empty = exclude none.
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
    /// shares one fetch cache, so the same URL string fetched again after its
    /// first fetch completed costs no request. A fetch outside any scope uses the defaults
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
    /// Fetched HTML by the URL fetched, for the crawl [`Context::scope`] runs.
    static FETCH_CACHE: Arc<DashMap<String, String>>;
}

pub(crate) fn fetch_cache_get(url: &str) -> Option<String> {
    FETCH_CACHE
        .try_with(|cache| cache.get(url).map(|html| html.clone()))
        .ok()
        .flatten()
}

pub(crate) fn fetch_cache_put(url: &str, html: &str) {
    let _ = FETCH_CACHE.try_with(|cache| {
        cache.insert(url.to_string(), html.to_string());
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

/// The social facts of a URL — its platform, item kind, and identifiers —
/// extracted by `extract_social_parts`.
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
    /// `u/user`, a YouTube channel).
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
    /// channel feed.
    Collection,
}

/// A platform's structured representation of an item, served beside its
/// HTML page — fetched by `fetch_embed`, read by `scrape_embed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Embed {
    /// Reddit's `.json` representation of a post or comment.
    RedditJson(String),
    /// A provider's oEmbed JSON.
    Oembed(String),
}
