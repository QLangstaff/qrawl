//! Social URL parsing: what a URL's host and path say about the platform, the
//! kind of item, and its identifiers. Pure path-pattern matching, no network.

use url::Url;

use crate::types::{SocialKind, SocialParts, SocialPlatform};

use super::{host_of_url, social_platform};

/// The [`SocialParts`] of a social URL, or `None` when the host isn't a
/// recognized platform or the input isn't a URL.
pub(crate) fn social_parts(input: &str) -> Option<SocialParts> {
    let url = Url::parse(input).ok()?;
    let host = host_of_url(&url)?;
    let platform = social_platform(&host)?;

    let (kind, id, handle) = if is_opaque_shortener(&host) {
        // Platform known, but the path is an opaque short code that only a
        // network round-trip expands.
        (None, None, None)
    } else {
        // Owned segments, then a `&str` view for ergonomic slice patterns.
        let segments = path_segments(&url);
        let segs: Vec<&str> = segments.iter().map(String::as_str).collect();
        match platform {
            SocialPlatform::Reddit => reddit_parts(&segs),
            SocialPlatform::TikTok => tiktok_parts(&segs),
            SocialPlatform::Instagram => instagram_parts(&segs),
            SocialPlatform::Facebook => facebook_parts(&segs, &url),
            SocialPlatform::Pinterest => pinterest_parts(&segs),
            SocialPlatform::YouTube => youtube_parts(&host, &segs, &url),
            SocialPlatform::Twitter => twitter_parts(&segs),
            SocialPlatform::LinkedIn => linkedin_parts(&segs),
            SocialPlatform::Vimeo => vimeo_parts(&segs),
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

fn reddit_parts(segs: &[&str]) -> Parts {
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

fn tiktok_parts(segs: &[&str]) -> Parts {
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

fn instagram_parts(segs: &[&str]) -> Parts {
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

fn facebook_parts(segs: &[&str], url: &Url) -> Parts {
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

fn pinterest_parts(segs: &[&str]) -> Parts {
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

fn youtube_parts(host: &str, segs: &[&str], url: &Url) -> Parts {
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

fn twitter_parts(segs: &[&str]) -> Parts {
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

fn linkedin_parts(segs: &[&str]) -> Parts {
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

fn vimeo_parts(segs: &[&str]) -> Parts {
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
