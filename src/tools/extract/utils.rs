use once_cell::sync::Lazy;
use regex::Regex;
use scraper::Html;
use serde_json::Value;
use url::Url;

use crate::tools::normalize::normalize_social;
use crate::types::{
    CanonicalUrl, ContentSource, PageContent, SocialKind, SocialParts, SocialPlatform,
};
use crate::utils::{host_of_url, social_platform, LINK_SELECTOR};

// Lazy static regex patterns
static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    // More strict email pattern that excludes common false positives:
    // - Word boundaries (\b) ensure we match complete emails, not parts of concatenated text
    // - Local part: alphanumeric, dots, underscores, percent, plus, hyphens
    // - Domain: alphanumeric segments separated by dots or hyphens
    // - TLD: must be letters only (2-24 chars), excludes file extensions like .js, .css, .jpg
    Regex::new(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*\.[a-z]{2,24}\b").expect("valid regex")
});
static PHONE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").expect("valid regex")
});

/// Extract values from links with a specific href prefix (e.g., "mailto:", "tel:")
fn extract_with_prefix(doc: &Html, prefix: &str) -> Vec<String> {
    let mut results = Vec::new();
    for link in doc.select(&LINK_SELECTOR) {
        if let Some(href) = link.value().attr("href") {
            if let Some(value) = href.strip_prefix(prefix) {
                // Trim whitespace and remove query parameters
                let normalized = value.split('?').next().unwrap_or(value).trim();
                if !normalized.is_empty() {
                    results.push(normalized.to_string());
                }
            }
        }
    }
    results
}

/// Extract values from text content using a regex pattern
fn extract_with_regex(doc: &Html, regex: &Regex) -> Vec<String> {
    // Join text nodes with spaces to prevent adjacent elements from creating false patterns
    // e.g., <div>9631</div><a>contact@domain.com</a> becomes "9631 contact@domain.com"
    // instead of "9631contact@domain.com"
    let text = doc.root_element().text().collect::<Vec<_>>().join(" ");
    regex
        .captures_iter(&text)
        .filter_map(|cap| cap.get(0))
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Extract all email addresses from HTML document.
pub(super) fn extract_email_elements(html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(html);
    [
        extract_with_prefix(&doc, "mailto:"),
        extract_with_regex(&doc, &EMAIL_REGEX),
    ]
    .concat()
}

/// Extract all phone numbers from HTML document.
pub(super) fn extract_phone_elements(html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(html);
    [
        extract_with_prefix(&doc, "tel:"),
        extract_with_regex(&doc, &PHONE_REGEX),
    ]
    .concat()
}

/// Find the first non-empty value for any of the given keys in metadata pairs.
pub fn find_metadata_value(pairs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        for (k, v) in pairs {
            if k.eq_ignore_ascii_case(key) {
                let normalized = v.trim().to_string();
                if !normalized.is_empty() {
                    return Some(normalized);
                }
            }
        }
    }
    None
}

/// Scan `<a href>` links and return the canonical URLs of those that classify
/// to a known social platform with a recognized content kind. Drops relative
/// links, `mailto:`/opaque links (no host → classify error), non-social hosts,
/// and share-button intents / short links (a social URL with no resolved
/// `kind`). Raw — may contain duplicates; dedupe with
/// [`normalize_social_urls`](crate::tools::normalize::normalize_social_urls).
pub(super) fn extract_social_links(doc: &Html) -> Vec<String> {
    doc.select(&LINK_SELECTOR)
        .filter_map(|link| link.value().attr("href"))
        .map(str::trim)
        .filter(|href| !href.is_empty())
        .map(normalize_social)
        .filter(|url| extract_social_parts(url).is_some_and(|social| social.kind.is_some()))
        .collect()
}

// ---------------------------------------------------------------------------
// Social-URL extraction
//
// Extract a recognized social URL's structured parts (platform, content kind,
// id/handle). Pure, deterministic path-pattern matching. The URL-shaped `extract`:
// it pulls structure out of a URL the way the other `extract_*` fns pull structure
// out of HTML / JSON-LD.
// ---------------------------------------------------------------------------

