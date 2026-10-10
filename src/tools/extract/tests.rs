#![cfg(test)]
use crate::tools::extract::*;
use crate::types::{SocialKind, SocialPlatform};
use serde_json::json;

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

    let extracted = extract_preview(&metadata);
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
    let r = extract_preview(&metadata);
    assert_eq!(r.title.as_deref(), Some("OG Title"));
    assert_eq!(r.text.as_deref(), Some("OG Desc"));
    assert_eq!(r.image_url.as_deref(), Some("https://img.example/a.jpg"));
    assert_eq!(r.video_url.as_deref(), Some("https://vid.example/v.mp4"));
    assert_eq!(r.author.as_deref(), Some("Jane Doe"));
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

#[test]
fn extract_preview_prefers_embed_keys_over_page_metadata() {
    let metadata = vec![
        ("oembed:title".to_string(), "The video".to_string()),
        ("oembed:author_name".to_string(), "Creator".to_string()),
        ("title".to_string(), "TikTok - Make Your Day".to_string()),
        ("og:site_name".to_string(), "TikTok".to_string()),
        ("description".to_string(), "Page description".to_string()),
    ];
    let preview = extract_preview(&metadata);
    assert_eq!(preview.title.as_deref(), Some("The video"));
    assert_eq!(preview.author.as_deref(), Some("Creator"));
    assert_eq!(preview.text.as_deref(), Some("Page description"));
}

#[test]
fn extract_schema_types_shortens_iris_and_dedupes() {
    let jsonld = vec![
        json!({"@type": "https://schema.org/Recipe"}),
        json!({"@type": ["Recipe", "schema:Article"]}),
    ];
    assert_eq!(extract_schema_types(&jsonld), ["Recipe", "Article"]);
}

#[test]
fn site_pages_and_authorless_paths_are_not_profiles() {
    for url in [
        "https://x.com/privacy",
        "https://facebook.com/privacy",
        "https://vimeo.com/categories",
        "https://facebook.com/watch/",
    ] {
        let social = extract_social_parts(url).expect("social host");
        assert_eq!(social.kind, None, "{url}");
    }
    let post = extract_social_parts("https://x.com/i/status/1").expect("social host");
    assert_eq!(post.kind, Some(SocialKind::Post));
    assert_eq!(post.handle, None);
}

#[test]
fn a_www_prefix_does_not_change_what_a_short_host_means() {
    let short = extract_social_parts("https://www.pin.it/abc").expect("social host");
    assert_eq!(short.kind, None);
    let video = extract_social_parts("https://www.youtu.be/dQw4w9WgXcQ").expect("social host");
    assert_eq!(video.kind, Some(SocialKind::Video));
    assert_eq!(video.id.as_deref(), Some("dQw4w9WgXcQ"));
}

#[test]
fn is_social_url_true_for_known_platforms() {
    assert!(is_social("https://tiktok.com/@user/video/123"));
    assert!(is_social("https://reddit.com/r/rust"));
    assert!(is_social("https://youtu.be/dQw4w9WgXcQ"));
    assert!(is_social("https://www.pinterest.co.uk/pin/999"));
    // Recognized platform, opaque path — still a social URL.
    assert!(is_social("https://vm.tiktok.com/ZMabcdef"));
}

#[test]
fn is_social_url_false_for_web_and_garbage() {
    assert!(!is_social("https://example.com/foo/bar"));
    assert!(!is_social("https://www.allrecipes.com/recipe/10813"));
    assert!(!is_social("http://")); // a URL, but has no host
    assert!(!is_social("not a url"));
    assert!(!is_social("mailto:hi@example.com"));
}

#[test]
fn pinterest_label_needs_a_public_suffix_after_it() {
    assert!(is_social("https://www.pinterest.co.uk/pin/1"));
    assert!(is_social("https://uk.pinterest.com/pin/1"));
    assert!(!is_social("https://pinterest.evil.com/pin/1"));
}

#[test]
fn pinterest_label_before_an_unrelated_domain_is_not_pinterest() {
    assert!(is_social("https://pinterest.com.au/pin/1"));
    assert!(is_social("https://pinterest.de/pin/1"));
    assert!(!is_social("https://pinterest.abc.com/pin/1"));
    assert!(!is_social("https://pinterest.aws.dev/pin/1"));
}

#[test]
fn only_pinterest_country_hosts_are_pinterest() {
    assert!(is_social("https://pinterest.ca/pin/1"));
    assert!(is_social("https://www.pinterest.co.kr/pin/1"));
    assert!(!is_social("https://pinterest.io/pin/1"));
    assert!(!is_social("https://pinterest.ws/pin/1"));
}

fn is_social(url: &str) -> bool {
    extract_social_parts(url).is_some()
}

#[tokio::test]
async fn emails_and_phones_come_from_content_not_from_scripts() {
    let html = r#"<html><body>
        <script>var owner = "js@tracker.io"; var id=4155551234567;</script>
        <style>.a::after { content: "css@style.io"; }</style>
        <p>Write to team@example.com or call 415-555-1234.</p>
        <a href="MAILTO:Sales@Example.com?subject=hi">Sales</a>
    </body></html>"#;
    let emails = extract_emails(&html.into()).await;
    assert_eq!(emails, ["Sales@Example.com", "team@example.com"]);
    let phones = extract_phones(&html.into()).await;
    assert_eq!(phones, ["415-555-1234"]);
}

#[test]
fn a_playlist_player_is_not_a_video() {
    let player =
        extract_social_parts("https://youtube.com/embed/videoseries?list=PL1").expect("youtube");
    assert_eq!(player.kind, None);
}

#[tokio::test]
async fn jsonld_emails_and_phones_come_from_their_properties_only() {
    let html = r#"<html><head><script type="application/ld+json">{
        "@type":"Organization",
        "email":"ld@example.com",
        "telephone":"212-555-1212",
        "description":"Reach us:\nme@x.com",
        "image":"https://cdn.example.com/img@2x.png",
        "gtin13":"0012345678905",
        "contactPoint":{"@type":"ContactPoint","email":"help@example.com"}
    }</script></head><body></body></html>"#;
    let mut emails = extract_emails(&html.into()).await;
    emails.sort();
    assert_eq!(emails, ["help@example.com", "ld@example.com"]);
    assert_eq!(extract_phones(&html.into()).await, ["212-555-1212"]);
}
