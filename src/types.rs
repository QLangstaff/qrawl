//! Shared Types

use dashmap::DashMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Fetch strategy for pipeline `fetch_*` steps.
///
/// - `Auto` (default): Minimal → Windows → iOS fetch strategy cascade.
/// - `Fast`: Minimal fetch strategy only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStrategy {
    Auto,
    Fast,
}

/// Default fetch timeout.
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Default concurrency.
pub const DEFAULT_CONCURRENCY: usize = 1000;

/// Context to chain tools
#[derive(Debug, Clone)]
pub struct Context {
    pub fetch_strategy: FetchStrategy,
    pub fetch_timeout: Duration,
    pub concurrency: usize,
    pub depth: usize,
    pub limit: usize,
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
    /// Context with the Minimal→Windows→iOS fetch strategy cascade.
    pub fn auto() -> Self {
        Self {
            fetch_strategy: FetchStrategy::Auto,
            fetch_timeout: DEFAULT_FETCH_TIMEOUT,
            concurrency: DEFAULT_CONCURRENCY,
            depth: 0,
            limit: 0,
            allow_domains: Vec::new(),
            block_domains: Vec::new(),
            allow_urls: Vec::new(),
            block_urls: Vec::new(),
            include_schemas: Vec::new(),
            exclude_schemas: Vec::new(),
        }
    }

    /// Context with the Minimal fetch strategy only.
    pub fn fast() -> Self {
        Self {
            fetch_strategy: FetchStrategy::Fast,
            ..Self::auto()
        }
    }

    /// Override the per-request fetch timeout. Applied to each profile attempt
    /// via reqwest's `RequestBuilder::timeout`. Bulk workloads typically set
    /// this aggressively (e.g., 5s) to cap tail latency; single-URL workflows
    /// leave it at the default to give slow sites a chance.
    pub fn with_fetch_timeout(mut self, timeout: Duration) -> Self {
        self.fetch_timeout = timeout;
        self
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
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
}

tokio::task_local! {
    pub static CTX: Arc<Context>;
    /// Per-pipeline fetch cache (canonical URL -> HTML). Populated by fetch functions
    /// on success and consulted on subsequent calls within the same crawl scope.
    pub static FETCH_CACHE: Arc<DashMap<String, String>>;
}

pub fn fetch_cache_new() -> Arc<DashMap<String, String>> {
    Arc::new(DashMap::new())
}

// Keys are the one social-aware canonical form (`normalize_social`) — the same
// form `CanonicalUrl` uses — so a URL reached through the
// pipeline and one fetched directly hit the same entry, and `m.` / tracking-param
// variants of one social item don't split into separate fetches. Idempotent, so
// it's a no-op for callers that already canonicalize.
pub fn fetch_cache_get(url: &str) -> Option<String> {
    let key = crate::tools::normalize::normalize_social(url);
    FETCH_CACHE
        .try_with(|cache| cache.get(&key).map(|v| v.clone()))
        .ok()
        .flatten()
}

pub fn fetch_cache_put(url: &str, html: &str) {
    let key = crate::tools::normalize::normalize_social(url);
    let _ = FETCH_CACHE.try_with(|cache| {
        cache.insert(key, html.to_string());
    });
}

pub fn get_fetch_strategy() -> FetchStrategy {
    CTX.try_with(|ctx| ctx.fetch_strategy)
        .ok()
        .unwrap_or(FetchStrategy::Auto)
}

pub fn get_fetch_timeout() -> Duration {
    CTX.try_with(|ctx| ctx.fetch_timeout)
        .ok()
        .unwrap_or(DEFAULT_FETCH_TIMEOUT)
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
pub type Microformats = Vec<Value>;

/// A URL in qrawl's one canonical form — the social-aware canonicalization that
/// every pipeline, the classify/extract tools, and the fetch cache all key on.
///
/// Construct via [`CanonicalUrl::new`] (or `From<&str>` / `From<String>`); the
/// inner string is private so the invariant can't be bypassed. Serialization is
/// transparent (a bare string); **deserialization re-canonicalizes**, so a
/// non-canonical string from a DB / API payload / hand-written JSON can't sneak
/// past the invariant. (Re-canonicalizing canonical input is a no-op —
/// `normalize_social` is idempotent.)
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