/// Extract the [`SocialParts`] facts of a social URL — its platform, content
/// kind, and identifiers — or `None` if the host isn't a recognized social
/// platform (or the input isn't a valid URL). Pure, deterministic path-pattern
/// matching; assumes a canonical input (`normalize_social` first) — the match is
/// on host/path shape, so a non-canonical input still resolves, just less
/// reliably. Pair with
/// [`classify_is_social_url`](crate::tools::classify::classify_is_social_url)
/// for the cheap yes/no predicate.
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
pub fn extract_social_parts(input: &str) -> Option<SocialParts> {
    let url = Url::parse(input).ok()?;
    let host = host_of_url(&url)?;
    let platform = social_platform(&host)?;

    let (kind, id, handle) = if is_opaque_shortener(&host) {
        // Platform known, but the path is an opaque short code that needs a
        // network round-trip to expand (deferred to the side-door layer).
        (None, None, None)
    } else {
        // Owned segments, then a `&str` view for ergonomic slice patterns.
        let segments = path_segments(&url);
        let segs: Vec<&str> = segments.iter().map(String::as_str).collect();
        match platform {
            SocialPlatform::Reddit => extract_social_reddit_parts(&segs),
            SocialPlatform::TikTok => extract_social_tiktok_parts(&segs),
            SocialPlatform::Instagram => extract_social_instagram_parts(&segs),
            SocialPlatform::Facebook => extract_social_facebook_parts(&segs, &url),
            SocialPlatform::Pinterest => extract_social_pinterest_parts(&segs),
            SocialPlatform::YouTube => extract_social_youtube_parts(&host, &segs, &url),
            SocialPlatform::Twitter => extract_social_twitter_parts(&segs),
            SocialPlatform::LinkedIn => extract_social_linkedin_parts(&segs),
            SocialPlatform::Vimeo => extract_social_vimeo_parts(&segs),
        }
    };

    Some(SocialParts {
        platform,
        kind,
        id,
        handle,
    })
}

