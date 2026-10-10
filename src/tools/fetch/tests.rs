#![cfg(test)]
use crate::tools::fetch::embed::{embed_request, oembed_request_url, reddit_json_url};
use crate::tools::fetch::headers::headers_for_profile;
use crate::tools::fetch::strategies::{
    acquire_host_permit, attempt_order, HOST_SEMAPHORES, PER_HOST_CONCURRENCY,
};
use crate::tools::fetch::types::FetchProfile;
use crate::tools::fetch::utils::{is_url_allowed, validate_response};
use crate::types::{SocialKind, SocialParts, SocialPlatform};
use crate::utils::host_matches;
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
    let headers = headers_for_profile(FetchProfile::Ios);
    assert!(headers.contains_key("accept"));
    assert!(!headers.contains_key("sec-ch-ua"));
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
fn is_suspicious_perimeter_x_challenge() {
    let html = padded_html(r#"<div id="px-captcha"></div>"#);
    let err = validate_response(StatusCode::OK, &html).unwrap_err();
    assert_eq!(err, crate::errors::QrawlError::BotChallenge("px-captcha"));
}

#[test]
fn a_page_that_loads_the_perimeterx_sensor_is_not_a_challenge() {
    let html =
        padded_html(r#"<script src="https://client.perimeterx.net/PX123/main.min.js"></script>"#);
    assert!(validate_response(StatusCode::OK, &html).is_ok());
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

#[test]
fn is_url_allowed_ignores_case_and_www() {
    let listed = vec!["WWW.Example.com".to_string()];
    assert!(is_url_allowed("https://example.com/x", &listed, &[]));
    assert!(!is_url_allowed("https://www.example.com/x", &[], &listed));
    assert!(!is_url_allowed("https://other.com/x", &listed, &[]));
}

#[test]
fn attempt_order_moves_cached_profile_first_and_keeps_the_rest() {
    let profiles = [
        FetchProfile::Minimal,
        FetchProfile::Windows,
        FetchProfile::Ios,
    ];
    assert_eq!(attempt_order(&profiles, None), profiles);
    assert_eq!(
        attempt_order(&profiles, Some(FetchProfile::Ios)),
        [
            FetchProfile::Ios,
            FetchProfile::Minimal,
            FetchProfile::Windows
        ]
    );
    assert_eq!(
        attempt_order(&[FetchProfile::Minimal], Some(FetchProfile::Windows)),
        [FetchProfile::Minimal]
    );
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

// ===== Embed request builders (fetch/embed.rs) =====

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
fn embed_request_url(url: &str) -> Option<String> {
    embed_request(url).map(|(request, _embed)| request)
}

#[test]
fn embed_request_for_an_item_addresses_its_platform() {
    assert_eq!(
        embed_request_url("https://reddit.com/r/rust/comments/abc/title").as_deref(),
        Some("https://reddit.com/r/rust/comments/abc/title.json")
    );
    for (url, endpoint) in [
        (
            "https://tiktok.com/@u/video/123",
            "https://www.tiktok.com/oembed?",
        ),
        (
            "https://youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/oembed?",
        ),
        (
            "https://pinterest.com/pin/123",
            "https://www.pinterest.com/oembed.json?",
        ),
        (
            "https://vimeo.com/123",
            "https://vimeo.com/api/oembed.json?",
        ),
    ] {
        let request = embed_request_url(url).expect(url);
        assert!(request.starts_with(endpoint), "{request}");
    }
}

#[test]
fn no_embed_request_for_listings_profiles_shortlinks_and_other_hosts() {
    for url in [
        // A listing's payload would describe one of its items.
        "https://reddit.com/r/rust",
        "https://reddit.com/user/someone",
        "https://tiktok.com/@u",
        "https://youtube.com/@NASA",
        "https://pinterest.com/user/board",
        // An opaque shortlink has no ids to address.
        "https://vm.tiktok.com/ZMabcdef",
        // No embed without a token.
        "https://instagram.com/p/Cabc",
        "https://facebook.com/zuck/posts/1",
        "https://x.com/jack/status/20",
        "https://linkedin.com/in/someone",
        // Not social.
        "https://example.com/recipe",
    ] {
        assert_eq!(embed_request_url(url), None, "{url}");
    }
}

#[test]
fn is_url_allowed_blocks_a_fully_qualified_host() {
    let block = vec!["example.com".to_string()];
    assert!(!is_url_allowed("https://example.com./a", &[], &block));
}
