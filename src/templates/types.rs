use serde::Serialize;

use crate::tools::normalize::CanonicalUrl;
use crate::types::{Html, Jsonld, Metadata, SocialKind, SocialPlatform};

/// Everything qrawl could pull from one URL — the one shape
/// [`qrawl_any_url`](super::qrawl_any_url) and
/// [`qrawl_many_urls`](super::qrawl_many_urls) return for every URL, wherever
/// it's from. Domain-agnostic: it carries generic web signals and leaves
/// domain mapping — turning a schema.org `Recipe` or a video caption into the
/// caller's recipe type, say — to the caller.
///
/// Every field is filled by the same steps for every URL: from the page, plus
/// the platform's embed when the URL has one. When only one of the two could
/// be fetched, the fields the other would fill are empty.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageContent {
    /// The page's identity: one value for every address of the same page.
    pub url: CanonicalUrl,
    /// The address that served the page — after redirects — to fetch it again
    /// or resolve its relative links against. The address given, when only the
    /// embed could be fetched.
    pub address: String,
    /// The social platform, or `None` for a non-social URL.
    pub platform: Option<SocialPlatform>,
    /// The kind of social item, or `None` for a non-social URL or an opaque
    /// social shortlink.
    pub kind: Option<SocialKind>,
    pub title: Option<String>,
    /// Body / caption / description.
    pub text: Option<String>,
    /// The declared author, else the social handle in the URL.
    pub author: Option<String>,
    /// Hero image or thumbnail.
    pub image_url: Option<String>,
    /// A video: a file, or an embeddable player.
    pub video_url: Option<String>,
    /// schema.org entities: JSON-LD, Microdata, RDFa, and Microformats2,
    /// merged (`scrape_jsonld`).
    pub structured: Jsonld,
    /// Metadata pairs: the embed's (`scrape_embed`, keyed `oembed:` /
    /// `reddit:`), then the page's `<meta>` tags (`scrape_metadata`). The
    /// embed's keys are present exactly when the embed was fetched.
    pub metadata: Metadata,
    /// Canonical URLs of the social profiles and items the page links to.
    pub socials: Vec<String>,
    /// The page `<body>`, or `None` when only the embed could be fetched.
    /// Render it with `transform_markdown` for LLM-ready text.
    pub body: Option<Html>,
}

/// Upper bound on crawl depth — a guardrail against a runaway expansion.
pub const MAX_DEPTH: usize = 3;

/// What [`qrawl_many_urls`](super::qrawl_many_urls) crawls beyond the URLs it
/// is given, and which pages it returns. The default crawls exactly the given
/// URLs and returns every page.
#[derive(Debug, Clone, Default)]
pub struct CrawlOptions {
    pub(super) depth: usize,
    pub(super) limit: Option<usize>,
    pub(super) allow_urls: Vec<String>,
    pub(super) block_urls: Vec<String>,
    pub(super) include_schemas: Vec<String>,
    pub(super) exclude_schemas: Vec<String>,
}

impl CrawlOptions {
    /// Follow each page's children `depth` levels deep, at most [`MAX_DEPTH`].
    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth.min(MAX_DEPTH);
        self
    }

    /// Stop once `limit` pages are returned.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Crawl only these URLs. Empty = allow all.
    pub fn with_allow_urls(mut self, urls: Vec<String>) -> Self {
        self.allow_urls = urls;
        self
    }

    /// Skip these URLs.
    pub fn with_block_urls(mut self, urls: Vec<String>) -> Self {
        self.block_urls = urls;
        self
    }

    /// Return only pages with one of these schema.org types. Empty = any.
    pub fn with_include_schemas(mut self, schemas: Vec<String>) -> Self {
        self.include_schemas = schemas;
        self
    }

    /// Don't return pages with one of these schema.org types.
    pub fn with_exclude_schemas(mut self, schemas: Vec<String>) -> Self {
        self.exclude_schemas = schemas;
        self
    }
}
