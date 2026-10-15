//! Fetch Tools

mod client;
mod embed;
mod headers;
mod strategies;
mod types;
mod utils;

mod tests;

pub use embed::fetch_embed;
pub use strategies::MAX_RESPONSE_BYTES;
pub use types::Fetched;

use crate::errors::QrawlError;
use crate::types::{get_fetch_strategy, CanonicalUrl, FetchStrategy, Html};
use crate::utils::absolute_url;
use utils::check_domain_filter;

/// Fetch a page with the `Fast` strategy: one `Minimal` attempt.
pub async fn fetch_fast(url: &str) -> Result<Fetched, QrawlError> {
    fetch_html(url, FetchStrategy::Fast).await
}

/// Fetch a page with the `Auto` strategy: the Minimal → Windows → iOS cascade.
pub async fn fetch_auto(url: &str) -> Result<Fetched, QrawlError> {
    fetch_html(url, FetchStrategy::Auto).await
}

/// Fetch a page with the strategy of the enclosing [`Context`](crate::types::Context).
pub async fn fetch_strategy(url: &str) -> Result<Fetched, QrawlError> {
    fetch_html(url, get_fetch_strategy()).await
}

async fn fetch_html(url: &str, strategy: FetchStrategy) -> Result<Fetched, QrawlError> {
    let canonical_url = check_request(url)?;
    let (address, html) = strategies::fetch_html(url, &canonical_url, strategy).await?;
    Ok(Fetched {
        address,
        html: Html::new(html),
    })
}

/// Fetch raw bytes (images, PDFs, JSON) with the strategy of the enclosing
/// [`Context`](crate::types::Context).
pub async fn fetch_bytes(url: &str, referer: Option<&str>) -> Result<Vec<u8>, QrawlError> {
    let canonical_url = check_request(url)?;
    strategies::fetch_bytes(url, &canonical_url, get_fetch_strategy(), referer).await
}

/// Refuse, before any network, a URL that isn't `http(s)` or that the domain
/// filter rejects; otherwise its [`CanonicalUrl`].
fn check_request(url: &str) -> Result<CanonicalUrl, QrawlError> {
    let invalid = || QrawlError::InvalidUrl(url.to_string());
    if absolute_url(url).as_deref() != Some(url.trim()) {
        return Err(invalid());
    }
    let canonical_url = CanonicalUrl::new(url).ok_or_else(invalid)?;
    check_domain_filter(url, &canonical_url)?;
    Ok(canonical_url)
}
