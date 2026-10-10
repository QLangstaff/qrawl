#![cfg(test)]
use crate::tools::extract::utils::{extract_oembed, extract_reddit};
use crate::tools::extract::*;
use crate::types::{CanonicalUrl, SocialKind, SocialPlatform};
use serde_json::json;

/// A canonical URL for the payload-extractor tests, which need one to build a
/// [`PageContent`] but don't care about its value.
fn url() -> CanonicalUrl {
    CanonicalUrl::new("https://example.com/x")
}

#[tokio::test]
async fn test_extract_emails_basic() {
    let html = r#"
            <html>
                <body>
                    <a href="mailto:john@example.com">Email John</a>
                    <p>Contact us at support@example.com</p>
                </body>
            </html>
        "#;

    let emails = extract_emails(&html.into()).await;
    assert!(emails.len() >= 2);
    assert!(emails.contains(&"john@example.com".to_string()));
    assert!(emails.contains(&"support@example.com".to_string()));
}

#[tokio::test]
async fn test_extract_phones_basic() {
    let html = r#"
            <html>
                <body>
                    <a href="tel:555-123-4567">Call us</a>
                    <p>Phone: (555) 987-6543</p>
                </body>
            </html>
        "#;

    let phones = extract_phones(&html.into()).await;
    assert!(phones.len() >= 2);
}

#[test]
fn test_extract_og_uses_metadata_fallbacks() {
    let metadata = vec![
        ("og:title".to_string(), "OG Title".to_string()),
        (
            "twitter:description".to_string(),
            "Twitter Description".to_string(),
        ),
        (
            "og:image:secure_url".to_string(),
            "https://secure.example.com/image.jpg".to_string(),
        ),
    ];

    let extracted = extract_open_graph(&url(), &metadata);
    assert_eq!(extracted.title, Some("OG Title".to_string()));
    assert_eq!(extracted.text, Some("Twitter Description".to_string()));
    assert_eq!(
        extracted.image_url,
        Some("https://secure.example.com/image.jpg".to_string())
    );
}

#[test]
fn test_extract_schema_types_collects_unique_values() {
    let jsonld = vec![
        json!({
            "@type": ["Recipe", "Article"]
        }),
        json!({
            "@type": "Article"
        }),
        json!({
            "@type": ["HowTo", "Recipe"]
        }),
    ];

    let mut types = extract_schema_types(&jsonld);
    types.sort();
    assert_eq!(types, vec!["Article", "HowTo", "Recipe"]);
}

