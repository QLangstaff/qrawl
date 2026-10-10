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
    pub url: CanonicalUrl,
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
    /// Playable video.
    pub video_url: Option<String>,
    /// schema.org entities: JSON-LD, Microdata, RDFa, and Microformats2,
    /// merged (`scrape_jsonld`).
    pub structured: Jsonld,
    /// Metadata pairs: the embed's (`scrape_embed`), then the page's `<meta>`
    /// tags (`scrape_metadata`).
    pub metadata: Metadata,
    /// Canonical URLs of the social profiles and items the page links to.
    pub socials: Vec<String>,
    /// The page `<body>`, or `None` when only the embed could be fetched.
    /// Render it with `transform_markdown` for LLM-ready text.
    pub body: Option<Html>,
}
