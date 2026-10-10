//! Shared streaming crawl building blocks for the `qrawl_*` templates.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures_util::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

use crate::tools::fetch::fetch_strategy;
use crate::tools::map::map_children;
use crate::tools::normalize::{normalize_url, normalize_urls};

/// Discover half of the pipeline: clean+dedupe parents, fetch them, run
/// `map_children` (parallel, parse-bounded), canonicalize+dedupe children
/// across the stream. Pure stream construction — no scoping, no spawning;
/// must be run inside a `CTX` + `FETCH_CACHE` scope.
pub(super) fn build_discover_stream(
    urls: Vec<String>,
    concurrency: usize,
) -> impl Stream<Item = String> + Send + 'static {
    // Stage 1: normalize input URLs (synchronous; small list).
    let parents = normalize_urls(&urls);

    // Parse concurrency is CPU-bound (scraper DOM build inside spawn_blocking);
    // exceeding core count just piles parsed `Html` trees into memory without
    // throughput gain. Capped independently of `ctx.concurrency`, which sizes
    // fetch fan-out.
    let parse_concurrency = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(concurrency)
        .min(concurrency);

    // Shared dedupe set for children. Only one task accesses it (the producer
    // task), but `flat_map`'s closure must be `FnMut + Send`; an `Arc<Mutex<_>>`
    // is the simplest way to satisfy that with no actual contention.
    let seen: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    futures_util::stream::iter(parents)
        // Stage 2: fetch parents, drop fetch errors.
        .map(|url| async move {
            fetch_strategy(&url)
                .await
                .map_err(|e| log::debug!("parent fetch failed {}: {}", url, e))
                .ok()
                .map(|html| (url, html))
        })
        .buffer_unordered(concurrency)
        .filter_map(|opt| async move { opt })
        // Stage 3: per parent, discover children. `map_children` parses via
        // `spawn_blocking`; bounded concurrency lets parents parse in parallel
        // across cores without unbounded DOMs in memory.
        .map(|(parent_url, parent_html)| async move {
            map_children(&parent_html, &parent_url).await
        })
        .buffer_unordered(parse_concurrency)
        // Stage 4: flatten + canonicalize + dedupe across the stream.
        .flat_map(move |children| {
            let mut unique = Vec::with_capacity(children.len());
            for c in children {
                let canonical = normalize_url(&c);
                if seen.lock().unwrap().insert(canonical.clone()) {
                    unique.push(canonical);
                }
            }
            futures_util::stream::iter(unique)
        })
}

/// Fetch half of the pipeline: per-URL `fetch_strategy`, drop errors. Pure
/// stream construction — no scoping, no spawning; must be run inside a `CTX`
/// + `FETCH_CACHE` scope.
pub(super) fn build_fetch_stream<S>(
    urls: S,
    concurrency: usize,
) -> impl Stream<Item = (String, String)> + Send + 'static
where
    S: Stream<Item = String> + Send + 'static,
{
    urls.map(|child_url| async move {
        fetch_strategy(&child_url)
            .await
            .map_err(|e| log::debug!("child fetch failed {}: {}", child_url, e))
            .ok()
            .map(|html| (child_url, html.into_inner()))
    })
    .buffer_unordered(concurrency)
    .filter_map(|opt| async move { opt })
}

/// Pump items from a stream into an mpsc channel until the stream ends or the
/// receiver is dropped.
pub(super) async fn pump<T, S>(stream: S, tx: mpsc::Sender<T>)
where
    S: Stream<Item = T>,
    T: Send + 'static,
{
    let mut s = Box::pin(stream);
    while let Some(item) = s.next().await {
        if tx.send(item).await.is_err() {
            break;
        }
    }
}