#[tokio::test]
async fn test_extract_emails_collects_raw_results() {
    let html = r#"
            <html>
                <body>
                    <a href="mailto:info@example.com">Email</a>
                    <p>Contact: info@example.com</p>
                </body>
            </html>
        "#;

    let emails = extract_emails(&html.into()).await;
    assert_eq!(
        emails,
        vec!["info@example.com", "info@example.com"]
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_extract_phones_preserves_formats() {
    let html = r#"
            <html>
                <body>
                    <a href="tel:+1-555-123-4567">Call</a>
                    <span>+1 (555) 123-4567</span>
                </body>
            </html>
        "#;

    let phones = extract_phones(&html.into()).await;
    assert_eq!(phones.len(), 2); // Raw formats retained for downstream normalization
    assert!(phones.contains(&"+1-555-123-4567".to_string()));
    assert!(phones.contains(&"+1 (555) 123-4567".to_string()));
}

// Self-contained: links are extracted by `extract_social_parts` (no `classify`
// dependency) and only known-platform / known-kind ones survive, as canonical URLs.
#[tokio::test]
async fn test_extract_social_urls_keeps_profiles_drops_noise() {
    let html = r#"
            <html><body>
              <a href="https://instagram.com/nasa">IG</a>
              <a href="https://twitter.com/nasa">X</a>
              <a href="https://www.youtube.com/@NASA">YT</a>
              <a href="/about">About</a>
              <a href="https://facebook.com/sharer/sharer.php?u=x">Share</a>
              <a href="mailto:hi@nasa.gov">Email</a>
              <a href="https://example.com/page">External</a>
            </body></html>
        "#;
    let socials = extract_social_urls(&html.into()).await;
    assert!(socials.contains(&"https://instagram.com/nasa".to_string()));
    assert!(socials.contains(&"https://twitter.com/nasa".to_string()));
    assert!(socials.contains(&"https://youtube.com/@NASA".to_string()));
    // Dropped: relative, share-button (Unknown kind), mailto (no host), external host.
    assert!(!socials.iter().any(|s| s.contains("sharer")));
    assert!(!socials.iter().any(|s| s.contains("example.com")));
    assert!(!socials.iter().any(|s| s.contains("about")));
}

// ---------------------------------------------------------------------------
// extract_social_parts — extract a social URL's parts (moved here from classify:
// parsing is extraction). Covers the per-platform path shapes; the yes/no host
// predicate (`classify_is_social_url`) is tested in `classify`.
// Inputs are already canonical (the pipeline runs `normalize_social` first).
// ---------------------------------------------------------------------------

/// Extract a URL's [`SocialParts`] parts. Panics on a non-social URL.
fn social(url: &str) -> SocialParts {
    extract_social_parts(url).unwrap_or_else(|| panic!("expected a social URL: {url}"))
}

#[test]
fn reddit_post() {
    let s = social("https://reddit.com/r/rust/comments/abc123/some_title");
    assert_eq!(s.platform, SocialPlatform::Reddit);
    assert_eq!(s.kind, Some(SocialKind::Post));
    assert_eq!(s.id.as_deref(), Some("abc123"));
    assert_eq!(s.handle.as_deref(), Some("r/rust"));
}

#[test]
fn reddit_comment() {
    let s = social("https://reddit.com/r/rust/comments/abc123/some_title/def456");
    assert_eq!(s.kind, Some(SocialKind::Comment));
    assert_eq!(s.id.as_deref(), Some("def456"));
    assert_eq!(s.handle.as_deref(), Some("r/rust"));
}

#[test]
fn reddit_subreddit_and_user() {
    let sub = social("https://reddit.com/r/rust");
    assert_eq!(sub.kind, Some(SocialKind::Collection));
    assert_eq!(sub.handle.as_deref(), Some("r/rust"));

    let user = social("https://reddit.com/user/spez");
    assert_eq!(user.kind, Some(SocialKind::Profile));
    assert_eq!(user.handle.as_deref(), Some("u/spez"));
}

#[test]
fn reddit_share_link_is_opaque_but_keeps_sub() {
    let s = social("https://reddit.com/r/rust/s/AbCdEf");
    assert_eq!(s.platform, SocialPlatform::Reddit);
    assert_eq!(s.kind, None);
    assert_eq!(s.handle.as_deref(), Some("r/rust"));
}

#[test]
fn reddit_short_link_is_opaque() {
    let s = social("https://redd.it/abc123");
    assert_eq!(s.platform, SocialPlatform::Reddit);
    assert_eq!(s.kind, None);
    assert!(s.id.is_none());
}

#[test]
fn tiktok_video() {
    let s = social("https://tiktok.com/@charlidamelio/video/7212345678901234567");
    assert_eq!(s.platform, SocialPlatform::TikTok);
    assert_eq!(s.kind, Some(SocialKind::Video));
    assert_eq!(s.id.as_deref(), Some("7212345678901234567"));
    assert_eq!(s.handle.as_deref(), Some("@charlidamelio"));
}

#[test]
fn tiktok_profile() {
    let s = social("https://tiktok.com/@charlidamelio");
    assert_eq!(s.kind, Some(SocialKind::Profile));
    assert_eq!(s.handle.as_deref(), Some("@charlidamelio"));
}

#[test]
fn tiktok_short_link_is_opaque() {
    // `vm.`/`vt.` shorteners are kept (not stripped like `m.`) — the path is an
    // opaque code, so kind is unresolved.
    let s = social("https://vm.tiktok.com/ZMabcdef");
    assert_eq!(s.platform, SocialPlatform::TikTok);
    assert_eq!(s.kind, None);
}

#[test]
fn instagram_post_reel_profile() {
    let post = social("https://instagram.com/p/CabcDEF123");
    assert_eq!(post.platform, SocialPlatform::Instagram);
    assert_eq!(post.kind, Some(SocialKind::Post));
    assert_eq!(post.id.as_deref(), Some("CabcDEF123"));

    let reel = social("https://instagram.com/reel/Cxyz987");
    assert_eq!(reel.kind, Some(SocialKind::Video));
    assert_eq!(reel.id.as_deref(), Some("Cxyz987"));

    let profile = social("https://instagram.com/nasa");
    assert_eq!(profile.kind, Some(SocialKind::Profile));
    assert_eq!(profile.handle.as_deref(), Some("@nasa"));
}

#[test]
fn instagram_reserved_word_is_not_a_profile() {
    let s = social("https://instagram.com/explore");
    assert_eq!(s.kind, None);
}

#[test]
fn youtube_watch() {
    // Canonical input has sorted params (`t` before `v`); `v` is the video id.
    let s = social("https://youtube.com/watch?t=30&v=dQw4w9WgXcQ");
    assert_eq!(s.platform, SocialPlatform::YouTube);
    assert_eq!(s.kind, Some(SocialKind::Video));
    assert_eq!(s.id.as_deref(), Some("dQw4w9WgXcQ"));
}

#[test]
fn youtube_short_link_classifies() {
    let s = social("https://youtu.be/dQw4w9WgXcQ");
    assert_eq!(s.platform, SocialPlatform::YouTube);
    assert_eq!(s.kind, Some(SocialKind::Video));
    assert_eq!(s.id.as_deref(), Some("dQw4w9WgXcQ"));
}

#[test]
fn youtube_shorts_and_handle_and_channel() {
    assert_eq!(
        social("https://youtube.com/shorts/abc123").kind,
        Some(SocialKind::Video)
    );

    let handle = social("https://youtube.com/@MrBeast");
    assert_eq!(handle.kind, Some(SocialKind::Profile));
    assert_eq!(handle.handle.as_deref(), Some("@MrBeast"));

    let channel = social("https://youtube.com/channel/UC123abc");
    assert_eq!(channel.kind, Some(SocialKind::Profile));
    assert_eq!(channel.handle.as_deref(), Some("UC123abc"));
}

#[test]
fn twitter_status_on_both_domains() {
    let t = social("https://twitter.com/jack/status/20");
    assert_eq!(t.platform, SocialPlatform::Twitter);
    assert_eq!(t.kind, Some(SocialKind::Post));
    assert_eq!(t.id.as_deref(), Some("20"));
    assert_eq!(t.handle.as_deref(), Some("@jack"));

    let x = social("https://x.com/jack/status/20");
    assert_eq!(x.platform, SocialPlatform::Twitter);
    assert_eq!(x.id.as_deref(), Some("20"));
}

#[test]
fn twitter_profile() {
    let s = social("https://x.com/nasa");
    assert_eq!(s.kind, Some(SocialKind::Profile));
    assert_eq!(s.handle.as_deref(), Some("@nasa"));
}

#[test]
fn pinterest_pin_board_profile() {
    let pin = social("https://pinterest.com/pin/12345");
    assert_eq!(pin.platform, SocialPlatform::Pinterest);
    assert_eq!(pin.kind, Some(SocialKind::Post));
    assert_eq!(pin.id.as_deref(), Some("12345"));

    let board = social("https://pinterest.com/someuser/recipes");
    assert_eq!(board.kind, Some(SocialKind::Collection));
    assert_eq!(board.id.as_deref(), Some("recipes"));
    assert_eq!(board.handle.as_deref(), Some("someuser"));

    let profile = social("https://pinterest.com/someuser");
    assert_eq!(profile.kind, Some(SocialKind::Profile));
    assert_eq!(profile.handle.as_deref(), Some("someuser"));
}

#[test]
fn pinterest_international_tld() {
    let s = social("https://pinterest.co.uk/pin/999");
    assert_eq!(s.platform, SocialPlatform::Pinterest);
    assert_eq!(s.kind, Some(SocialKind::Post));
}

#[test]
fn linkedin_profile_and_company() {
    assert_eq!(
        social("https://linkedin.com/in/williamhgates")
            .handle
            .as_deref(),
        Some("williamhgates")
    );
    assert_eq!(
        social("https://linkedin.com/company/microsoft").kind,
        Some(SocialKind::Profile)
    );
}

#[test]
fn vimeo_video_and_profile() {
    let v = social("https://vimeo.com/123456789");
    assert_eq!(v.platform, SocialPlatform::Vimeo);
    assert_eq!(v.kind, Some(SocialKind::Video));
    assert_eq!(v.id.as_deref(), Some("123456789"));

    let p = social("https://vimeo.com/johndoe");
    assert_eq!(p.kind, Some(SocialKind::Profile));
    assert_eq!(p.handle.as_deref(), Some("johndoe"));
}

#[test]
fn facebook_watch_and_profile() {
    let w = social("https://facebook.com/watch?v=123456");
    assert_eq!(w.platform, SocialPlatform::Facebook);
    assert_eq!(w.kind, Some(SocialKind::Video));
    assert_eq!(w.id.as_deref(), Some("123456"));

    let p = social("https://facebook.com/zuck");
    assert_eq!(p.kind, Some(SocialKind::Profile));
    assert_eq!(p.handle.as_deref(), Some("zuck"));
}

#[test]
fn non_social_and_unparseable_are_none() {
    assert!(extract_social_parts("https://example.com/foo/bar").is_none());
    assert!(extract_social_parts("https://www.allrecipes.com/recipe/10813").is_none());
    assert!(extract_social_parts("http://").is_none()); // a URL, but has no host
    assert!(extract_social_parts("mailto:hi@example.com").is_none());
    assert!(extract_social_parts("not a url").is_none());
}

#[test]
fn social_serializes_with_camel_case() {
    let s = social("https://tiktok.com/@user/video/123");
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["platform"], "TikTok");
    assert_eq!(json["kind"], "Video");
    assert_eq!(json["id"], "123");
    assert_eq!(json["handle"], "@user");
}

// ===== Side-door payload extractors (pure, no network) =====

#[test]
fn extract_reddit_permalink_array() {
    let body = r#"[
        {"kind":"Listing","data":{"children":[
            {"kind":"t3","data":{
                "title":"Hello World",
                "selftext":"Body text here",
                "author":"spez",
                "thumbnail":"https://b.thumbs.redditmedia.com/x.jpg"
            }}
        ]}},
        {"kind":"Listing","data":{"children":[]}}
    ]"#;
    let r = extract_reddit(&url(), &reddit_post_parts(), body).expect("parses");
    assert_eq!(r.url, url());
    assert_eq!(r.title.as_deref(), Some("Hello World"));
    assert_eq!(r.text.as_deref(), Some("Body text here"));
    assert_eq!(r.author.as_deref(), Some("u/spez"));
    // No link destination, so the preview thumbnail is the image.
    assert_eq!(
        r.image_url.as_deref(),
        Some("https://b.thumbs.redditmedia.com/x.jpg")
    );
    assert!(r.video_url.is_none());
}

