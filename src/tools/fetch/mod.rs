//! Fetch Tools

mod client;
mod headers;
pub mod profile;
pub mod strategies;
mod utils;

mod tests;
pub mod types;

pub use types::*;

use crate::types::{fetch_cache_get, fetch_cache_put, get_fetch_strategy, FetchStrategy, CTX};

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn is_host_allowed(
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

fn check_domain_filter(url: &str) -> Result<(), String> {
    let filters = CTX
        .try_with(|ctx| (ctx.allow_domains.clone(), ctx.block_domains.clone()))
        .ok();
    let (allow, block) = match filters {
        Some((a, b)) if a.is_some() || b.is_some() => (a, b),
        _ => return Ok(()),
    };
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()));
    match host {
        Some(h) if !is_host_allowed(&h, allow.as_deref(), block.as_deref()) => {
            Err(format!("blocked by domain filter: {}", h))
        }
        Some(_) => Ok(()),
        None if allow.is_some() => {
            Err(format!("blocked by domain filter: unparseable host in {}", url))
        }
        None => Ok(()),
    }
}

/// Fetch with fast strategy
pub async fn fetch_fast(url: &str) -> Result<String, String> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(cached);
    }
    let html = strategies::fetch_fast_with_client(url)
        .await
        .map(|r| r.html)?;
    fetch_cache_put(url, &html);
    Ok(html)
}

/// Fetch with auto strategy
pub async fn fetch_auto(url: &str) -> Result<String, String> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(cached);
    }
    let html = strategies::fetch_auto_with_client(url)
        .await
        .map(|r| r.html)?;
    fetch_cache_put(url, &html);
    Ok(html)
}

/// Fetch with auto strategy, returning full result with metadata.
pub async fn fetch_auto_with_result(url: &str) -> Result<FetchResult, String> {
    strategies::fetch_auto_with_client(url).await
}

pub async fn fetch_strategy(url: &str) -> Result<String, String> {
    match get_fetch_strategy() {
        FetchStrategy::Fast => fetch_fast(url).await,
        FetchStrategy::Auto => fetch_auto(url).await,
    }
}

/// Fetch raw bytes (images, PDFs, other binary content) using same strategy
pub async fn fetch_bytes(url: &str, referer: Option<&str>) -> Result<Vec<u8>, String> {
    check_domain_filter(url)?;
    match get_fetch_strategy() {
        FetchStrategy::Fast => strategies::fetch_bytes_fast_with_client(url, referer).await,
        FetchStrategy::Auto => strategies::fetch_bytes_auto_with_client(url, referer).await,
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;

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
    fn is_host_allowed_block_list() {
        let block = vec!["reddit.com".to_string(), "tiktok.com".to_string()];
        assert!(!is_host_allowed("reddit.com", None, Some(&block)));
        assert!(!is_host_allowed("old.reddit.com", None, Some(&block)));
        assert!(is_host_allowed("example.com", None, Some(&block)));
    }

    #[test]
    fn is_host_allowed_allow_list() {
        let allow = vec!["example.com".to_string()];
        assert!(is_host_allowed("example.com", Some(&allow), None));
        assert!(is_host_allowed("sub.example.com", Some(&allow), None));
        assert!(!is_host_allowed("reddit.com", Some(&allow), None));
    }

    #[test]
    fn is_host_allowed_block_beats_allow() {
        let allow = vec!["example.com".to_string()];
        let block = vec!["bad.example.com".to_string()];
        assert!(!is_host_allowed(
            "bad.example.com",
            Some(&allow),
            Some(&block)
        ));
        assert!(is_host_allowed(
            "good.example.com",
            Some(&allow),
            Some(&block)
        ));
    }
}
