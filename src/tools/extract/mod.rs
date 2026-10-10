//! Extract Tools

mod tests;
mod utils;

use crate::tools::normalize::normalize_social_urls;
use crate::tools::scrape::scrape_all;
use crate::types::{
    CanonicalUrl, ContentSource, Html, Jsonld, Metadata, PageContent, SocialParts,
};
pub use utils::{extract_social_parts, find_metadata_value};

/// Extract a social door's payload into its [`PageContent`] — dispatches on
/// the [`ContentSource`] the fetch tagged the bytes with (Reddit `.json` or
/// oEmbed JSON) to the matching internal parser. `None` when the source has
/// no payload extractor or the payload fills no content field
/// ([`PageContent::has_content`]), so the caller falls back to the page path. The page extras stay empty: a door hit
/// never fetched the page itself.
pub fn extract_social_content(
    url: &CanonicalUrl,
    social: &SocialParts,
    source: ContentSource,
    payload: &str,
) -> Option<PageContent> {
    let result = match source {
        ContentSource::RedditJson => utils::extract_reddit(url, payload),
        ContentSource::Oembed => utils::extract_oembed(url, payload),
        _ => None,
    };
    result
        .filter(PageContent::has_content)
        .map(|page| page.with_social(Some(social)))
}

/// Extract a fetched page's full [`PageContent`] — the page-path sibling of
/// [`extract_social_content`]. One parse ([`scrape_all`]) yields the body,
/// `<meta>` tags, and the merged schema.org view; the social links on the page
/// ride along; the content fields derive from the Open Graph metadata
/// ([`extract_open_graph`]). `body` is `None` when the page carried none.
pub async fn extract_page_content(
    url: &CanonicalUrl,
    social: Option<&SocialParts>,
    html: &Html,
) -> PageContent {
    let (body, metadata, structured) = scrape_all(html).await;
    let body = (!body.as_str().is_empty()).then_some(body);
    let socials = normalize_social_urls(&extract_social_urls(html).await);
    let core = extract_open_graph(url, &metadata).with_social(social);
    PageContent {
        source: ContentSource::Html,
        structured,
        metadata,
        socials,
        body,
        ..core
    }
}

/// Extract schema.org `@type` values from JSON-LD.
pub fn extract_schema_types(jsonld: &Jsonld) -> Vec<String> {
    let mut types = Vec::new();

    for value in jsonld {
        if let Some(type_value) = value.get("@type") {
            match type_value {
                serde_json::Value::String(s) => utils::push_unique(&mut types, s.to_string()),
                serde_json::Value::Array(arr) => {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            utils::push_unique(&mut types, s.to_string());
                        }
                    }
                }
                _ => {}
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
/// `mailto:`, share-button intents, and opaque short links. The social-link analogue of [`extract_emails`]; raw output may
/// contain duplicates, so pass it through
/// [`normalize_social_urls`].
pub async fn extract_social_urls(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::extract_social_links(&html))
        .await
        .expect("extract_social_urls: spawn_blocking failed")
}

