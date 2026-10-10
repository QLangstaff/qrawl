#![cfg(test)]
use crate::hosts::host_matches;
use crate::tools::fetch::headers::headers_for_profile;
use crate::tools::fetch::social::{fetch_social, oembed_request_url, reddit_json_url};
use crate::tools::fetch::strategies::{acquire_host_permit, HOST_SEMAPHORES, PER_HOST_CONCURRENCY};
use crate::tools::fetch::types::FetchProfile;
use crate::tools::fetch::utils::{is_url_allowed, validate_response};
use crate::types::{SocialKind, SocialParts, SocialPlatform};
use reqwest::StatusCode;
use std::time::{Duration, Instant};

fn padded_html(marker: &str) -> String {
    let filler = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
    let mut body = String::new();
    while body.len() < 600 {
        body.push_str(filler);
    }
    format!("<!DOCTYPE html><html><body>{marker} {}</body></html>", body)
}

#[test]
fn minimal_has_only_user_agent() {
    let headers = headers_for_profile(FetchProfile::Minimal);
    assert!(headers.contains_key("user-agent"));
    assert_eq!(headers.len(), 1);
}

#[test]
fn windows_has_chrome_headers() {
    let headers = headers_for_profile(FetchProfile::Windows);
    assert!(headers.contains_key("sec-ch-ua"));
    assert!(headers.contains_key("sec-ch-ua-platform"));
    assert_eq!(
        headers
            .get("sec-ch-ua-platform")
            .and_then(|v| v.to_str().ok()),
        Some("\"Windows\"")
    );
    assert_eq!(
        headers
            .get("sec-ch-ua-mobile")
            .and_then(|v| v.to_str().ok()),
        Some("?0")
    );
}

#[test]
fn ios_has_safari_headers() {
    let headers = headers_for_profile(FetchProfile::IOS);
    assert!(headers.contains_key("accept"));
    assert!(!headers.contains_key("sec-ch-ua"));
}

#[test]
fn android_has_mobile_chrome_headers() {
    let headers = headers_for_profile(FetchProfile::Android);
    assert!(headers.contains_key("sec-ch-ua-mobile"));
    assert_eq!(
        headers
            .get("sec-ch-ua-mobile")
            .and_then(|v| v.to_str().ok()),
        Some("?1")
    );
    assert_eq!(
        headers
            .get("sec-ch-ua-platform")
            .and_then(|v| v.to_str().ok()),
        Some("\"Android\"")
    );
}

#[test]
fn is_suspicious_cloudflare_challenge() {
    let html = padded_html("Checking your browser before accessing... cf-browser-verification");
    let err = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(err.to_string().contains("suspicious"));
    assert!(err.to_string().contains("cf-browser-verification"));
}

#[test]
fn is_suspicious_cloudflare_captcha() {
    let html = padded_html("Please complete the captcha to continue. cf-captcha-container");
    let err = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(err.to_string().contains("suspicious"));
    assert!(err.to_string().contains("please complete the captcha"));
}

#[test]
fn is_suspicious_perimeter_x() {
    let html = padded_html("PerimeterX robot detection blocking this request");
    let err = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(err.to_string().contains("suspicious"));
    assert!(err.to_string().contains("perimeterx"));
}

#[test]
fn is_suspicious_generic_captcha() {
    let html = padded_html("Please solve this captcha to verify you are a human");
    let err = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(err.to_string().contains("suspicious"));
    assert!(err.to_string().contains("verify you are a human"));
}

#[test]
fn forbidden_text_in_body_is_not_a_block() {
    // A 200-OK page is judged by status + HTML markers + bot-challenge markers,
    // NOT by stray "forbidden" / "access denied" words — those appear in
    // legitimate page content. A real auth block arrives with a 401/403 status.
    let html = padded_html(
        "<h1>Recipe</h1><p>Unauthorized reproduction forbidden. Access denied to bots.</p>",
    );
    assert!(validate_response(StatusCode::OK, &html).is_ok());
}

#[test]
fn validate_response_normal_content() {
    let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Welcome</h1><p>This is normal content with lots of text to meet the minimum length requirement. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.</p></body></html>"#;
    assert!(validate_response(StatusCode::OK, html).is_ok());
}

