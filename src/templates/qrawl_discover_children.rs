use std::sync::Arc;

use futures_util::stream::Stream;
use tokio::sync::mpsc;

use crate::types::{fetch_cache_new, Context, CTX, FETCH_CACHE};

use super::utils::{build_discover_stream, pump};

/// Streaming child-URL discovery.
///
/// Fetches each parent URL, runs `map_children` on the HTML, and yields the
/// resulting child URLs (canonicalized + deduped) in completion order. Stops
/// before the second fetch stage, so callers can interpose a `filter`,
/// `take`, etc. on the URL stream before paying the fetch cost.
///
/// Compose with [`qrawl_fetch_stream`](super::qrawl_fetch_stream) for the common
/// discover → filter → fetch pattern:
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
/// Note: [`qrawl_children_stream`](super::qrawl_children_stream) does *not*
/// compose these two primitives — it runs both stages inside a single task scope
/// so the per-pipeline `FETCH_CACHE` deduplicates the leaf-fallback case (where
/// `map_children` returns `vec![parent_url]` because no children were found). If
/// you compose `qrawl_discover_children` + `qrawl_fetch_stream` yourself, each
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
