use futures_util::stream::Stream;

use crate::types::{Context, Html};

use super::utils::{fetch_stream, spawn_crawl};

/// Streaming fetch over an input URL stream.
///
/// Fetches each URL with the strategy in `ctx`, yielding `(url, html)` pairs
/// in completion order. A URL that fails to fetch is dropped.
///
/// The input is typically the output of
/// [`qrawl_discover_children`](super::qrawl_discover_children) after
/// caller-side filtering, but any URL stream works. A URL repeated in the
/// input is yielded each time and fetched once.
pub fn qrawl_fetch_stream<S>(
    urls: S,
    ctx: Context,
) -> impl Stream<Item = (String, Html)> + Send + 'static
where
    S: Stream<Item = String> + Send + 'static,
{
    let concurrency = ctx.concurrency.get();
    spawn_crawl(ctx, fetch_stream(urls, concurrency))
}
