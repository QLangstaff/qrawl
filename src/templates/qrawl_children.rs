use futures_util::stream::StreamExt;

use crate::types::{Context, Html};

use super::qrawl_children_stream::qrawl_children_stream;

/// Get children from URLs: [`qrawl_children_stream`], collected.
///
/// Fetches each parent URL, discovers its child URLs, fetches those, and
/// returns every `(child_url, child_html)` pair. To stop fetching once you have
/// enough, use `qrawl_children_stream` with `StreamExt::take`.
pub async fn qrawl_children(urls: Vec<String>, ctx: Context) -> Vec<(String, Html)> {
    qrawl_children_stream(urls, ctx).collect().await
}