#[test]
fn extract_reddit_subreddit_listing() {
    // Single listing (not an array): take the first post; empty selftext and
    // a non-http thumbnail collapse to None.
    let body = r#"{"kind":"Listing","data":{"children":[
        {"kind":"t3","data":{"title":"Top Post","selftext":"","author":"alice","thumbnail":"self"}}
    ]}}"#;
    let r = extract_reddit(&url(), &reddit_post_parts(), body).expect("parses");
    assert_eq!(r.title.as_deref(), Some("Top Post"));
    assert!(r.text.is_none());
    assert_eq!(r.author.as_deref(), Some("u/alice"));
    assert!(r.image_url.is_none());
}

#[test]
fn extract_reddit_splits_video_and_image_honestly() {
    // A hosted video → video_url; an image link-post destination beats the
    // thumbnail → image_url; a non-image article destination has no home → dropped.
    let video = r#"{"kind":"Listing","data":{"children":[{"kind":"t3","data":{
        "title":"Clip","thumbnail":"https://b.thumbs/x.jpg",
        "secure_media":{"reddit_video":{"fallback_url":"https://v.redd.it/x/DASH_720.mp4"}}
    }}]}}"#;
    let r = extract_reddit(&url(), &reddit_post_parts(), video).expect("parses");
    assert_eq!(
        r.video_url.as_deref(),
        Some("https://v.redd.it/x/DASH_720.mp4")
    );
    assert_eq!(r.image_url.as_deref(), Some("https://b.thumbs/x.jpg"));

    let image_dest = r#"{"kind":"Listing","data":{"children":[{"kind":"t3","data":{
        "title":"Pic","thumbnail":"https://b.thumbs/x.jpg",
        "url_overridden_by_dest":"https://i.redd.it/full.png"
    }}]}}"#;
    let r = extract_reddit(&url(), &reddit_post_parts(), image_dest).expect("parses");
    assert_eq!(r.image_url.as_deref(), Some("https://i.redd.it/full.png"));
    assert!(r.video_url.is_none());

    let article_dest = r#"{"kind":"Listing","data":{"children":[{"kind":"t3","data":{
        "title":"Link","url_overridden_by_dest":"https://news.example/story"
    }}]}}"#;
    let r = extract_reddit(&url(), &reddit_post_parts(), article_dest).expect("parses");
    assert!(
        r.image_url.is_none(),
        "an article link is neither image nor video"
    );
    assert!(r.video_url.is_none());
}