/// Non-empty path segments.
fn path_segments(url: &Url) -> Vec<String> {
    url.path_segments()
        .map(|segs| segs.filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Hosts whose path is an opaque short code — platform is known, kind is not.
/// (`youtu.be` is intentionally absent: its path *is* the video id.)
fn is_opaque_shortener(host: &str) -> bool {
    matches!(
        host,
        "redd.it"
            | "vm.tiktok.com"
            | "vt.tiktok.com"
            | "pin.it"
            | "fb.me"
            | "fb.watch"
            | "instagr.am"
            | "t.co"
            | "lnkd.in"
    )
}

// Each returns (kind, id, handle). Slice bindings are `&&str`; deref with `*`
// before `.to_string()`.
type Parts = (Option<SocialKind>, Option<String>, Option<String>);

fn extract_social_reddit_parts(segs: &[&str]) -> Parts {
    match segs {
        // /r/{sub}/comments/{post}/{slug}/{comment}
        ["r", sub, "comments", _post, _slug, comment, ..] => (
            Some(SocialKind::Comment),
            Some((*comment).to_string()),
            Some(format!("r/{}", sub)),
        ),
        // /r/{sub}/comments/{post}[/slug]
        ["r", sub, "comments", post, ..] => (
            Some(SocialKind::Post),
            Some((*post).to_string()),
            Some(format!("r/{}", sub)),
        ),
        // /comments/{post} (subreddit omitted)
        ["comments", post, ..] | ["gallery", post, ..] => {
            (Some(SocialKind::Post), Some((*post).to_string()), None)
        }
        // /r/{sub}/s/{code} — modern share link (opaque, needs expansion)
        ["r", sub, "s", ..] => (None, None, Some(format!("r/{}", sub))),
        ["r", sub] => (
            Some(SocialKind::Collection),
            None,
            Some(format!("r/{}", sub)),
        ),
        ["user", name, ..] | ["u", name, ..] => {
            (Some(SocialKind::Profile), None, Some(format!("u/{}", name)))
        }
        _ => (None, None, None),
    }
}

fn extract_social_tiktok_parts(segs: &[&str]) -> Parts {
    match segs {
        [user, "video", id, ..] if user.starts_with('@') => (
            Some(SocialKind::Video),
            Some((*id).to_string()),
            Some((*user).to_string()),
        ),
        [user, "photo", id, ..] if user.starts_with('@') => (
            Some(SocialKind::Photo),
            Some((*id).to_string()),
            Some((*user).to_string()),
        ),
        [user] if user.starts_with('@') => {
            (Some(SocialKind::Profile), None, Some((*user).to_string()))
        }
        _ => (None, None, None),
    }
}

fn extract_social_instagram_parts(segs: &[&str]) -> Parts {
    match segs {
        ["p", id, ..] => (Some(SocialKind::Post), Some((*id).to_string()), None),
        ["reel", id, ..] | ["reels", id, ..] | ["tv", id, ..] => {
            (Some(SocialKind::Video), Some((*id).to_string()), None)
        }
        [user] if !is_reserved_instagram(user) => {
            (Some(SocialKind::Profile), None, Some(format!("@{}", user)))
        }
        _ => (None, None, None),
    }
}

fn extract_social_facebook_parts(segs: &[&str], url: &Url) -> Parts {
    match segs {
        ["watch", ..] => (Some(SocialKind::Video), query_value(url, "v"), None),
        [_user, "videos", id, ..] => (Some(SocialKind::Video), Some((*id).to_string()), None),
        [user, "posts", id, ..] => (
            Some(SocialKind::Post),
            Some((*id).to_string()),
            Some((*user).to_string()),
        ),
        ["story.php", ..] | ["permalink.php", ..] => (
            Some(SocialKind::Post),
            query_value(url, "story_fbid").or_else(|| query_value(url, "fbid")),
            None,
        ),
        ["photo.php", ..] | ["photo", ..] => {
            (Some(SocialKind::Photo), query_value(url, "fbid"), None)
        }
        ["profile.php", ..] => (Some(SocialKind::Profile), query_value(url, "id"), None),
        [user] if !is_reserved_facebook(user) => {
            (Some(SocialKind::Profile), None, Some((*user).to_string()))
        }
        _ => (None, None, None),
    }
}

fn extract_social_pinterest_parts(segs: &[&str]) -> Parts {
    match segs {
        ["pin", id, ..] if id.chars().all(|c| c.is_ascii_digit()) => {
            (Some(SocialKind::Post), Some((*id).to_string()), None)
        }
        [user, board] if !is_reserved_pinterest(user) => (
            Some(SocialKind::Collection),
            Some((*board).to_string()),
            Some((*user).to_string()),
        ),
        [user] if !is_reserved_pinterest(user) => {
            (Some(SocialKind::Profile), None, Some((*user).to_string()))
        }
        _ => (None, None, None),
    }
}

fn extract_social_youtube_parts(host: &str, segs: &[&str], url: &Url) -> Parts {
    // youtu.be/{id}
    if host == "youtu.be" {
        return match segs {
            [id, ..] => (Some(SocialKind::Video), Some((*id).to_string()), None),
            _ => (None, None, None),
        };
    }
    match segs {
        ["watch", ..] => {
            let id = query_value(url, "v");
            (id.is_some().then_some(SocialKind::Video), id, None)
        }
        ["shorts", id, ..] | ["embed", id, ..] | ["live", id, ..] | ["v", id, ..] => {
            (Some(SocialKind::Video), Some((*id).to_string()), None)
        }
        ["channel", id, ..] => (Some(SocialKind::Profile), None, Some((*id).to_string())),
        ["c", name, ..] | ["user", name, ..] => {
            (Some(SocialKind::Profile), None, Some((*name).to_string()))
        }
        [handle, ..] if handle.starts_with('@') => {
            (Some(SocialKind::Profile), None, Some((*handle).to_string()))
        }
        _ => (None, None, None),
    }
}

fn extract_social_twitter_parts(segs: &[&str]) -> Parts {
    match segs {
        [user, "status", id, ..] => (
            Some(SocialKind::Post),
            Some((*id).to_string()),
            Some(format!("@{}", user)),
        ),
        [user] if !is_reserved_twitter(user) => {
            (Some(SocialKind::Profile), None, Some(format!("@{}", user)))
        }
        _ => (None, None, None),
    }
}

fn extract_social_linkedin_parts(segs: &[&str]) -> Parts {
    match segs {
        ["in", slug, ..] | ["company", slug, ..] => {
            (Some(SocialKind::Profile), None, Some((*slug).to_string()))
        }
        ["posts", slug, ..] | ["pulse", slug, ..] => {
            (Some(SocialKind::Post), Some((*slug).to_string()), None)
        }
        ["feed", "update", id, ..] => (Some(SocialKind::Post), Some((*id).to_string()), None),
        _ => (None, None, None),
    }
}

fn extract_social_vimeo_parts(segs: &[&str]) -> Parts {
    match segs {
        [id, ..] if id.chars().all(|c| c.is_ascii_digit()) => {
            (Some(SocialKind::Video), Some((*id).to_string()), None)
        }
        [user] => (Some(SocialKind::Profile), None, Some((*user).to_string())),
        _ => (None, None, None),
    }
}

// First-segment words that are routes, not usernames.
fn is_reserved_instagram(s: &str) -> bool {
    matches!(
        s,
        "p" | "reel"
            | "reels"
            | "tv"
            | "explore"
            | "accounts"
            | "stories"
            | "directory"
            | "about"
            | "developer"
            | "legal"
    )
}

fn is_reserved_facebook(s: &str) -> bool {
    matches!(
        s,
        "watch"
            | "share"
            | "share.php"
            | "sharer"
            | "sharer.php"
            | "dialog"
            | "story.php"
            | "permalink.php"
            | "photo.php"
            | "photo"
            | "profile.php"
            | "groups"
            | "pages"
            | "events"
            | "marketplace"
            | "gaming"
            | "help"
            | "settings"
    )
}

fn is_reserved_pinterest(s: &str) -> bool {
    matches!(
        s,
        "pin" | "search" | "ideas" | "today" | "business" | "settings" | "categories"
    )
}

fn is_reserved_twitter(s: &str) -> bool {
    matches!(
        s,
        "home"
            | "share"
            | "intent"
            | "search"
            | "explore"
            | "notifications"
            | "messages"
            | "settings"
            | "i"
            | "compose"
            | "hashtag"
            | "login"
            | "signup"
    )
}

/// A query parameter's value, where present.
fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

// ---------------------------------------------------------------------------
// Side-door payload extractors (Reddit `.json` / oEmbed)
// ---------------------------------------------------------------------------

/// Extract a [`PageContent`] from a Reddit `.json` body fetched for `url`: the
/// post, or — when `social` names a comment — that comment under its post's
/// title.
pub(super) fn extract_reddit(
    url: &CanonicalUrl,
    social: &SocialParts,
    body: &str,
) -> Option<PageContent> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| log::debug!("reddit json extract failed: {}", e))
        .ok()?;
    let post = reddit_first_post(&value)?;
    let page = PageContent {
        title: json_string(post.get("title")),
        ..PageContent::new(url.clone(), ContentSource::RedditJson)
    };
    if social.kind == Some(SocialKind::Comment) {
        let comment = reddit_comment(&value, social.id.as_deref()?)?;
        return Some(PageContent {
            text: json_string(comment.get("body")),
            author: json_string(comment.get("author")).map(|a| format!("u/{}", a)),
            ..page
        });
    }
    Some(PageContent {
        text: json_string(post.get("selftext")),
        author: json_string(post.get("author")).map(|a| format!("u/{}", a)),
        image_url: reddit_image(post),
        video_url: reddit_video(post),
        ..page
    })
}

