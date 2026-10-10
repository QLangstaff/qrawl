use futures_util::stream::StreamExt;

use crate::errors::QrawlError;
use crate::types::Context;

use super::qrawl_children_stream::qrawl_children_stream;

/// Get children from URLs (collect-mode wrapper around
/// [`qrawl_children_stream`]).
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
) -> Result<Vec<(String, String)>, QrawlError> {
    Ok(qrawl_children_stream(urls, ctx).collect().await)
}
