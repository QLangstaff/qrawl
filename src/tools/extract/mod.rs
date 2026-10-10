//! Extract Tools

mod tests;
mod types;
mod utils;

use crate::types::{Html, Jsonld, Metadata, SocialParts};
use crate::utils::embed_keys::*;
use crate::utils::{schema_types, social_parts, with_document};
pub use types::Preview;

/// Extract the schema.org types in JSON-LD, as short names (`Recipe`), each
/// once, in first-seen order.
pub fn extract_schema_types(jsonld: &Jsonld) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for schema_type in jsonld.iter().flat_map(schema_types) {
        if !types.contains(&schema_type) {
            types.push(schema_type);
        }
    }
    types
}

/// Extract a [`Preview`] from metadata: the first value present per field,
/// from an embed's keys (`scrape_embed`), then the standard `<meta>` names,
/// then Open Graph and Twitter Card. An embed describes the item itself, so
/// it wins over a page that may be a login wall or an app shell.
pub fn extract_preview(metadata: &Metadata) -> Preview {
    let find = |keys: &[&str]| utils::find_metadata_value(metadata, keys);
    Preview {
        title: find(&[
            OEMBED_TITLE,
            REDDIT_TITLE,
            "title",
            "og:title",
            "twitter:title",
        ]),
        text: find(&[
            REDDIT_TEXT,
            "description",
            "og:description",
            "twitter:description",
        ]),
        author: find(&[
            OEMBED_AUTHOR_NAME,
            REDDIT_AUTHOR,
            "author",
            "article:author",
        ]),
        image_url: find(&[
            REDDIT_IMAGE,
            OEMBED_THUMBNAIL_URL,
            "og:image",
            "twitter:image",
            "og:image:secure_url",
        ]),
        video_url: find(&[
            REDDIT_VIDEO,
            "og:video:secure_url",
            "og:video:url",
            "og:video",
            "twitter:player:stream",
        ]),
    }
}

/// Extract the [`SocialParts`] of a social URL — its platform, the kind of
/// item, and its identifiers — or `None` when the host isn't a recognized
/// platform or the input isn't a URL. Pure path-pattern matching, no network.
///
/// ```
/// use qrawl::tools::extract::extract_social_parts;
/// use qrawl::types::{SocialKind, SocialPlatform};
///
/// let s = extract_social_parts("https://tiktok.com/@user/video/12345").unwrap();
/// assert_eq!(s.platform, SocialPlatform::TikTok);
/// assert_eq!(s.kind, Some(SocialKind::Video));
/// assert_eq!(s.id.as_deref(), Some("12345"));
/// assert_eq!(s.handle.as_deref(), Some("@user"));
/// assert!(extract_social_parts("https://example.com/recipe").is_none());
/// ```
pub fn extract_social_parts(url: &str) -> Option<SocialParts> {
    social_parts(url)
}

/// Extract email addresses from HTML.
pub async fn extract_emails(html: &Html) -> Vec<String> {
    with_document(html, utils::extract_email_elements).await
}

/// Extract phone numbers from HTML.
pub async fn extract_phones(html: &Html) -> Vec<String> {
    with_document(html, utils::extract_phone_elements).await
}
