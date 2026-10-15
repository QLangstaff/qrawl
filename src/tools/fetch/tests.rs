#![cfg(test)]
use crate::errors::QrawlError;
use crate::tools::fetch::embed::{embed_request, oembed_request_url, reddit_json_url};
use crate::tools::fetch::fetch_strategy;
use crate::tools::fetch::headers::headers_for_profile;
use crate::tools::fetch::strategies::{
    acquire_host_permit, attempt_order, HOST_SEMAPHORES, PER_HOST_CONCURRENCY,
};
use crate::tools::fetch::types::FetchProfile;
use crate::tools::fetch::utils::{is_url_allowed, validate_response};
use crate::types::{
    CanonicalDomain, CanonicalUrl, Context, SocialKind, SocialParts, SocialPlatform,
};
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
            .and_then(|value| value.to_str().ok()),
        Some("\"Windows\"")
    );
    assert_eq!(
        headers
            .get("sec-ch-ua-mobile")
            .and_then(|value| value.to_str().ok()),
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
    let error = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(error.to_string().contains("suspicious"));
    assert!(error.to_string().contains("cf-browser-verification"));
}

#[test]
fn is_suspicious_cloudflare_captcha() {
    let html = padded_html("Please complete the captcha to continue. cf-captcha-container");
    let error = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(error.to_string().contains("suspicious"));
    assert!(error.to_string().contains("please complete the captcha"));
}

