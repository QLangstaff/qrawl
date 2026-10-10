use std::sync::Arc;

use futures_util::stream::Stream;
use tokio::sync::mpsc;

use crate::types::{fetch_cache_new, Context, CTX, FETCH_CACHE};

use super::utils::{build_discover_stream, build_fetch_stream, pump};

/// Streaming variant of [`qrawl_children`](super::qrawl_children).
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
/// *before* paying their fetch cost (e.g., to skip URLs the caller already has
/// cached), compose [`qrawl_discover_children`](super::qrawl_discover_children),
/// your filter, and [`qrawl_fetch_stream`](super::qrawl_fetch_stream) directly.
/// This function exists for the common "give me everything" case where no such
/// filter is needed, and preserves the leaf-fallback `FETCH_CACHE` win by
/// running both stages in one scope.
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
