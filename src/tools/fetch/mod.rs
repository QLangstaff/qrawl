//! Fetch Tools

mod client;
mod embed;
mod headers;
mod strategies;
mod types;
mod utils;

mod tests;

pub use embed::fetch_embed;
pub use types::FetchProfile;

use crate::errors::QrawlError;
use crate::types::{fetch_cache_get, fetch_cache_put, get_fetch_strategy, FetchStrategy, Html};
use utils::check_domain_filter;

/// Fetch a page with the `Fast` strategy: one `Minimal` attempt.
pub async fn fetch_fast(url: &str) -> Result<Html, QrawlError> {
    fetch_html(url, FetchStrategy::Fast).await
}

/// Fetch a page with the `Auto` strategy: the Minimal → Windows → iOS cascade.
pub async fn fetch_auto(url: &str) -> Result<Html, QrawlError> {
    fetch_html(url, FetchStrategy::Auto).await
}

/// Fetch a page with the strategy of the enclosing [`Context`](crate::types::Context).
pub async fn fetch_strategy(url: &str) -> Result<Html, QrawlError> {
    fetch_html(url, get_fetch_strategy()).await
}

async fn fetch_html(url: &str, strategy: FetchStrategy) -> Result<Html, QrawlError> {
    check_domain_filter(url)?;
    if let Some(cached) = fetch_cache_get(url) {
        return Ok(Html::new(cached));
    }
    let html = strategies::fetch_html(url, strategy).await?;
    fetch_cache_put(url, &html);
    Ok(Html::new(html))
}

/// Fetch raw bytes (images, PDFs, JSON) with the strategy of the enclosing
/// [`Context`](crate::types::Context).
pub async fn fetch_bytes(url: &str, referer: Option<&str>) -> Result<Vec<u8>, QrawlError> {
    check_domain_filter(url)?;
    strategies::fetch_bytes(url, get_fetch_strategy(), referer).await
}
