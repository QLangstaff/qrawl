//! Embed readers: a platform's structured representation of an item (Reddit
//! `.json`, oEmbed JSON) scraped into the same metadata pairs a page's
//! `<meta>` tags yield, under `reddit:` and `oembed:` keys.

use serde_json::Value;

use crate::types::{Embed, Metadata};
use crate::utils::embed_keys::*;

pub(super) fn scrape_embed(embed: &Embed) -> Metadata {
    let fields = match embed {
        Embed::RedditJson { body, comment_id } => reddit_fields(body, comment_id.as_deref()),
        Embed::Oembed(body) => oembed_fields(body),
    };
    fields
        .into_iter()
        .filter_map(|(key, value)| Some((key.to_string(), value?)))
        .collect()
}

type Fields = Vec<(&'static str, Option<String>)>;

fn parse(body: &str) -> Option<Value> {
    serde_json::from_str(body)
        .map_err(|e| log::debug!("embed is not JSON: {}", e))
        .ok()
}

fn oembed_fields(body: &str) -> Fields {
    let Some(value) = parse(body) else {
        return Vec::new();
    };
    vec![
        (OEMBED_TITLE, json_string(value.get("title"))),
        (OEMBED_AUTHOR_NAME, json_string(value.get("author_name"))),
        (
            OEMBED_THUMBNAIL_URL,
            json_string(value.get("thumbnail_url")),
        ),
    ]
}

/// The post's fields — or, for `comment_id`, that comment's text and author
/// under its post's title.
fn reddit_fields(body: &str, comment_id: Option<&str>) -> Fields {
    let Some(value) = parse(body) else {
        return Vec::new();
    };
    let Some(post) = reddit_first_post(&value) else {
        return Vec::new();
    };
    let author = |item: &Value| json_string(item.get("author")).map(|a| format!("u/{}", a));
    let title = (REDDIT_TITLE, json_string(post.get("title")));

    if let Some(comment_id) = comment_id {
        return match reddit_comment(&value, comment_id) {
            Some(comment) => vec![
                title,
                (REDDIT_TEXT, json_string(comment.get("body"))),
                (REDDIT_AUTHOR, author(comment)),
            ],
            None => Vec::new(),
        };
    }
    vec![
        title,
        (REDDIT_TEXT, json_string(post.get("selftext"))),
        (REDDIT_AUTHOR, author(post)),
        (REDDIT_IMAGE, reddit_image(post)),
        (REDDIT_VIDEO, reddit_video(post)),
        // A link post's destination: the page the post is about.
        (REDDIT_URL, json_string(post.get("url_overridden_by_dest"))),
    ]
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
/// thumbnail; otherwise fall back to the thumbnail.
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

/// A trimmed, non-empty string value from a JSON field.
fn json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
