use crate::errors::QrawlError;
use crate::types::with_context;
use crate::utils::{canonical_domain, host_matches, host_of, listed_domain};

// ---------------------------------------------------------------------------
// Domain filtering
// ---------------------------------------------------------------------------

/// Whether `url`'s host passes the allow/block lists. An empty list filters
/// nothing; a blocked host loses even when it's also allowed; a URL with no
/// host passes only when there's no allowlist. Hosts and listed domains
/// compare in [`canonical_domain`] form, and an entry is read by
/// [`listed_domain`], so `WWW.Example.com`, `*.example.com`, and
/// `https://example.com:8080/a` all list `example.com`.
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
            .any(|listed| host_matches(&host, &listed_domain(listed)))
    };
    !matches_any(block_domains) && (allow_domains.is_empty() || matches_any(allow_domains))
}

/// Enforce the enclosing `Context`'s domain lists on a URL about to be
/// requested: the one given, or one a redirect points to. Outside a
/// `Context::scope` nothing is filtered.
pub(super) fn check_domain_filter(url: &str) -> Result<(), QrawlError> {
    let is_allowed =
        with_context(|context| is_url_allowed(url, &context.allow_domains, &context.block_domains))
            .unwrap_or(true);
    if is_allowed {
        Ok(())
    } else {
        Err(QrawlError::BlockedDomain(url.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Response validation
// ---------------------------------------------------------------------------

const MIN_BODY_LEN: usize = 500;

/// Bot-challenge markers: strings a 200-OK *soft* block carries in its
/// challenge markup. A vendor's name alone (`perimeterx`) isn't one — its
/// sensor script loads on ordinary pages.
const SUSPICIOUS_PATTERNS: [&str; 8] = [
    "verify you are a human",
    "please complete the captcha",
    "solve this captcha",
    "captcha challenge",
    "cf-browser-verification",
    "cf-captcha-container",
    "px-captcha",
    "blocked by cloudflare",
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
        return Err(QrawlError::NotHtml("body is too short"));
    }
    let lower = body.to_ascii_lowercase();
    if !lower.contains("<html") && !lower.contains("<!doctype") {
        return Err(QrawlError::NotHtml("missing HTML markers"));
    }
    if let Some(pattern) = SUSPICIOUS_PATTERNS
        .iter()
        .find(|pattern| lower.contains(**pattern))
    {
        return Err(QrawlError::BotChallenge(pattern));
    }
    Ok(())
}

/// Path markers of a consent interstitial: the page a site redirects a visitor
/// without its consent cookie to, in place of the page asked for. No list
/// names every one; a wall at another path, or served at the address asked
/// for, is read as the page.
const CONSENT_PATH_MARKERS: [&str; 1] = [
    // derstandard.at answers `/story/<id>/<slug>` with a 302 to
    // `/consent/tcf/story/<id>/<slug>` (seen 2026-10-11). Its IAB TCF markup
    // (`__tcfapi`) is no marker: the article carries it too.
    "/consent/tcf/",
];

/// Reject a response a redirect took to a consent interstitial: `served`'s
/// path has a [`CONSENT_PATH_MARKERS`] entry that `requested`'s doesn't.
pub(super) fn check_consent_interstitial(
    requested: &str,
    served: &reqwest::Url,
) -> Result<(), QrawlError> {
    let requested_path = reqwest::Url::parse(requested)
        .map(|requested| requested.path().to_string())
        .unwrap_or_default();
    let is_interstitial = CONSENT_PATH_MARKERS
        .iter()
        .any(|marker| served.path().contains(marker) && !requested_path.contains(marker));
    if is_interstitial {
        Err(QrawlError::ConsentInterstitial(served.to_string()))
    } else {
        Ok(())
    }
}

/// Reject a non-2xx status.
pub(super) fn check_status(status_code: reqwest::StatusCode) -> Result<(), QrawlError> {
    if status_code.is_success() {
        Ok(())
    } else {
        Err(QrawlError::Status(status_code.as_u16()))
    }
}
