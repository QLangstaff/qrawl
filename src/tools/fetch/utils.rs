use crate::errors::QrawlError;
use crate::types::CTX;

// ---------------------------------------------------------------------------
// Domain filtering
// ---------------------------------------------------------------------------

pub(super) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

pub(super) fn is_host_allowed(
    host: &str,
    allow: Option<&[String]>,
    block: Option<&[String]>,
) -> bool {
    if let Some(block) = block {
        if block.iter().any(|d| host_matches(host, d)) {
            return false;
        }
    }
    if let Some(allow) = allow {
        if !allow.iter().any(|d| host_matches(host, d)) {
            return false;
        }
    }
    true
}

/// Whether `url`'s host passes the given allow/block lists — a prefetch check
/// that does NOT read the task-local `Context`, so crawl orchestration can drop
/// URLs *before* dispatching a fetch. With no lists, everything passes; an
/// unparseable host passes only when there's no allowlist (mirrors
/// [`check_domain_filter`]).
pub(crate) fn is_url_allowed(
    url: &str,
    allow: Option<&[String]>,
    block: Option<&[String]>,
) -> bool {
    if allow.is_none() && block.is_none() {
        return true;
    }
    match url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
    {
        Some(host) => is_host_allowed(&host, allow, block),
        None => allow.is_none(),
    }
}

/// Enforce the task-local `Context`'s allow/block lists for a fetch about to
/// dispatch. Unlike [`is_url_allowed`], this reads `CTX` and returns a
/// descriptive error so the fetch leaves can short-circuit.
pub(super) fn check_domain_filter(url: &str) -> Result<(), QrawlError> {
    let filters = CTX
        .try_with(|ctx| (ctx.allow_domains.clone(), ctx.block_domains.clone()))
        .ok();
    let (allow, block) = match filters {
        Some((a, b)) if !a.is_empty() || !b.is_empty() => (a, b),
        _ => return Ok(()),
    };
    let allow_d = (!allow.is_empty()).then_some(allow.as_slice());
    let block_d = (!block.is_empty()).then_some(block.as_slice());
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()));
    match host {
        Some(h) if !is_host_allowed(&h, allow_d, block_d) => {
            Err(QrawlError::new(format!("blocked by domain filter: {h}")))
        }
        Some(_) => Ok(()),
        None if !allow.is_empty() => Err(QrawlError::new(format!(
            "blocked by domain filter: unparseable host in {url}"
        ))),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Response validation
// ---------------------------------------------------------------------------

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
