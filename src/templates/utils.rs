//! Shared building blocks for the `qrawl_*` templates.

use std::collections::HashSet;
use std::future::Future;

use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use tokio::sync::mpsc;

use crate::errors::QrawlError;
use crate::tools::extract::{extract_page_content, extract_social_content, extract_social_parts};
use crate::tools::fetch::{fetch_social, fetch_strategy};
use crate::tools::map::map_children;
use crate::types::{CanonicalUrl, Context, Html, PageContent};

/// A URL to crawl. `address` is what gets fetched: the URL as given, because
/// a server can answer `http://`, `www.`, and a trailing slash differently.
/// `url` only identifies the page, so two addresses of one page are crawled
/// once.
#[derive(Debug, Clone)]
pub(super) struct Target {
    pub(super) address: String,
    pub(super) url: CanonicalUrl,
}

/// The [`Target`] for `input`, or an error — before any network — when
/// `input` isn't an HTTP URL. An input without a scheme (`example.com/a`) is
/// fetched over `https`.
pub(super) fn parse_url(input: &str) -> Result<Target, QrawlError> {
    let input = input.trim();
    let url = CanonicalUrl::new(input);
    // `CanonicalUrl` is `https` for every HTTP URL and leaves anything else
    // (`mailto:`, `ftp:`, a relative path) as written.
    let is_http = url::Url::parse(url.as_str()).is_ok_and(|parsed| parsed.scheme() == "https");
    if !is_http {
        return Err(QrawlError::new(format!("invalid URL: {input}")));
    }
    let lower = input.to_ascii_lowercase();
    let address = if lower.starts_with("http://") || lower.starts_with("https://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    Ok(Target { address, url })
}

/// Resolve one URL into its [`PageContent`]: a recognized social item through
/// its door ([`fetch_social`] → [`extract_social_content`]), everything else —
/// and any door miss — through the page ([`fetch_strategy`] →
/// [`extract_page_content`]). The page path also returns the fetched [`Html`],
/// which a crawl maps for children.
pub(super) async fn resolve_url(
    target: &Target,
) -> Result<(PageContent, Option<Html>), QrawlError> {
    let social = extract_social_parts(target.url.as_str());
    if let Some(social) = &social {
        if let Some(payload) = fetch_social(social, &target.url).await {
            if let Some(page) = extract_social_content(&target.url, social, &payload) {
                return Ok((page, None));
            }
        }
    }
    let html = fetch_strategy(&target.address).await?;
    let page = extract_page_content(&target.url, social.as_ref(), &html).await;
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
        loop {
            let item = tokio::select! {
                _ = tx.closed() => break,
                item = stream.next() => item,
            };
            let Some(item) = item else { break };
            if tx.send(item).await.is_err() {
                break;
            }
        }
    }));
    receiver_stream(rx)
}

fn receiver_stream<T: Send + 'static>(
    rx: mpsc::Receiver<T>,
) -> impl Stream<Item = T> + Send + 'static {
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// Breadth-first crawl from `seeds`, `depth` levels of children deep, as one
/// crawl ([`Context::scope`]) on its own task.
///
/// `visit` fetches one [`Target`] and returns its item plus — when told to
/// expand — its child URLs, or `None` to drop it. Each page is visited once,
/// at the shallowest level that reaches it: a level starts only when the one
/// before it has finished, so the result doesn't depend on which fetch
/// completes first. `admit` filters every URL before its visit.
///
/// Yields `(level, item)` in completion order within a level. Every visit of
/// a level shares one pool of `ctx.concurrency`, and dropping the stream stops
/// the crawl at once.
pub(super) fn crawl<T, Admit, Visit, Fut>(
    ctx: Context,
    seeds: Vec<String>,
    depth: usize,
    admit: Admit,
    visit: Visit,
) -> impl Stream<Item = (usize, T)> + Send + 'static
where
    T: Send + 'static,
    Admit: Fn(&CanonicalUrl) -> bool + Send + Sync + 'static,
    Visit: Fn(Target, bool) -> Fut + Send + 'static,
    Fut: Future<Output = Option<(T, Vec<String>)>> + Send + 'static,
{
    let concurrency = ctx.concurrency.get();
    let (tx, rx) = mpsc::channel(concurrency);
    tokio::spawn(ctx.scope(async move {
        let mut seen = HashSet::new();
        let mut admit_new = |urls: &[String]| -> Vec<Target> {
            urls.iter()
                .filter_map(|url| parse_url(url).ok())
                .filter(|target| admit(&target.url) && seen.insert(target.url.clone()))
                .collect()
        };
        let mut frontier = admit_new(&seeds);
        for level in 0..=depth {
            let mut waiting = std::mem::take(&mut frontier).into_iter();
            let mut in_flight = FuturesUnordered::new();
            loop {
                while in_flight.len() < concurrency {
                    match waiting.next() {
                        Some(target) => in_flight.push(visit(target, level < depth)),
                        None => break,
                    }
                }
                let visited = tokio::select! {
                    _ = tx.closed() => return,
                    visited = in_flight.next() => visited,
                };
                let Some(visited) = visited else { break };
                let Some((item, children)) = visited else {
                    continue;
                };
                frontier.extend(admit_new(&children));
                if tx.send((level, item)).await.is_err() {
                    return;
                }
            }
        }
    }));
    receiver_stream(rx)
}

/// Fetch the parents, map each one's children, and yield every child once, in
/// [`CanonicalUrl`] form.
pub(super) fn discover_children(
    ctx: Context,
    urls: Vec<String>,
) -> impl Stream<Item = String> + Send + 'static {
    crawl(
        ctx,
        urls,
        0,
        |_url| true,
        |target, _expand| async move {
            let html = fetch_strategy(&target.address)
                .await
                .map_err(|e| log::debug!("fetch failed {}: {}", target.address, e))
                .ok()?;
            Some((map_children(&html, &target.address).await, Vec::new()))
        },
    )
    .flat_map({
        let mut seen = HashSet::new();
        move |(_level, children): (usize, Vec<String>)| {
            let unseen: Vec<String> = children
                .iter()
                .map(|child| CanonicalUrl::new(child).into_inner())
                .filter(|child| seen.insert(child.clone()))
                .collect();
            futures_util::stream::iter(unseen)
        }
    })
}
