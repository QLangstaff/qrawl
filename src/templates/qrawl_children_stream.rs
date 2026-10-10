use futures_util::stream::Stream;

use crate::types::{Context, Html};

use super::utils::{discover_stream, fetch_stream, spawn_crawl};

/// Streaming variant of [`qrawl_children`](super::qrawl_children).
///
/// Yields `(child_url, child_html)` pairs as each fetch completes, in
/// completion order. Dropping the stream (`take`, `take_while`) stops the
/// crawl: fetches in flight finish, no new ones start, so the wasted fetches
/// are bounded by a small multiple of `ctx.concurrency`.
///
/// Both stages run as one crawl, so a parent with no children — which
/// `map_children` returns as its own child — is fetched once. To filter the
/// discovered URLs before paying their fetch cost, compose
/// [`qrawl_discover_children`](super::qrawl_discover_children), your filter,
/// and [`qrawl_fetch_stream`](super::qrawl_fetch_stream) instead.
pub fn qrawl_children_stream(
    urls: Vec<String>,
    ctx: Context,
) -> impl Stream<Item = (String, Html)> + Send + 'static {
    let concurrency = ctx.concurrency.get();
    spawn_crawl(
        ctx,
        fetch_stream(discover_stream(urls, concurrency), concurrency),
    )
}
