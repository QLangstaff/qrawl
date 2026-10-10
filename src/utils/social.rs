//! Social URL parsing: what a URL's host and path say about the platform, the
//! kind of item, and its identifiers. Pure path-pattern matching, no network.

use url::Url;

use crate::types::{SocialKind, SocialParts, SocialPlatform};

use super::{canonical_domain, host_of_url, social_platform};

/// The [`SocialParts`] of a social URL, or `None` when the host isn't a
/// recognized platform or the input isn't a URL.
pub(crate) fn social_parts(input: &str) -> Option<SocialParts> {
    let url = Url::parse(input).ok()?;
    let host = canonical_domain(&host_of_url(&url)?);
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

/// The account a path segment names, or `None` for a bare separator like `_`,
/// which is a route prefix. Every handle is built from one.
fn account(segment: &str) -> Option<String> {
    segment
        .chars()
        .any(char::is_alphanumeric)
        .then(|| segment.to_string())
}

/// A profile, when `handle` names an account.
fn profile(handle: Option<String>) -> Parts {
    (
        handle.is_some().then_some(SocialKind::Profile),
        None,
        handle,
    )
}

fn reddit_parts(segs: &[&str]) -> Parts {
    match segs {
        // /r/{sub}/comments/{post}/{slug}/{comment}
        ["r", sub, "comments", _post, _slug, comment, ..] => (
            Some(SocialKind::Comment),
            Some((*comment).to_string()),
            account(sub).map(|sub| format!("r/{sub}")),
        ),
        // /r/{sub}/comments/{post}[/slug]
        ["r", sub, "comments", post, ..] => (
            Some(SocialKind::Post),
            Some((*post).to_string()),
            account(sub).map(|sub| format!("r/{sub}")),
        ),
        // /comments/{post} (subreddit omitted)
        ["comments", post, ..] | ["gallery", post, ..] => {
            (Some(SocialKind::Post), Some((*post).to_string()), None)
        }
        // /r/{sub}/s/{code} — modern share link (opaque, needs expansion)
        ["r", sub, "s", ..] => (None, None, account(sub).map(|sub| format!("r/{sub}"))),
        ["r", sub] => {
            let handle = account(sub).map(|sub| format!("r/{sub}"));
            (
                handle.is_some().then_some(SocialKind::Collection),
                None,
                handle,
            )
        }
        ["user", name, ..] | ["u", name, ..] => {
            profile(account(name).map(|name| format!("u/{name}")))
        }
        _ => (None, None, None),
    }
}

fn tiktok_parts(segs: &[&str]) -> Parts {
    match segs {
        [user, "video", id, ..] if user.starts_with('@') => (
            Some(SocialKind::Video),
            Some((*id).to_string()),
            account(user),
        ),
        [user, "photo", id, ..] if user.starts_with('@') => (
            Some(SocialKind::Photo),
            Some((*id).to_string()),
            account(user),
        ),
        [user] if user.starts_with('@') => profile(account(user)),
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
            profile(account(user).map(|user| format!("@{user}")))
        }
        _ => (None, None, None),
    }
}

fn facebook_parts(segs: &[&str], url: &Url) -> Parts {
    match segs {
        ["watch", ..] => {
            let id = query_value(url, "v");
            (id.is_some().then_some(SocialKind::Video), id, None)
        }
        [_user, "videos", id, ..] => (Some(SocialKind::Video), Some((*id).to_string()), None),
        [user, "posts", id, ..] => (
            Some(SocialKind::Post),
            Some((*id).to_string()),
            account(user),
        ),
        ["story.php", ..] | ["permalink.php", ..] => (
            Some(SocialKind::Post),
            query_value(url, "story_fbid").or_else(|| query_value(url, "fbid")),
            None,
        ),
        ["photo.php", ..] | ["photo", ..] => {
            (Some(SocialKind::Photo), query_value(url, "fbid"), None)
        }
        ["profile.php", ..] => match query_value(url, "id") {
            Some(id) => (Some(SocialKind::Profile), Some(id), None),
            None => (None, None, None),
        },
        [user] if !is_reserved_facebook(user) => profile(account(user)),
        _ => (None, None, None),
    }
}

