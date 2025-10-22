use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MIN_BODY_LEN: usize = 500;

const UNAUTHORIZED_PATTERNS: [&str; 4] = [
    "access denied",
    "permission denied",
    "forbidden",
    "unauthorized",
];

const SUSPICIOUS_PATTERNS: [&str; 12] = [
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

/// Random-ish jitter in milliseconds within [0, range).
///
/// Uses high-resolution timing to generate pseudo-random jitter for
/// introducing variability in retry delays and request timing.
pub(super) fn jitter_ms(range: u64) -> u64 {
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

fn ensure_lower<'a>(body: &'a str, cache: &'a mut Option<String>) -> &'a str {
    if let Some(ref lower) = cache {
        lower.as_str()
    } else {
        *cache = Some(body.to_ascii_lowercase());
        cache.as_ref().unwrap()
    }
}

fn is_invalid_cached<'a>(body: &'a str, cache: &'a mut Option<String>) -> Option<&'static str> {
    if body.len() < MIN_BODY_LEN {
        return Some("body is too short");
    }

    let lower = ensure_lower(body, cache);
    if !lower.contains("<html") && !lower.contains("<!doctype") {
        return Some("missing HTML markers");
    }

    None
}

fn is_unauthorized_cached<'a>(
    body: &'a str,
    cache: &'a mut Option<String>,
) -> Option<&'static str> {
    let lower = ensure_lower(body, cache);
    UNAUTHORIZED_PATTERNS
        .iter()
        .copied()
        .find(|pattern| lower.contains(pattern))
}

fn is_suspicious_cached<'a>(body: &'a str, cache: &'a mut Option<String>) -> Option<&'static str> {
    let lower = ensure_lower(body, cache);
    SUSPICIOUS_PATTERNS
        .iter()
        .copied()
        .find(|pattern| lower.contains(pattern))
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
pub(super) fn validate_response(
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

    let mut body_lower_cache = None;

    // If page has JSON-LD structured data, accept it
    if body.contains("application/ld+json") {
        if let Some(reason) = is_invalid_cached(body, &mut body_lower_cache) {
            return Err(format!("invalid - {}", reason));
        }
        return Ok(());
    }

    // Strict validation for pages without JSON-LD
    if let Some(reason) = is_invalid_cached(body, &mut body_lower_cache) {
        return Err(format!("invalid - {}", reason));
    }

    if let Some(pattern) = is_unauthorized_cached(body, &mut body_lower_cache) {
        return Err(format!("unauthorized - {}", pattern));
    }

    if let Some(pattern) = is_suspicious_cached(body, &mut body_lower_cache) {
        return Err(format!("suspicious - {}", pattern));
    }

    Ok(())
}
