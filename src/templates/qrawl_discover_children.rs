use futures_util::stream::Stream;

use crate::types::Context;

use super::utils::discover_children;

/// Streaming child-URL discovery.
///
/// Fetches each parent URL, runs `map_children` on the HTML, and yields the
/// child URLs — each once, in [`CanonicalUrl`](crate::types::CanonicalUrl)
/// form, in completion order. It stops before the children are fetched, so
/// callers can `filter` or `take` the URL stream before paying the fetch cost:
///
/// ```no_run
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
/// Composed this way, each half is its own crawl with its own fetch cache, so
/// a parent with no children — which `map_children` returns as its own child —
/// is fetched twice. [`qrawl_children_stream`](super::qrawl_children_stream)
/// runs both halves as one crawl and fetches it once.
pub fn qrawl_discover_children(
    urls: Vec<String>,
    ctx: Context,
) -> impl Stream<Item = String> + Send + 'static {
    discover_children(ctx, urls)
}