fn pinterest_parts(segs: &[&str]) -> Parts {
    match segs {
        ["pin", id, ..] if id.chars().all(|c| c.is_ascii_digit()) => {
            (Some(SocialKind::Post), Some((*id).to_string()), None)
        }
        // A profile's tabs (`_saved`, `_created`, `pins`) aren't boards.
        [user, tab] if !is_pinterest_route(user) && is_pinterest_tab(tab) => profile(account(user)),
        [user, board] if !is_pinterest_route(user) && account(board).is_some() => {
            match account(user) {
                Some(user) => (
                    Some(SocialKind::Collection),
                    Some((*board).to_string()),
                    Some(user),
                ),
                None => (None, None, None),
            }
        }
        [user] if !is_pinterest_route(user) => profile(account(user)),
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
        // `/embed/videoseries` is a playlist player, not a video.
        ["embed", "videoseries", ..] => (None, None, None),
        ["shorts", id, ..] | ["embed", id, ..] | ["live", id, ..] | ["v", id, ..] => {
            (Some(SocialKind::Video), Some((*id).to_string()), None)
        }
        ["channel", id, ..] | ["c", id, ..] | ["user", id, ..] => profile(account(id)),
        [handle, ..] if handle.starts_with('@') => profile(account(handle)),
        _ => (None, None, None),
    }
}

fn twitter_parts(segs: &[&str]) -> Parts {
    match segs {
        // `/i/status/{id}` addresses a post without naming its author.
        [user, "status", id, ..] => (
            Some(SocialKind::Post),
            Some((*id).to_string()),
            account(user)
                .filter(|user| user != "i")
                .map(|user| format!("@{user}")),
        ),
        [user] if !is_reserved_twitter(user) => {
            profile(account(user).map(|user| format!("@{user}")))
        }
        _ => (None, None, None),
    }
}

fn linkedin_parts(segs: &[&str]) -> Parts {
    match segs {
        ["in", slug, ..] | ["company", slug, ..] => profile(account(slug)),
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
        [user] if !is_reserved_vimeo(user) => profile(account(user)),
        _ => (None, None, None),
    }
}

fn is_reserved_vimeo(s: &str) -> bool {
    matches!(
        s,
        "categories"
            | "channels"
            | "groups"
            | "watch"
            | "features"
            | "upgrade"
            | "about"
            | "help"
            | "blog"
            | "jobs"
            | "log_in"
            | "join"
            | "privacy"
            | "terms"
    )
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

/// A Pinterest profile's tab: a `_`-prefixed segment (`_saved`, `_created`),
/// or `pins`, `boards`, `followers`, `following`.
fn is_pinterest_tab(segment: &str) -> bool {
    segment.starts_with('_') || matches!(segment, "pins" | "boards" | "followers" | "following")
}

/// A first path segment that is Pinterest's own route, not a user's: a
/// reserved word, or a profile tab standing alone.
fn is_pinterest_route(segment: &str) -> bool {
    is_reserved_pinterest(segment) || is_pinterest_tab(segment)
}

/// A `.php` segment is a script (`login.php`, `share.php`), never a profile.
fn is_reserved_facebook(s: &str) -> bool {
    s.ends_with(".php")
        || matches!(
            s,
            "watch"
                | "privacy"
                | "policies"
                | "legal"
                | "about"
                | "login"
                | "share"
                | "sharer"
                | "dialog"
                | "photo"
                | "groups"
                | "pages"
                | "events"
                | "marketplace"
                | "gaming"
                | "help"
                | "settings"
                | "checkpoint"
        )
}

fn is_reserved_pinterest(s: &str) -> bool {
    matches!(
        s,
        "pin"
            | "search"
            | "ideas"
            | "today"
            | "business"
            | "settings"
            | "categories"
            | "login"
            | "signup"
            | "explore"
            | "password"
            | "about"
            | "careers"
            | "privacy"
            | "terms"
    )
}

fn is_reserved_twitter(s: &str) -> bool {
    matches!(
        s,
        "home"
            | "privacy"
            | "tos"
            | "about"
            | "help"
            | "jobs"
            | "download"
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

/// A query parameter's value, where present and not empty.
fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, v)| k == key && !v.is_empty())
        .map(|(_, v)| v.into_owned())
}
