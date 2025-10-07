use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Random-ish jitter in milliseconds within [0, range).
pub(crate) fn jitter_ms(range: u64) -> u64 {
    if range == 0 {
        return 0;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_nanos(0));
    let nanos = now.subsec_nanos() as u64;
    let micros = (now.as_micros() & 0xFFFF) as u64;
    (nanos ^ (micros << 5)) % range
}

/// Check if HTTP response contains valid scrapable HTML content.
///
/// Returns false for:
/// - Non-success HTTP status codes
/// - Content too short (< 500 bytes)
/// - Non-HTML content
/// - Bot challenges (Cloudflare, PerimeterX, captcha)
/// - Access denied pages
///
/// This validation allows the fetch strategy to fallback to the next strategy
pub(crate) fn is_valid_response(status_code: reqwest::StatusCode, body: &str) -> bool {
    if !status_code.is_success() {
        return false;
    }

    if body.len() < 500 {
        return false;
    }

    let body_lower = body.to_ascii_lowercase();

    if !body_lower.contains("<html") && !body_lower.contains("<!doctype") {
        return false;
    }

    let access_denied_patterns = [
        "access denied",
        "permission denied",
        "forbidden",
        "unauthorized",
    ];

    for pattern in &access_denied_patterns {
        if body_lower.contains(pattern) {
            return false;
        }
    }

    let bot_challenge_patterns = [
        "verify you are a human",
        "please complete the captcha",
        "solve this captcha",
        "captcha challenge",
        "cf-browser-verification",
        "cf-captcha-container",
        "px-captcha",
        "blocked by cloudflare",
        "please enable javascript and cookies",
        "suspicious activity",
        "bot detection",
        "perimeterx",
    ];

    for pattern in &bot_challenge_patterns {
        if body_lower.contains(pattern) {
            return false;
        }
    }

    // Passed all checks
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn detects_cloudflare_challenge() {
        let html = r#"<html><body>Checking your browser before accessing... cf-browser-verification</body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn detects_cloudflare_captcha() {
        let html = r#"<html><body>Please complete the captcha to continue. cf-captcha-container</body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn detects_perimeter_x() {
        let html = r#"<html><body>PerimeterX robot detection blocking this request</body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn detects_generic_captcha() {
        let html =
            r#"<html><body>Please solve this captcha to verify you are a human</body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn detects_access_denied() {
        let html = r#"<html><head><title>Access Denied</title></head><body><h1>Access Denied</h1><p>Permission denied to access this resource</p></body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn accepts_normal_content() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Welcome to my site</h1><p>This is normal content with lots of text to meet the minimum length requirement. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.</p></body></html>"#;
        assert!(is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn rejects_non_success_status() {
        let html = r#"<!DOCTYPE html><html><body><h1>Page content</h1></body></html>"#;
        assert!(!is_valid_response(StatusCode::NOT_FOUND, html));
        assert!(!is_valid_response(StatusCode::INTERNAL_SERVER_ERROR, html));
        assert!(!is_valid_response(StatusCode::FORBIDDEN, html));
    }

    #[test]
    fn rejects_too_short_content() {
        let html = r#"<html><body>Short</body></html>"#;
        assert!(!is_valid_response(StatusCode::OK, html));
    }

    #[test]
    fn rejects_non_html_content() {
        let json = r#"{"status": "ok", "data": "This is JSON not HTML but has enough length to pass the minimum length check so we need more text here to make it realistic"}"#;
        assert!(!is_valid_response(StatusCode::OK, json));
    }

    #[test]
    fn jitter_returns_within_range() {
        for _ in 0..100 {
            let result = jitter_ms(100);
            assert!(
                result < 100,
                "jitter_ms returned {}, expected < 100",
                result
            );
        }
    }

    #[test]
    fn jitter_zero_range_returns_zero() {
        assert_eq!(jitter_ms(0), 0);
    }
}
