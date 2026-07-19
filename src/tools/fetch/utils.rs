use crate::errors::QrawlError;

const MIN_BODY_LEN: usize = 500;

/// Bot-challenge markers — vendor / challenge-page-specific strings that a 200-OK
/// *soft* block carries but ordinary page text does not. Looser phrases ("bot
/// detection", "suspicious activity", "please enable javascript and cookies") are
/// deliberately excluded: they appear in legitimate content and caused false
/// rejections.
const SUSPICIOUS_PATTERNS: [&str; 9] = [
    "verify you are a human",
    "please complete the captcha",
    "solve this captcha",
    "captcha challenge",
    "cf-browser-verification",
    "cf-captcha-container",
    "px-captcha",
    "blocked by cloudflare",
    "perimeterx",
];

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

fn is_suspicious_cached<'a>(body: &'a str, cache: &'a mut Option<String>) -> Option<&'static str> {
    let lower = ensure_lower(body, cache);
    SUSPICIOUS_PATTERNS
        .iter()
        .copied()
        .find(|pattern| lower.contains(pattern))
}

/// Validate that an HTTP response carries scrapable HTML.
///
/// **Domain-agnostic**: it judges whether the bytes are fetchable HTML, not
/// whether they are any particular *kind* of content — interpreting the page (a
/// recipe, a profile, …) is the caller's job.
///
/// Rejects:
/// - a non-2xx status — the deterministic signal for auth / rate blocks, 404s and
///   5xx; the Auto cascade uses this to fall through to the next profile;
/// - a body too short to be a page, or missing `<html>` / `<!doctype>` markers;
/// - a body carrying a bot-challenge marker (Cloudflare / PerimeterX / a captcha
///   wall), so a 200-OK *soft* block is caught and the cascade retries. These
///   markers are challenge-specific, so they don't fire on ordinary page text.
///
/// It does **not** sniff for "forbidden" / "access denied" words: those appear in
/// legitimate page content, and a real auth block comes with a 401/403 status,
/// caught above.
pub(super) fn validate_response(
    status_code: reqwest::StatusCode,
    body: &str,
) -> Result<(), QrawlError> {
    if !status_code.is_success() {
        return Err(QrawlError::new(format!(
            "HTTP status {}",
            status_code.as_u16()
        )));
    }

    let mut body_lower_cache = None;
    if let Some(reason) = is_invalid_cached(body, &mut body_lower_cache) {
        return Err(QrawlError::new(format!("invalid content - {}", reason)));
    }
    if let Some(pattern) = is_suspicious_cached(body, &mut body_lower_cache) {
        return Err(QrawlError::new(format!("suspicious content - {}", pattern)));
    }

    Ok(())
}
