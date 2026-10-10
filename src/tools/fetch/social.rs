//! Social-platform side-door fetch.
//!
//! Some platforms serve a structured representation of an item through a
//! dedicated endpoint — Reddit appends `.json`, oEmbed providers expose an
//! `/oembed` API — instead of the rendered HTML page. [`fetch_social`] routes a
//! classified URL to its endpoint and acquires the raw bytes; the
//! [`extract`](crate::tools::extract) tool parses them.
//!
//! It uses [`fetch_bytes`](super::fetch_bytes), NOT
//! [`fetch_strategy`](super::fetch_strategy): the strategy path validates for
//! HTML and rejects any body without `<html>`/`<!doctype>` markers (and anything
//! under 500 bytes), which would throw away every JSON response.

use crate::types::{CanonicalUrl, ContentSource, SocialKind, SocialParts, SocialPlatform};

use super::fetch_bytes;

/// Fetch a social item's structured representation through its dedicated door,
/// tagged with the [`ContentSource`] that produced it so the matching payload
/// extractor can parse it. Owns the routing knowledge: `.json` works on any
/// Reddit URL; oEmbed covers individual TikTok/YouTube/Pinterest/Vimeo content
/// (not profiles, boards, feeds, or opaque shortlinks with `kind: None`); every
/// other platform (Instagram/Facebook/X/LinkedIn) has no public door. `None`
/// when the URL has no door or lacks the ids to address one — returned before
/// any network — or the door fails (logged at debug); the caller falls back to
/// fetching the page.
pub async fn fetch_social(
    social: &SocialParts,
    url: &CanonicalUrl,
) -> Option<(ContentSource, Vec<u8>)> {
    let (source, request) = match social.platform {
        SocialPlatform::Reddit => (ContentSource::RedditJson, reddit_json_url(url.as_str())),
        SocialPlatform::TikTok
        | SocialPlatform::YouTube
        | SocialPlatform::Pinterest
        | SocialPlatform::Vimeo
            if matches!(
                social.kind,
                Some(SocialKind::Video | SocialKind::Post | SocialKind::Photo)
            ) =>
        {
            (ContentSource::Oembed, oembed_request_url(social)?)
        }
        _ => return None,
    };
    match fetch_bytes(&request, None).await {
        Ok(bytes) => Some((source, bytes)),
        Err(e) => {
            log::debug!("social door failed {}: {}", url.as_str(), e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

/// `{url}.json` — Reddit serves structured JSON for any URL this way.
pub(super) fn reddit_json_url(url: &str) -> String {
    let base = url.split('?').next().unwrap_or(url);
    format!("{}.json", base.trim_end_matches('/'))
}

/// A platform's oEmbed request URL, or `None` if it has no unauthenticated
/// oEmbed endpoint (Instagram/Facebook deprecated theirs behind app tokens) or
/// the URL lacks the ids needed to address the content.
pub(super) fn oembed_request_url(social: &SocialParts) -> Option<String> {
    let endpoint = match social.platform {
        SocialPlatform::TikTok => "https://www.tiktok.com/oembed",
        SocialPlatform::YouTube => "https://www.youtube.com/oembed",
        SocialPlatform::Pinterest => "https://www.pinterest.com/oembed.json",
        SocialPlatform::Vimeo => "https://vimeo.com/api/oembed.json",
        _ => return None,
    };
    let target = oembed_target_url(social)?;
    Some(format!(
        "{}?url={}&format=json",
        endpoint,
        urlencoding::encode(&target)
    ))
}

/// Rebuild the provider's expected canonical URL from the classified fields,
/// independent of how the shared URL was formatted. Required because `canonical`
/// strips `www.` for dedup, but some oEmbed endpoints are picky about the exact
/// host — notably **TikTok 400s without `www.`**.
fn oembed_target_url(social: &SocialParts) -> Option<String> {
    let id = social.id.as_deref();
    match social.platform {
        SocialPlatform::TikTok => {
            let handle = social.handle.as_deref()?;
            let segment = if social.kind == Some(SocialKind::Photo) {
                "photo"
            } else {
                "video"
            };
            Some(format!(
                "https://www.tiktok.com/{}/{}/{}",
                handle, segment, id?
            ))
        }
        SocialPlatform::YouTube => Some(format!("https://www.youtube.com/watch?v={}", id?)),
        SocialPlatform::Pinterest => Some(format!("https://www.pinterest.com/pin/{}/", id?)),
        SocialPlatform::Vimeo => Some(format!("https://vimeo.com/{}", id?)),
        _ => None,
    }
}
