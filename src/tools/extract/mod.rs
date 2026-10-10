//! Extract Tools

mod tests;
mod types;
mod utils;

use crate::types::{Html, Jsonld, Metadata, SocialParts};
use crate::utils::{schema_types, social_parts};
pub use types::Preview;
pub use utils::find_metadata_value;

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
            "oembed:title",
            "reddit:title",
            "title",
            "og:title",
            "twitter:title",
        ]),
        text: find(&[
            "reddit:text",
            "description",
            "og:description",
            "twitter:description",
        ]),
        author: find(&[
            "oembed:author_name",
            "reddit:author",
            "author",
            "article:author",
        ]),
        image_url: find(&[
            "reddit:image",
            "oembed:thumbnail_url",
            "og:image",
            "twitter:image",
            "og:image:secure_url",
        ]),
        video_url: find(&[
            "reddit:video",
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
