use futures_util::stream::{Stream, StreamExt};

use crate::tools::fetch::fetch_strategy;
use crate::tools::map::map_children;
use crate::types::{Context, Html};

use super::utils::crawl;

/// Streaming variant of [`qrawl_children`](super::qrawl_children).
///
/// Yields `(child_url, child_html)` pairs as each fetch completes, in
/// completion order. Dropping the stream (`take`, `take_while`) stops the
/// crawl: fetches in flight are cancelled and no new ones start.
///
/// A parent with no children — which `map_children` returns as its own child —
/// is yielded itself, from the fetch that mapped it. To filter the discovered
/// URLs before paying their fetch cost, compose
/// [`qrawl_discover_children`](super::qrawl_discover_children), your filter,
/// and [`qrawl_fetch_stream`](super::qrawl_fetch_stream) instead.
pub fn qrawl_children_stream(
    urls: Vec<String>,
    ctx: Context,
) -> impl Stream<Item = (String, Html)> + Send + 'static {
    crawl(
        ctx,
        urls,
        1,
        |_url| true,
        |target, is_parent| async move {
            let html = fetch_strategy(&target.address)
                .await
                .map_err(|e| log::debug!("fetch failed {}: {}", target.address, e))
                .ok()?;
            let children = if is_parent {
                map_children(&html, &target.address).await
            } else {
                Vec::new()
            };
            let is_childless_parent = children == [target.address.clone()];
            Some(((target.address, html, is_childless_parent), children))
        },
    )
    .filter_map(|(level, (url, html, is_childless_parent))| async move {
        (level == 1 || is_childless_parent).then_some((url, html))
    })
}