#[test]
fn validate_response_non_success_status() {
    let html = r#"<!DOCTYPE html><html><body><h1>Page content</h1></body></html>"#;
    assert!(validate_response(StatusCode::NOT_FOUND, html).is_err());
    assert!(validate_response(StatusCode::INTERNAL_SERVER_ERROR, html).is_err());
    assert!(validate_response(StatusCode::FORBIDDEN, html).is_err());
}

#[test]
fn detect_body_too_short() {
    let html = r#"<html><body>Short</body></html>"#;
    let err = validate_response(StatusCode::OK, html).unwrap_err();
    assert!(err.to_string().contains("body is too short"));
}

#[test]
fn detect_non_html_content() {
    let json = r#"{"status": "ok", "data": "This is JSON not HTML but has enough length to pass the minimum length check so we need more text here to make it realistic. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore. And some additional text to ensure we exceed 500 bytes."}"#;
    let err = validate_response(StatusCode::OK, json).unwrap_err();
    assert!(err.to_string().contains("missing HTML markers"));
}

#[test]
fn host_matches_equal_and_subdomain() {
    assert!(host_matches("reddit.com", "reddit.com"));
    assert!(host_matches("old.reddit.com", "reddit.com"));
    assert!(host_matches("a.b.reddit.com", "reddit.com"));
    assert!(!host_matches("notreddit.com", "reddit.com"));
    assert!(!host_matches("reddit.com.evil.com", "reddit.com"));
    assert!(!host_matches("reddit.co", "reddit.com"));
}

#[test]
fn is_url_allowed_block_list() {
    let block = vec!["reddit.com".to_string(), "tiktok.com".to_string()];
    assert!(!is_url_allowed("https://reddit.com/r/rust", &[], &block));
    assert!(!is_url_allowed(
        "https://old.reddit.com/r/rust",
        &[],
        &block
    ));
    assert!(is_url_allowed("https://example.com", &[], &block));
}

#[test]
fn is_url_allowed_allow_list() {
    let allow = vec!["example.com".to_string()];
    assert!(is_url_allowed("https://example.com", &allow, &[]));
    assert!(is_url_allowed("https://sub.example.com", &allow, &[]));
    assert!(!is_url_allowed("https://reddit.com", &allow, &[]));
}

#[test]
fn is_url_allowed_block_beats_allow() {
    let allow = vec!["example.com".to_string()];
    let block = vec!["bad.example.com".to_string()];
    assert!(!is_url_allowed("https://bad.example.com", &allow, &block));
    assert!(is_url_allowed("https://good.example.com", &allow, &block));
}

#[test]
fn is_url_allowed_hostless_url_needs_no_allow_list() {
    let allow = vec!["example.com".to_string()];
    let block = vec!["reddit.com".to_string()];
    assert!(is_url_allowed("mailto:a@example.com", &[], &block));
    assert!(!is_url_allowed("mailto:a@example.com", &allow, &[]));
}

