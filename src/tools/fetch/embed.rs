//! Embed fetch.
//!
//! Some platforms serve a structured representation of an item through a
//! dedicated endpoint — Reddit appends `.json`, oEmbed providers expose an
//! `/oembed` API — beside the rendered HTML page. [`fetch_embed`] fetches it;
//! `scrape_embed` reads it.
//!
//! It uses [`fetch_bytes`](super::fetch_bytes), not
//! [`fetch_strategy`](super::fetch_strategy), which validates for HTML and
//! would reject every JSON response.

use crate::types::{Embed, SocialKind, SocialParts, SocialPlatform};
use crate::utils::social_parts;

use super::fetch_bytes;
use super::utils::check_domain_filter;

/// Fetch the embed of the item at `url`: Reddit's `.json` for a post or
/// comment, oEmbed for an individual TikTok/YouTube/Pinterest/Vimeo item.
/// `None` — before any network — for every URL with no embed: a non-social
/// URL, a profile, subreddit, board, or feed (whose payload would describe one
/// of its items), an opaque shortlink, Instagram/Facebook/X/LinkedIn, which
/// serve none without a token, and a URL the domain filter rejects. Also `None`
/// when the request fails (logged at debug).
pub async fn fetch_embed(url: &str) -> Option<Embed> {
    // The embed is served by another host (`youtu.be` → `www.youtube.com`), so
    // the item's own URL has to pass the filter too.
    check_domain_filter(url).ok()?;
    let (request, embed) = embed_request(url)?;
    match fetch_bytes(&request, None).await {
        Ok(bytes) => Some(embed(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) => {
            log::debug!("embed fetch failed {}: {}", url, e);
            None
        }
    }
}

/// Wraps a response body as the [`Embed`] its endpoint serves.
type EmbedOf = fn(String) -> Embed;

/// The request that serves `url`'s embed, and the [`Embed`] its response is.
pub(super) fn embed_request(url: &str) -> Option<(String, EmbedOf)> {
    let social = social_parts(url)?;
    match social.platform {
        SocialPlatform::Reddit
            if matches!(social.kind, Some(SocialKind::Post | SocialKind::Comment)) =>
        {
            Some((reddit_json_url(url), Embed::RedditJson))
        }
        SocialPlatform::TikTok
        | SocialPlatform::YouTube
        | SocialPlatform::Pinterest
        | SocialPlatform::Vimeo
            if matches!(
                social.kind,
                Some(SocialKind::Video | SocialKind::Post | SocialKind::Photo)
            ) =>
        {
            Some((oembed_request_url(&social)?, Embed::Oembed))
        }
        _ => None,
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
/// independent of how the given URL was written, because some oEmbed endpoints
/// are picky about the exact host — notably **TikTok 400s without `www.`**.
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