#[test]
fn extract_reddit_garbage_is_none() {
    assert!(extract_reddit(&url(), &reddit_post_parts(), "<html>not json</html>").is_none());
}

#[test]
fn extract_oembed_fields() {
    let body = r#"{
        "title":"My Video",
        "author_name":"Creator",
        "thumbnail_url":"https://i.example/t.jpg",
        "html":"<iframe></iframe>"
    }"#;
    let r = extract_oembed(&url(), body).expect("parses");
    assert_eq!(r.title.as_deref(), Some("My Video"));
    assert_eq!(r.author.as_deref(), Some("Creator"));
    assert_eq!(r.image_url.as_deref(), Some("https://i.example/t.jpg"));
    assert!(r.text.is_none());
    assert!(r.video_url.is_none());
}

#[test]
fn extract_og_maps_all_fields() {
    let metadata = vec![
        ("og:title".to_string(), "OG Title".to_string()),
        ("og:description".to_string(), "OG Desc".to_string()),
        (
            "og:image".to_string(),
            "https://img.example/a.jpg".to_string(),
        ),
        (
            "og:video".to_string(),
            "https://vid.example/v.mp4".to_string(),
        ),
        ("author".to_string(), "Jane Doe".to_string()),
    ];
    let r = extract_open_graph(&url(), &metadata);
    assert_eq!(r.title.as_deref(), Some("OG Title"));
    assert_eq!(r.text.as_deref(), Some("OG Desc"));
    assert_eq!(r.image_url.as_deref(), Some("https://img.example/a.jpg"));
    assert_eq!(r.video_url.as_deref(), Some("https://vid.example/v.mp4"));
    assert_eq!(r.author.as_deref(), Some("Jane Doe"));
}

