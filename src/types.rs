//! Shared Types

mod canonical_url;

pub use canonical_url::CanonicalUrl;

use crate::errors::QrawlError;
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

/// A domain name in the form hosts compare in: lowercased and IDNA-encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDomain(String);

impl CanonicalDomain {
    /// `domain` as a [`CanonicalDomain`], or [`QrawlError::InvalidDomain`]
    /// when it isn't a domain name.
    pub fn new(domain: &str) -> Result<CanonicalDomain, QrawlError> {
        let invalid = || QrawlError::InvalidDomain(domain.to_string());
        let encoded = idna::domain_to_ascii(domain).map_err(|_error| invalid())?;
        let is_label = |label: &str| {
            !label.is_empty()
                && label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        };
        if !encoded.split('.').all(is_label) {
            return Err(invalid());
        }
        // A domain is written the way a URL's host comes out, so that it can
        // match one: `www.reddit.com` and `m.facebook.com` never do.
        let is_a_canonical_host = CanonicalUrl::new(&format!("https://{encoded}"))
            .is_some_and(|url| url.host() == encoded);
        if !is_a_canonical_host {
            return Err(invalid());
        }
        Ok(CanonicalDomain(encoded))
    }

    /// `domain` as a [`CanonicalDomain`], unchecked: for the crate's own
    /// table of social domains, which [`CanonicalDomain::new`] reads.
    pub(crate) fn new_unchecked(domain: &str) -> CanonicalDomain {
        CanonicalDomain(domain.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How every fetch inside a [`Context::scope`] behaves.
#[derive(Debug, Clone)]
pub struct Context {
    pub fetch_strategy: FetchStrategy,
    /// Max pages in flight (each may also fetch its embed). Nonzero, because
    /// a crawl with no slots never finishes.
    pub concurrency: NonZeroUsize,
    /// Fetch only these domains, redirects included. Empty = allow all.
    pub allow_domains: Vec<CanonicalDomain>,
    /// Never request these domains, redirects included. Empty = block none.
    pub block_domains: Vec<CanonicalDomain>,
}

impl Context {
    fn with_strategy(fetch_strategy: FetchStrategy) -> Self {
        Self {
            fetch_strategy,
            concurrency: DEFAULT_CONCURRENCY,
            allow_domains: Vec::new(),
            block_domains: Vec::new(),
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

    pub fn with_allow_domains(mut self, domains: Vec<CanonicalDomain>) -> Self {
        self.allow_domains = domains;
        self
    }

    pub fn with_block_domains(mut self, domains: Vec<CanonicalDomain>) -> Self {
        self.block_domains = domains;
        self
    }

    /// Run `future` with this context: every fetch it makes uses this
    /// strategy and these domain lists. A fetch outside any scope uses the
    /// defaults — including one in a task `future` spawns, because the context
    /// is task-local.
    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        CONTEXT.scope(Arc::new(self), future).await
    }
}

tokio::task_local! {
    static CONTEXT: Arc<Context>;
}

/// Read a field of the enclosing [`Context`], or `None` outside a
/// [`Context::scope`].
pub(crate) fn with_context<T>(read: impl FnOnce(&Context) -> T) -> Option<T> {
    CONTEXT.try_with(|context| read(context)).ok()
}

pub(crate) fn get_fetch_strategy() -> FetchStrategy {
    with_context(|context| context.fetch_strategy).unwrap_or(FetchStrategy::Auto)
}

#[doc(hidden)]
pub fn get_concurrency() -> NonZeroUsize {
    with_context(|context| context.concurrency).unwrap_or(DEFAULT_CONCURRENCY)
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
    /// Reddit's `.json` representation of a post, or of the comment
    /// `comment_id` under it.
    RedditJson {
        body: String,
        comment_id: Option<String>,
    },
    /// A provider's oEmbed JSON.
    Oembed(String),
}
