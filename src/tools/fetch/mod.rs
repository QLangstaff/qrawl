//! Fetch Tools

mod client;
mod headers;
mod social;
pub mod strategies;
pub mod types;
mod utils;

mod tests;

pub use social::fetch_social;
pub use types::FetchProfile;
pub(crate) use utils::is_url_allowed;

use crate::errors::QrawlError;
use crate::types::{fetch_cache_get, fetch_cache_put, get_fetch_strategy, FetchStrategy, Html};
use utils::check_domain_filter;

/// Fetch with fast strategy
pub async fn fetch_fast(url: &str) -> Result<Html, QrawlError> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(Html::new(cached));
    }
    let html = strategies::fetch_fast(url).await?;
    fetch_cache_put(url, &html);
    Ok(Html::new(html))
}

/// Fetch with auto strategy
pub async fn fetch_auto(url: &str) -> Result<Html, QrawlError> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(Html::new(cached));
    }
    let html = strategies::fetch_auto(url).await?;
    fetch_cache_put(url, &html);
    Ok(Html::new(html))
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
        FetchStrategy::Fast => strategies::fetch_bytes_fast(url, referer).await,
        FetchStrategy::Auto => strategies::fetch_bytes_auto(url, referer).await,
    }
}