#[test]
fn is_suspicious_perimeter_x_challenge() {
    let html = padded_html(r#"<div id="px-captcha"></div>"#);
    let error = validate_response(StatusCode::OK, &html).unwrap_err();
    assert_eq!(error, crate::errors::QrawlError::BotChallenge("px-captcha"));
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
    let error = validate_response(StatusCode::OK, &html).unwrap_err();
    assert!(error.to_string().contains("suspicious"));
    assert!(error.to_string().contains("verify you are a human"));
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
    let error = validate_response(StatusCode::OK, html).unwrap_err();
    assert!(error.to_string().contains("body is too short"));
}

#[test]
fn detect_non_html_content() {
    let json = r#"{"status": "ok", "data": "This is JSON not HTML but has enough length to pass the minimum length check so we need more text here to make it realistic. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore. And some additional text to ensure we exceed 500 bytes."}"#;
    let error = validate_response(StatusCode::OK, json).unwrap_err();
    assert!(error.to_string().contains("missing HTML markers"));
}

#[test]
fn has_domain_is_true_for_the_domain_and_its_subdomains() {
    let reddit = CanonicalDomain::new("reddit.com").unwrap();
    let has_domain = |url: &str| CanonicalUrl::new(url).unwrap().has_domain(&reddit);
    assert!(has_domain("https://reddit.com/"));
    assert!(has_domain("https://old.reddit.com/"));
    assert!(has_domain("https://a.b.reddit.com/"));
    assert!(!has_domain("https://notreddit.com/"));
    assert!(!has_domain("https://reddit.com.evil.com/"));
    assert!(!has_domain("https://reddit.co/"));
}

#[test]
fn is_url_allowed_block_list() {
    let block = vec![
        CanonicalDomain::new("reddit.com").unwrap(),
        CanonicalDomain::new("tiktok.com").unwrap(),
    ];
    assert!(!is_allowed("https://reddit.com/r/rust", &[], &block));
    assert!(!is_allowed("https://old.reddit.com/r/rust", &[], &block));
    assert!(is_allowed("https://example.com", &[], &block));
}

#[test]
fn is_url_allowed_allow_list() {
    let allow = vec![CanonicalDomain::new("example.com").unwrap()];
    assert!(is_allowed("https://example.com", &allow, &[]));
    assert!(is_allowed("https://sub.example.com", &allow, &[]));
    assert!(!is_allowed("https://reddit.com", &allow, &[]));
}

#[test]
fn is_url_allowed_block_beats_allow() {
    let allow = vec![CanonicalDomain::new("example.com").unwrap()];
    let block = vec![CanonicalDomain::new("bad.example.com").unwrap()];
    assert!(!is_allowed("https://bad.example.com", &allow, &block));
    assert!(is_allowed("https://good.example.com", &allow, &block));
}

#[test]
fn is_url_allowed_ignores_the_case_and_www_of_a_host() {
    let listed = vec![CanonicalDomain::new("Example.com").unwrap()];
    assert!(is_allowed("https://example.com/x", &listed, &[]));
    assert!(!is_allowed("https://WWW.Example.com/x", &[], &listed));
    assert!(!is_allowed("https://other.com/x", &listed, &[]));
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
            let permit = acquire_host_permit("cap-test.invalid")
                .await
                .expect("permit must issue");
            tokio::time::sleep(hold).await;
            drop(permit);
        }));
    }
    for handle in handles {
        handle.await.unwrap();
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
async fn a_host_and_its_www_form_share_one_permit() {
    // The fetch fails (`.invalid` never resolves), after it took its permit.
    let _ = Context::fast()
        .scope(fetch_strategy("http://www.permit-key.invalid/"))
        .await;
    assert!(HOST_SEMAPHORES.contains_key("permit-key.invalid"));
    assert!(!HOST_SEMAPHORES.contains_key("www.permit-key.invalid"));
}

// ===== Embed request builders (fetch/embed.rs) =====

#[test]
fn reddit_json_url_appends_suffix() {
    assert_eq!(
        reddit_json_url("https://reddit.com/r/rust/comments/abc/title"),
        "https://reddit.com/r/rust/comments/abc/title.json?raw_json=1"
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
    // TikTok oEmbed 400s on a www-less host, so the target carries `www.`.
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
/// Whether a URL string passes the lists.
fn is_allowed(
    url: &str,
    allow_domains: &[CanonicalDomain],
    block_domains: &[CanonicalDomain],
) -> bool {
    is_url_allowed(
        &CanonicalUrl::new(url).expect("valid URL"),
        allow_domains,
        block_domains,
    )
}

fn embed_request_url(url: &str) -> Option<String> {
    CanonicalUrl::new(url)
        .and_then(|url| embed_request(&url))
        .map(|(request, _embed)| request)
}

#[test]
fn embed_request_for_an_item_addresses_its_platform() {
    assert_eq!(
        embed_request_url("https://reddit.com/r/rust/comments/abc/title").as_deref(),
        Some("https://reddit.com/r/rust/comments/abc/title.json?raw_json=1")
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
    let block = vec![CanonicalDomain::new("example.com").unwrap()];
    assert!(!is_allowed("https://example.com./a", &[], &block));
}

#[test]
fn a_domain_entry_written_another_way_is_refused() {
    for entry in [
        "example.com:8080",
        "example.com.",
        " example.com",
        "*.example.com",
        "example.com/path",
        "https://www.example.com/a",
    ] {
        assert_eq!(
            CanonicalDomain::new(entry),
            Err(QrawlError::InvalidDomain(entry.to_string()))
        );
    }
}

#[test]
fn canonical_domain_new_accepts_a_domain_name() {
    for entry in [
        "reddit.com",
        "old.reddit.com",
        "my-site.com",
        "127.0.0.1",
        "localhost",
        "www.com",
    ] {
        assert_eq!(CanonicalDomain::new(entry).unwrap().as_str(), entry);
    }
    assert_eq!(
        CanonicalDomain::new("Reddit.COM").unwrap().as_str(),
        "reddit.com"
    );
    assert_eq!(
        CanonicalDomain::new("münchen.de").unwrap().as_str(),
        "xn--mnchen-3ya.de"
    );
    assert_eq!(
        CanonicalDomain::new("MÜNCHEN.DE").unwrap().as_str(),
        "xn--mnchen-3ya.de"
    );
}

#[test]
fn canonical_domain_new_refuses_a_host_no_canonical_url_has() {
    // A `www.` or mobile social host, and a number the `url` crate rewrites
    // as an IPv4 address (`1` → `0.0.0.1`, `1.2.3` → `1.2.0.3`).
    for entry in [
        "www.reddit.com",
        "m.facebook.com",
        "mobile.twitter.com",
        "1",
        "123",
        "1.2.3",
    ] {
        assert_eq!(
            CanonicalDomain::new(entry),
            Err(QrawlError::InvalidDomain(entry.to_string()))
        );
    }
    for entry in [
        "127.0.0.1",
        "m.example.com",
        "old.reddit.com",
        "vm.tiktok.com",
        "www.com",
    ] {
        assert_eq!(CanonicalDomain::new(entry).unwrap().as_str(), entry);
    }
}

#[test]
fn canonical_domain_new_refuses_what_is_not_a_domain_name() {
    for entry in [
        "www.reddit.com",
        "*.reddit.com",
        "reddit.com:8080",
        "reddit.com/r/rust",
        "https://reddit.com/r/rust",
        "reddit.com.",
        " reddit.com",
        "",
        "not a domain!",
        // Letters, digits and hyphens, but no Punycode a name decodes from.
        "xn--a.com",
    ] {
        assert_eq!(
            CanonicalDomain::new(entry),
            Err(QrawlError::InvalidDomain(entry.to_string()))
        );
    }
}

#[test]
fn a_www_tld_entry_does_not_list_every_host_of_the_tld() {
    let block = vec![CanonicalDomain::new("www.com").unwrap()];
    assert!(!is_allowed("https://www.com/x", &[], &block));
    assert!(is_allowed("https://example.com/x", &[], &block));
}

#[tokio::test]
async fn fetch_refuses_what_is_not_an_http_url_before_any_request() {
    for url in ["not a url", "mailto:a@b.com", "example.com"] {
        assert_eq!(
            crate::tools::fetch::fetch_auto(url).await.unwrap_err(),
            crate::errors::QrawlError::InvalidUrl(url.to_string())
        );
    }
}

#[test]
fn reddit_json_url_drops_the_query_and_fragment() {
    assert_eq!(
        reddit_json_url("https://reddit.com/r/rust/comments/abc/title/?share=1#frag"),
        "https://reddit.com/r/rust/comments/abc/title.json?raw_json=1"
    );
}
