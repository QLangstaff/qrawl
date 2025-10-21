use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Random-ish jitter in milliseconds within [0, range).
///
/// Uses high-resolution timing to generate pseudo-random jitter for
/// introducing variability in retry delays and request timing.
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

/// Check if response body is invalid HTML content.
///
/// Returns Some(reason) if invalid:
/// - Body too short (< 500 bytes)
/// - Missing HTML markers (<html or <!doctype)
pub(crate) fn is_invalid(body: &str) -> Option<&'static str> {
    if body.len() < 500 {
        return Some("body is too short");
    }

    let body_lower = body.to_ascii_lowercase();
    if !body_lower.contains("<html") && !body_lower.contains("<!doctype") {
        return Some("missing HTML markers");
    }

    None
}

/// Check if response indicates access denied or unauthorized.
///
/// Returns Some(pattern) if unauthorized patterns detected.
pub(crate) fn is_unauthorized(body: &str) -> Option<&'static str> {
    let body_lower = body.to_ascii_lowercase();

    let patterns = [
        "access denied",
        "permission denied",
        "forbidden",
        "unauthorized",
    ];

    patterns
        .iter()
        .copied()
        .find(|pattern| body_lower.contains(pattern))
}

/// Check if response contains bot challenge patterns.
///
/// Returns Some(pattern) if bot detection patterns found.
pub(crate) fn is_suspicious(body: &str) -> Option<&'static str> {
    let body_lower = body.to_ascii_lowercase();

    let patterns = [
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

    patterns
        .iter()
        .copied()
        .find(|pattern| body_lower.contains(pattern))
}

/// Validate HTTP response for scrapable content.
///
/// Returns Ok(()) if valid, Err(reason) if invalid.
/// Checks for:
/// - Non-success status codes
/// - Invalid HTML content
/// - Access denied patterns (skipped if JSON-LD present)
/// - Bot challenge patterns (skipped if JSON-LD present)
///
/// Pages with JSON-LD structured data are considered valid even if they
/// contain "forbidden" or "unauthorized" text, since recipe sites often
/// have such text in unrelated page elements.
pub(crate) fn validate_response(
    status_code: reqwest::StatusCode,
    body: &str,
) -> Result<(), String> {
    if !status_code.is_success() {
        if status_code == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(format!("rate limited ({})", status_code.as_u16()));
        }
        if status_code == reqwest::StatusCode::FORBIDDEN {
            return Err(format!("status {} (forbidden)", status_code.as_u16()));
        }
        if status_code == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("status {} (not found)", status_code.as_u16()));
        }
        if status_code == reqwest::StatusCode::UNAUTHORIZED {
            return Err(format!("status {} (unauthorized)", status_code.as_u16()));
        }
        if status_code == reqwest::StatusCode::BAD_REQUEST {
            return Err(format!("status {} (bad request)", status_code.as_u16()));
        }
        if status_code == reqwest::StatusCode::INTERNAL_SERVER_ERROR {
            return Err(format!("status {} (server error)", status_code.as_u16()));
        }
        return Err(format!("status {} (unknown error)", status_code.as_u16()));
    }

    // If page has JSON-LD structured data, accept it
    if body.contains("application/ld+json") {
        if let Some(reason) = is_invalid(body) {
            return Err(format!("invalid - {}", reason));
        }
        return Ok(());
    }

    // Strict validation for pages without JSON-LD
    if let Some(reason) = is_invalid(body) {
        return Err(format!("invalid - {}", reason));
    }

    if let Some(pattern) = is_unauthorized(body) {
        return Err(format!("unauthorized - {}", pattern));
    }

    if let Some(pattern) = is_suspicious(body) {
        return Err(format!("suspicious - {}", pattern));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

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

    #[test]
    fn test_is_suspicious_cloudflare_challenge() {
        let html = r#"<html><body>Checking your browser before accessing... cf-browser-verification</body></html>"#;
        assert_eq!(is_suspicious(html), Some("cf-browser-verification"));
    }

    #[test]
    fn test_is_suspicious_cloudflare_captcha() {
        let html = r#"<html><body>Please complete the captcha to continue. cf-captcha-container</body></html>"#;
        assert_eq!(is_suspicious(html), Some("please complete the captcha"));
    }

    #[test]
    fn test_is_suspicious_perimeter_x() {
        let html = r#"<html><body>PerimeterX robot detection blocking this request</body></html>"#;
        assert_eq!(is_suspicious(html), Some("bot detection"));
    }

    #[test]
    fn test_is_suspicious_generic_captcha() {
        let html =
            r#"<html><body>Please solve this captcha to verify you are a human</body></html>"#;
        assert_eq!(is_suspicious(html), Some("verify you are a human"));
    }

    #[test]
    fn test_is_unauthorized_access_denied() {
        let html = r#"<html><head><title>Access Denied</title></head><body><h1>Access Denied</h1><p>Permission denied to access this resource</p></body></html>"#;
        assert_eq!(is_unauthorized(html), Some("access denied"));
    }

    #[test]
    fn test_validate_response_normal_content() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Welcome to my site</h1><p>This is normal content with lots of text to meet the minimum length requirement. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.</p></body></html>"#;
        assert!(validate_response(StatusCode::OK, html).is_ok());
    }

    #[test]
    fn test_validate_response_non_success_status() {
        let html = r#"<!DOCTYPE html><html><body><h1>Page content</h1></body></html>"#;
        assert!(validate_response(StatusCode::NOT_FOUND, html).is_err());
        assert!(validate_response(StatusCode::INTERNAL_SERVER_ERROR, html).is_err());
        assert!(validate_response(StatusCode::FORBIDDEN, html).is_err());
    }

    #[test]
    fn test_is_too_short() {
        let html = r#"<html><body>Short</body></html>"#;
        assert_eq!(is_invalid(html), Some("body is too short"));
    }

    #[test]
    fn test_is_non_html_content() {
        let json = r#"{"status": "ok", "data": "This is JSON not HTML but has enough length to pass the minimum length check so we need more text here to make it realistic. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore. And some additional text to ensure we exceed 500 bytes."}"#;
        assert_eq!(is_invalid(json), Some("missing HTML markers"));
    }
}
