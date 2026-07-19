//! Fetch Tools

mod client;
mod headers;
pub mod profile;
pub mod strategies;
mod utils;

mod tests;
pub mod types;

pub use types::*;

use crate::errors::QrawlError;
use crate::types::{
    fetch_cache_get, fetch_cache_put, get_fetch_strategy, FetchStrategy, Html, CTX,
};

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn is_host_allowed(host: &str, allow: Option<&[String]>, block: Option<&[String]>) -> bool {
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

fn check_domain_filter(url: &str) -> Result<(), QrawlError> {
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

/// Fetch with fast strategy
pub async fn fetch_fast(url: &str) -> Result<Html, QrawlError> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(Html::new(cached));
    }
    let html = strategies::fetch_fast_with_client(url)
        .await
        .map(|r| r.html)?;
    fetch_cache_put(url, &html);
    Ok(Html::new(html))
}

/// Fetch with auto strategy
pub async fn fetch_auto(url: &str) -> Result<Html, QrawlError> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(Html::new(cached));
    }
    let html = strategies::fetch_auto_with_client(url)
        .await
        .map(|r| r.html)?;
    fetch_cache_put(url, &html);
    Ok(Html::new(html))
}

/// Fetch with auto strategy, returning full result with metadata.
pub async fn fetch_auto_with_result(url: &str) -> Result<FetchResult, QrawlError> {
    strategies::fetch_auto_with_client(url).await
}

pub async fn fetch_strategy(url: &str) -> Result<Html, QrawlError> {
    match get_fetch_strategy() {
        FetchStrategy::Fast => fetch_fast(url).await,
        FetchStrategy::Auto => fetch_auto(url).await,
    }
}

/// Fetch raw bytes (images, PDFs, other binary content) using same strategy
pub async fn fetch_bytes(url: &str, referer: Option<&str>) -> Result<Vec<u8>, QrawlError> {
    check_domain_filter(url)?;
    match get_fetch_strategy() {
        FetchStrategy::Fast => strategies::fetch_bytes_fast_with_client(url, referer).await,
        FetchStrategy::Auto => strategies::fetch_bytes_auto_with_client(url, referer).await,
    }
}
