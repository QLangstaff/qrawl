use std::sync::Arc;

use futures_util::stream::Stream;
use tokio::sync::mpsc;

use crate::types::{fetch_cache_new, Context, CTX, FETCH_CACHE};

use super::utils::{build_fetch_stream, pump};

/// Streaming fetch over an input URL stream.
///
/// Fetches each URL with the strategy in `ctx`, yielding `(url, html)` pairs
/// in completion order. Per-URL fetch errors are silently dropped (matches
/// [`qrawl_children_stream`](super::qrawl_children_stream)'s error policy).
///
/// The input is any `Stream<Item = String> + Send + 'static` — typically the
/// output of [`qrawl_discover_children`](super::qrawl_discover_children) after
/// caller-side filtering, but any URL stream works. Duplicate URLs in the input
/// are *not* deduped here; `FETCH_CACHE` will short-circuit repeats within this
/// pipeline scope, but they still cost a stream item and an in-flight slot.
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
