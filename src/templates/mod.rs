//! Example Templates

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures_util::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

use crate::tools::clean::{canonicalize_url, clean_urls};
use crate::tools::fetch::fetch_strategy;
use crate::tools::map::map_children;
use crate::types::{fetch_cache_new, Context, CTX, FETCH_CACHE};

/// Streaming child-URL discovery.
///
/// Fetches each parent URL, runs `map_children` on the HTML, and yields the
/// resulting child URLs (canonicalized + deduped) in completion order. Stops
/// before the second fetch stage, so callers can interpose a `filter`,
/// `take`, etc. on the URL stream before paying the fetch cost.
///
/// Compose with [`qrawl_fetch_stream`] for the common discover → filter →
/// fetch pattern:
///
/// ```ignore
/// use futures_util::StreamExt;
/// use qrawl::templates::{qrawl_discover_children, qrawl_fetch_stream};
/// use qrawl::types::Context;
///
/// # async fn run(parents: Vec<String>, saved: std::collections::HashSet<String>) {
/// let urls = qrawl_discover_children(parents, Context::fast())
///     .filter(move |u| {
///         let drop = saved.contains(u);
///         async move { !drop }
///     });
/// let pages = qrawl_fetch_stream(urls, Context::fast())
///     .collect::<Vec<_>>()
///     .await;
/// # }
/// ```
///
/// Note: [`qrawl_children_stream`] does *not* compose these two primitives —
/// it runs both stages inside a single task scope so the per-pipeline
/// `FETCH_CACHE` deduplicates the leaf-fallback case (where `map_children`
/// returns `vec![parent_url]` because no children were found). If you
/// compose `qrawl_discover_children` + `qrawl_fetch_stream` yourself, each
/// half has its own `FETCH_CACHE`, so a leaf parent URL gets fetched twice.
pub fn qrawl_discover_children(
    urls: Vec<String>,
    ctx: Context,
) -> impl Stream<Item = String> + Send + 'static {
    let concurrency = ctx.concurrency;
    let (tx, rx) = mpsc::channel::<String>(concurrency);
    let ctx_arc = Arc::new(ctx);
    let cache = fetch_cache_new();

    tokio::spawn(async move {
        CTX.scope(ctx_arc, async move {
            FETCH_CACHE
                .scope(cache, async move {
                    let stream = build_discover_stream(urls, concurrency);
                    pump(stream, tx).await;
                })
                .await;
        })
        .await;
    });

    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// Streaming fetch over an input URL stream.
