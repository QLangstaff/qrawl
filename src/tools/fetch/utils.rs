use crate::errors::QrawlError;
use crate::types::with_context;
use crate::utils::{canonical_domain, host_matches, host_of};

// ---------------------------------------------------------------------------
// Domain filtering
// ---------------------------------------------------------------------------

/// Whether `url`'s host passes the allow/block lists. An empty list filters
/// nothing; a blocked host loses even when it's also allowed; a URL with no
/// host passes only when there's no allowlist. Hosts and listed domains
/// compare in [`canonical_domain`] form, so `WWW.Example.com` lists
/// `example.com`.
pub(super) fn is_url_allowed(
    url: &str,
    allow_domains: &[String],
    block_domains: &[String],
) -> bool {
    if allow_domains.is_empty() && block_domains.is_empty() {
        return true;
    }
    let Some(host) = host_of(url) else {
        return allow_domains.is_empty();
    };
    let host = canonical_domain(&host);
    let matches_any = |domains: &[String]| {
        domains
            .iter()
            .any(|domain| host_matches(&host, &canonical_domain(domain)))
    };
    !matches_any(block_domains) && (allow_domains.is_empty() || matches_any(allow_domains))
}

/// Enforce the enclosing `Context`'s domain lists on a URL about to be fetched,
/// or one a redirect landed on. Outside a `Context::scope` nothing is filtered.
pub(super) fn check_domain_filter(url: &str) -> Result<(), QrawlError> {
    let is_allowed =
        with_context(|ctx| is_url_allowed(url, &ctx.allow_domains, &ctx.block_domains))
            .unwrap_or(true);
    if is_allowed {
        Ok(())
    } else {
        Err(QrawlError::new(format!("blocked by domain filter: {url}")))
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
    check_status(status_code)?;
    if body.len() < MIN_BODY_LEN {
        return Err(QrawlError::new("invalid content - body is too short"));
    }
    let lower = body.to_ascii_lowercase();
    if !lower.contains("<html") && !lower.contains("<!doctype") {
        return Err(QrawlError::new("invalid content - missing HTML markers"));
    }
    if let Some(pattern) = SUSPICIOUS_PATTERNS
        .iter()
        .find(|pattern| lower.contains(**pattern))
    {
        return Err(QrawlError::new(format!("suspicious content - {}", pattern)));
    }
    Ok(())
}

/// Reject a non-2xx status.
pub(super) fn check_status(status_code: reqwest::StatusCode) -> Result<(), QrawlError> {
    if status_code.is_success() {
        Ok(())
    } else {
        Err(QrawlError::new(format!(
            "HTTP status {}",
            status_code.as_u16()
        )))
    }
}