#[test]
fn page_content_has_content_requires_a_content_field() {
    let empty = PageContent::new(url(), ContentSource::OpenGraph);
    assert!(!empty.has_content());
    let with_title = PageContent {
        title: Some("x".to_string()),
        ..empty
    };
    assert!(with_title.has_content());
}

fn reddit_post_parts() -> crate::types::SocialParts {
    extract_social_parts("https://reddit.com/r/rust/comments/abc/title").expect("reddit post")
}

#[test]
fn extract_reddit_comment_returns_the_comment_not_its_post() {
    let url = CanonicalUrl::new("https://reddit.com/r/rust/comments/abc/title/def");
    let social = extract_social_parts(url.as_str()).expect("reddit comment");
    let body = r#"[
        {"data":{"children":[{"data":{"title":"Post title","selftext":"post text","author":"op"}}]}},
        {"data":{"children":[
            {"data":{"id":"zzz","body":"another comment","author":"other"}},
            {"data":{"id":"def","body":"the comment","author":"commenter"}}
        ]}}
    ]"#;
    let page = extract_reddit(&url, &social, body).expect("parses");
    assert_eq!(page.title.as_deref(), Some("Post title"));
    assert_eq!(page.text.as_deref(), Some("the comment"));
    assert_eq!(page.author.as_deref(), Some("u/commenter"));
}

#[test]
fn share_buttons_have_no_social_kind() {
    for url in [
        "https://twitter.com/share?url=https://example.com",
        "https://twitter.com/intent/tweet?url=https://example.com",
        "https://facebook.com/sharer.php?u=https://example.com",
        "https://pinterest.com/pin/create/button/?url=https://example.com",
    ] {
        let social = extract_social_parts(url).expect("social host");
        assert_eq!(social.kind, None, "{url}");
    }
    let profile = extract_social_parts("https://twitter.com/rustlang").expect("social host");
    assert_eq!(profile.kind, Some(SocialKind::Profile));
}