///
/// Fetches each URL with the strategy in `ctx`, yielding `(url, html)` pairs
/// in completion order. Per-URL fetch errors are silently dropped (matches
/// [`qrawl_children_stream`]'s error policy).
///
/// The input is any `Stream<Item = String> + Send + 'static` — typically the
/// output of [`qrawl_discover_children`] after caller-side filtering, but any
/// URL stream works. Duplicate URLs in the input are *not* deduped here;
/// `FETCH_CACHE` will short-circuit repeats within this pipeline scope, but
/// they still cost a stream item and an in-flight slot.
pub fn qrawl_fetch_stream<S>(
    urls: S,
    ctx: Context,
) -> impl Stream<Item = (String, String)> + Send + 'static
where
    S: Stream<Item = String> + Send + 'static,
{
    let concurrency = ctx.concurrency;
    let (tx, rx) = mpsc::channel::<(String, String)>(concurrency);
    let ctx_arc = Arc::new(ctx);
    let cache = fetch_cache_new();

    tokio::spawn(async move {
        CTX.scope(ctx_arc, async move {
            FETCH_CACHE
                .scope(cache, async move {
                    let stream = build_fetch_stream(urls, concurrency);
                    pump(stream, tx).await;
                })
                .await;
        })
        .await;
    });

    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// Streaming variant of `qrawl_children`.
///
/// Yields `(child_url, child_html)` pairs *as each fetch completes*, in
/// completion order rather than the input order. Downstream `StreamExt`
/// combinators (`take`, `take_while`, `filter_map`, etc.) can stop the
/// pipeline once the caller has enough — in-flight fetches finish at their
/// next await point, but no new fetches are dispatched.
///
/// Differences vs `qrawl_children`:
///
/// - Returns `impl Stream`, not `Future<Output = Vec<...>>`. The caller drives
///   consumption rate; `qrawl_children` is just `.collect().await` on the stream.
/// - Per-pipeline `FETCH_CACHE` and `CTX` are still scoped (inside an internal
///   `tokio::spawn`'d producer task); the returned stream is `'static + Send`.
/// - Backpressure: an internal `mpsc` channel buffers up to `ctx.concurrency`
///   items; slow consumers stall the producer.
///
/// **Overshoot:** when the consumer drops the stream, in-flight futures
/// across the pipeline's three stages (parent fetch, parse, child fetch)
/// finish at their next await point, and completed items may sit in the mpsc
/// buffer. Worst-case wasted fetches are bounded by a small multiple of
/// `ctx.concurrency` — exact bound depends on stage layout, but the property
/// is "early-termination overshoot is bounded, not unbounded."
///
/// **Pre-fetch filtering:** to apply a predicate to discovered child URLs
/// *before* paying their fetch cost (e.g., to skip URLs the caller already
/// has cached), compose [`qrawl_discover_children`] + your filter +
/// [`qrawl_fetch_stream`] directly. This function exists for the common
/// "give me everything" case where no such filter is needed, and preserves
/// the leaf-fallback `FETCH_CACHE` win by running both stages in one scope.
pub fn qrawl_children_stream(
    urls: Vec<String>,
    ctx: Context,
) -> impl Stream<Item = (String, String)> + Send + 'static {
    let concurrency = ctx.concurrency;
    let (tx, rx) = mpsc::channel::<(String, String)>(concurrency);
    let ctx_arc = Arc::new(ctx);
    let cache = fetch_cache_new();

    tokio::spawn(async move {
        CTX.scope(ctx_arc, async move {
            FETCH_CACHE
                .scope(cache, async move {
                    let discover = build_discover_stream(urls, concurrency);
                    let stream = build_fetch_stream(discover, concurrency);
                    pump(stream, tx).await;
                })
                .await;
        })
        .await;
    });

    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// Get children from URLs (collect-mode wrapper around `qrawl_children_stream`).
///
/// Fetches each parent URL, discovers its child URLs, fetches those, and
/// returns the full `(child_url, child_html)` set as a `Vec`. For early
/// termination (stop fetching once you have enough), use
/// `qrawl_children_stream` directly with `StreamExt::take`.
///
/// Honors `ctx.fetch_strategy` — pass `Context::fast()` for a single-attempt
/// fetch, or `Context::auto()` for the full Minimal → Windows → iOS cascade.
pub async fn qrawl_children(
    urls: Vec<String>,
    ctx: Context,
) -> Result<Vec<(String, String)>, String> {
    Ok(qrawl_children_stream(urls, ctx).collect().await)
}

/// Discover half of the pipeline: clean+dedupe parents, fetch them, run
/// `map_children` (parallel, parse-bounded), canonicalize+dedupe children
/// across the stream. Pure stream construction — no scoping, no spawning;
/// must be run inside a `CTX` + `FETCH_CACHE` scope.
fn build_discover_stream(
    urls: Vec<String>,
    concurrency: usize,
) -> impl Stream<Item = String> + Send + 'static {
    // Stage 1: clean + dedupe input URLs (synchronous; small list).
    let parents = clean_urls(&urls);

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
            fetch_strategy(&url).await.ok().map(|html| (url, html))
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
                let canonical = canonicalize_url(&c);
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
fn build_fetch_stream<S>(
    urls: S,
    concurrency: usize,
) -> impl Stream<Item = (String, String)> + Send + 'static
where
    S: Stream<Item = String> + Send + 'static,
{
    urls.map(|child_url| async move {
        fetch_strategy(&child_url).await.ok().map(|html| (child_url, html))
    })
    .buffer_unordered(concurrency)
    .filter_map(|opt| async move { opt })
}

/// Pump items from a stream into an mpsc channel until the stream ends or the
/// receiver is dropped.
async fn pump<T, S>(stream: S, tx: mpsc::Sender<T>)
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

/// Get emails from URLs.
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Result<Vec<String>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_strategy ->
        map_children ->
        clean_urls ->
        fetch_strategy ->
        map_page ->
        clean_urls ->
        fetch_strategy ->
        extract_emails ->
        clean_emails
    }
    .await;

    Ok(result.into_iter().map(|(_, email)| email).collect())
}