#[tokio::test]
async fn host_cap_serializes_excess_callers() {
    // Reset the semaphore for this host to avoid pollution from other tests.
    HOST_SEMAPHORES.remove("cap-test.invalid");

    // Hold-time chosen so that an unbounded run would finish well under
    // `PER_HOST_CONCURRENCY * hold`, while a capped run cannot.
    let hold = Duration::from_millis(40);
    let total = PER_HOST_CONCURRENCY + 4;

    let start = Instant::now();
    let mut handles = Vec::with_capacity(total);
    for _ in 0..total {
        handles.push(tokio::spawn(async move {
            let permit = acquire_host_permit(Some("cap-test.invalid"))
                .await
                .expect("permit must issue");
            tokio::time::sleep(hold).await;
            drop(permit);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    let elapsed = start.elapsed();

    // With a cap of N, (N + 4) tasks each holding for `hold` must take at
    // least 2 * hold (two "rounds"). Without the cap it'd be ~1 * hold.
    assert!(
        elapsed >= hold * 2,
        "per-host cap didn't serialize: elapsed={:?}, expected >= {:?}",
        elapsed,
        hold * 2
    );
}

#[tokio::test]
async fn host_cap_skipped_when_url_has_no_host() {
    let permit = acquire_host_permit(None).await;
    assert!(permit.is_none(), "no-host URLs should bypass the cap");
}

// ===== Side-door request builders (fetch/social.rs) =====

#[test]
fn reddit_json_url_appends_suffix() {
    assert_eq!(
        reddit_json_url("https://reddit.com/r/rust/comments/abc/title"),
        "https://reddit.com/r/rust/comments/abc/title.json"
    );
}

#[test]
fn oembed_request_url_per_platform() {
    let tiktok = oembed_request_url(&SocialParts {
        platform: SocialPlatform::TikTok,
        kind: Some(SocialKind::Video),
        id: Some("123".to_string()),
        handle: Some("@u".to_string()),
    })
    .expect("tiktok has oembed");
    assert!(tiktok.starts_with("https://www.tiktok.com/oembed?url="));
    assert!(tiktok.contains("format=json"));
    // Regression guard: the target is rebuilt WITH `www.` — TikTok oEmbed
    // 400s on a www-less host, which our `canonical` strips.
    assert!(
        tiktok.contains("https%3A%2F%2Fwww.tiktok.com%2F%40u%2Fvideo%2F123"),
        "expected www-qualified, field-rebuilt target, got {tiktok}"
    );

    // Rebuilt from the id (`youtu.be/<id>` → `www.youtube.com/watch?v=<id>`).
    let yt = oembed_request_url(&SocialParts {
        platform: SocialPlatform::YouTube,
        kind: Some(SocialKind::Video),
        id: Some("dQw4w9WgXcQ".to_string()),
        handle: None,
    })
    .expect("yt oembed");
    assert!(yt.contains("https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DdQw4w9WgXcQ"));

    // No unauthenticated oEmbed endpoint.
    assert!(oembed_request_url(&SocialParts {
        platform: SocialPlatform::Instagram,
        kind: None,
        id: None,
        handle: None,
    })
    .is_none());
}

// ===== Side-door routing (fetch/social.rs) =====
//
// `fetch_social` returns `None` before any network for URLs with no dedicated
// door, so the no-door routes test offline. The positive routes (Reddit
// `.json`, oEmbed) hit the network and are exercised by the live url tests.
// Routing reads only `platform` + `kind`, so the parts are built as literals —
// the fetch tool never depends on the extract tool, tests included.

use crate::types::CanonicalUrl;

/// `fetch_social` for a URL expected to have no dedicated door — must return
/// `None` without touching the network.
async fn no_door(platform: SocialPlatform, kind: Option<SocialKind>, url: &str) -> bool {
    let social = SocialParts {
        platform,
        kind,
        id: None,
        handle: None,
    };
    fetch_social(&social, &CanonicalUrl::new(url))
        .await
        .is_none()
}

#[tokio::test]
async fn fetch_social_no_door_for_profiles_and_boards() {
    // Profiles/feeds have no oEmbed; the caller fetches the page.
    assert!(
        no_door(
            SocialPlatform::TikTok,
            Some(SocialKind::Profile),
            "https://tiktok.com/@u"
        )
        .await
    );
    assert!(
        no_door(
            SocialPlatform::YouTube,
            Some(SocialKind::Profile),
            "https://youtube.com/@MrBeast"
        )
        .await
    );
    // oEmbed only serves individual pins, so a board has no door.
    assert!(
        no_door(
            SocialPlatform::Pinterest,
            Some(SocialKind::Collection),
            "https://pinterest.com/someuser/recipes"
        )
        .await
    );
}

#[tokio::test]
async fn fetch_social_no_door_for_doorless_platforms() {
    // Instagram has no public oEmbed/JSON door, so even a recognized post has
    // no door. (A non-social URL never reaches routing: it has no
    // `SocialParts`, so the callers scrape it directly.)
    assert!(
        no_door(
            SocialPlatform::Instagram,
            Some(SocialKind::Post),
            "https://instagram.com/p/Cabc"
        )
        .await
    );
}

#[tokio::test]
async fn fetch_social_no_door_for_none_kind_shortlink() {
    // An opaque TikTok shortener classifies with `kind: None` — the path is
    // a short code, not an addressable item. It must fall through to the page
    // fetch, NOT oEmbed (which needs an id/handle the shortlink lacks).
    assert!(
        no_door(
            SocialPlatform::TikTok,
            None,
            "https://vm.tiktok.com/ZMabc123"
        )
        .await
    );
}
