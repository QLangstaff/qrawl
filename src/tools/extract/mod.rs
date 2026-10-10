//! Extract Tools

mod tests;
mod utils;

use crate::tools::normalize::normalize_social_urls;
use crate::tools::scrape::scrape_from_doc;
use crate::types::{
    CanonicalUrl, ContentSource, Html, Jsonld, Metadata, PageContent, SocialParts, SocialPayload,
};
pub use utils::{extract_social_parts, find_metadata_value};

/// Extract a social door's payload into its [`PageContent`]. `None` when the
/// payload doesn't parse or fills no content field
/// ([`PageContent::has_content`]), so the caller falls back to the page path.
/// The page extras stay empty: a door hit never fetched the page itself.
pub fn extract_social_content(
    url: &CanonicalUrl,
    social: &SocialParts,
    payload: &SocialPayload,
) -> Option<PageContent> {
    let page = match payload {
        SocialPayload::RedditJson(body) => utils::extract_reddit(url, body),
        SocialPayload::Oembed(body) => utils::extract_oembed(url, body),
    }?;
    page.has_content().then(|| page.with_social(Some(social)))
}

/// Extract a fetched page's full [`PageContent`] — the page-path sibling of
/// [`extract_social_content`]. One parse yields the body, `<meta>` tags, the
/// merged schema.org view, and the social links on the page; the content
/// fields derive from the Open Graph metadata ([`extract_open_graph`]). `body`
/// is `None` when the page carried none.
pub async fn extract_page_content(
    url: &CanonicalUrl,
    social: Option<&SocialParts>,
    html: &Html,
) -> PageContent {
    let markup = html.to_string();
    let ((body, metadata, structured), socials) = tokio::task::spawn_blocking(move || {
        let doc = scraper::Html::parse_document(&markup);
        (scrape_from_doc(&doc), utils::extract_social_links(&doc))
    })
    .await
    .expect("extract_page_content: spawn_blocking failed");
    let open_graph = extract_open_graph(url, &metadata).with_social(social);
    PageContent {
        source: ContentSource::Html,
        structured,
        metadata,
        socials: normalize_social_urls(&socials),
        body: (!body.as_str().is_empty()).then_some(body),
        ..open_graph
    }
}

/// Extract schema.org `@type` values from JSON-LD, each once, in first-seen
/// order.
pub fn extract_schema_types(jsonld: &Jsonld) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for value in jsonld {
        let declared = match value.get("@type") {
            Some(serde_json::Value::String(ty)) => vec![ty.as_str()],
            Some(serde_json::Value::Array(tys)) => {
                tys.iter().filter_map(serde_json::Value::as_str).collect()
            }
            _ => Vec::new(),
        };
        for ty in declared {
            if !types.iter().any(|seen| seen == ty) {
                types.push(ty.to_string());
            }
        }
    }
    types
}

/// Extract a [`PageContent`] from the Open Graph / Twitter Card metadata of
/// the page at `url` — title, body text, author, hero image, and playable
/// video, tagged [`ContentSource::OpenGraph`]. The catch-all payload
/// extractor: every page exposes `<meta>`, so this always returns a result
/// (possibly all-empty — the caller filters with
/// [`PageContent::has_content`]).
pub fn extract_open_graph(url: &CanonicalUrl, metadata: &Metadata) -> PageContent {
    PageContent {
        title: utils::find_metadata_value(metadata, &["title", "og:title", "twitter:title"]),
        text: utils::find_metadata_value(
            metadata,
            &["description", "og:description", "twitter:description"],
        ),
        author: utils::find_metadata_value(metadata, &["author", "article:author", "og:site_name"]),
        image_url: utils::find_metadata_value(
            metadata,
            &["og:image", "twitter:image", "og:image:secure_url"],
        ),
        video_url: utils::find_metadata_value(
            metadata,
            &[
                "og:video:secure_url",
                "og:video:url",
                "og:video",
                "twitter:player:stream",
            ],
        ),
        ..PageContent::new(url.clone(), ContentSource::OpenGraph)
    }
}

/// Extract email addresses from HTML.
pub async fn extract_emails(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::extract_email_elements(&html))
        .await
        .expect("extract_emails: spawn_blocking failed")
}

/// Extract phone numbers from HTML.
pub async fn extract_phones(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::extract_phone_elements(&html))
        .await
        .expect("extract_phones: spawn_blocking failed")
}

/// Extract social profile/content links from a page's HTML.
///
/// Scans `<a href>` links, extracts the social parts of each with
/// [`extract_social_parts`], and returns the canonical URLs that resolve to a
/// known platform with a recognized content kind — dropping relative links,
/// `mailto:`, share-button intents, and opaque short links. Raw output may
/// contain duplicates, so pass it through [`normalize_social_urls`].
pub async fn extract_social_urls(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        utils::extract_social_links(&scraper::Html::parse_document(&html))
    })
    .await
    .expect("extract_social_urls: spawn_blocking failed")
}
