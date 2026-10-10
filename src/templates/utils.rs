//! Shared building blocks for the `qrawl_*` templates.

use std::collections::HashSet;

use futures_util::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

use crate::errors::QrawlError;
use crate::tools::extract::{extract_page_content, extract_social_content, extract_social_parts};
use crate::tools::fetch::{fetch_social, fetch_strategy};
use crate::tools::map::map_children;
use crate::tools::normalize::{normalize_social, normalize_social_urls};
use crate::types::{CanonicalUrl, Context, Html, PageContent};

/// Canonicalize `url`, or fail before any network when it isn't a URL.
pub(super) fn parse_url(url: &str) -> Result<CanonicalUrl, QrawlError> {
    let canonical = CanonicalUrl::new(url);
    match url::Url::parse(canonical.as_str()) {
        Ok(_) => Ok(canonical),
        Err(_) => Err(QrawlError::new(format!("invalid URL: {canonical}"))),
    }
}

/// Resolve one URL into its [`PageContent`]: a recognized social item through
/// its door ([`fetch_social`] → [`extract_social_content`]), everything else —
/// and any door miss — through the page ([`fetch_strategy`] →
/// [`extract_page_content`]). The page path also returns the fetched [`Html`],
/// which a crawl maps for children.
pub(super) async fn resolve_url(
    url: &CanonicalUrl,
) -> Result<(PageContent, Option<Html>), QrawlError> {
    let social = extract_social_parts(url.as_str());
    if let Some(social) = &social {
        if let Some(payload) = fetch_social(social, url).await {
            if let Some(page) = extract_social_content(url, social, &payload) {
                return Ok((page, None));
            }
        }
    }
    let html = fetch_strategy(url.as_str()).await?;
    let page = extract_page_content(url, social.as_ref(), &html).await;
    Ok((page, Some(html)))
}

/// Fetch each URL as it arrives and yield the `(url, html)` pairs that succeed,
/// in completion order.
pub(super) fn fetch_stream<S>(
    urls: S,
    concurrency: usize,
) -> impl Stream<Item = (String, Html)> + Send + 'static
where
    S: Stream<Item = String> + Send + 'static,
{
    urls.map(|url| async move {
        fetch_strategy(&url)
            .await
            .map_err(|e| log::debug!("fetch failed {}: {}", url, e))
            .ok()
            .map(|html| (url, html))
    })
    .buffer_unordered(concurrency)
    .filter_map(|fetched| async move { fetched })
}

/// Fetch the parents, map each one's children, and yield every child once, in
/// [`CanonicalUrl`] form.
pub(super) fn discover_stream(
    urls: Vec<String>,
    concurrency: usize,
) -> impl Stream<Item = String> + Send + 'static {
    // Parsing is CPU-bound, so more parses in flight than cores only holds more
    // DOMs in memory.
    let parse_concurrency = std::thread::available_parallelism()
        .map_or(concurrency, |cores| cores.get().min(concurrency));
    let mut seen = HashSet::new();

    fetch_stream(
        futures_util::stream::iter(normalize_social_urls(&urls)),
        concurrency,
    )
    .map(|(parent_url, parent_html)| async move { map_children(&parent_html, &parent_url).await })
    .buffer_unordered(parse_concurrency)
    .flat_map(move |children| {
        let unseen: Vec<String> = children
            .iter()
            .map(|child| normalize_social(child))
            .filter(|child| seen.insert(child.clone()))
            .collect();
        futures_util::stream::iter(unseen)
    })
}

/// Run `stream` as one crawl ([`Context::scope`]) on its own task, and yield
/// its items through a channel that holds `ctx.concurrency` of them, so a slow
/// consumer stalls the crawl and a dropped consumer ends it.
pub(super) fn spawn_crawl<T, S>(ctx: Context, stream: S) -> impl Stream<Item = T> + Send + 'static
where
    T: Send + 'static,
    S: Stream<Item = T> + Send + 'static,
{
    let (tx, rx) = mpsc::channel(ctx.concurrency.get());
    tokio::spawn(ctx.scope(async move {
        let mut stream = Box::pin(stream);
        while let Some(item) = stream.next().await {
            if tx.send(item).await.is_err() {
                break;
            }
        }
    }));
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}