/// The comment `id` in a permalink's `[postListing, commentListing]`.
fn reddit_comment<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    value
        .get(1)?
        .get("data")?
        .get("children")?
        .as_array()?
        .iter()
        .filter_map(|child| child.get("data"))
        .find(|comment| comment.get("id").and_then(Value::as_str) == Some(id))
}

/// Reddit `.json` is either `[postListing, commentListing]` (a permalink) or a
/// single listing (subreddit/user feed). Either way the first child's `data`
/// is the primary post.
fn reddit_first_post(value: &Value) -> Option<&Value> {
    let listing = if value.is_array() {
        value.get(0)?
    } else {
        value
    };
    listing
        .get("data")?
        .get("children")?
        .as_array()?
        .first()?
        .get("data")
}

/// A link-post whose destination is itself an image beats the tiny preview
/// thumbnail; otherwise fall back to the thumbnail. A non-image destination
/// (an article link) has no home in these fields, so it's dropped.
fn reddit_image(post: &Value) -> Option<String> {
    json_string(post.get("url_overridden_by_dest"))
        .filter(|u| is_image_url(u))
        .or_else(|| json_string(post.get("thumbnail")).filter(|t| t.starts_with("http")))
}

/// Only a genuinely playable Reddit-hosted video — never the `url_overridden_by_dest`
/// catch-all, which can be an image or an external link.
fn reddit_video(post: &Value) -> Option<String> {
    post.get("secure_media")
        .and_then(|m| m.get("reddit_video"))
        .and_then(|v| v.get("fallback_url"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Whether a URL points at an image, by file extension (ignoring query/fragment).
fn is_image_url(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".svg"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// Extract a [`PageContent`] from an oEmbed JSON body fetched for `url`.
pub(super) fn extract_oembed(url: &CanonicalUrl, body: &str) -> Option<PageContent> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| log::debug!("oembed extract failed: {}", e))
        .ok()?;
    Some(PageContent {
        title: json_string(value.get("title")),
        author: json_string(value.get("author_name")),
        image_url: json_string(value.get("thumbnail_url")),
        ..PageContent::new(url.clone(), ContentSource::Oembed)
    })
}

/// A trimmed, non-empty string value from a JSON field.
fn json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
