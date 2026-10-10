use std::collections::HashSet;

use futures_util::stream::StreamExt;

use crate::tools::classify::classify_has_schema_type;
use crate::tools::map::map_children;
use crate::tools::normalize::CanonicalUrl;
use crate::types::{Context, Jsonld};

use super::types::PageContent;
use super::utils::{crawl, page_content};

/// Upper bound on crawl depth — a guardrail against a runaway expansion;
/// `ctx.depth` is clamped to this.
const MAX_DEPTH: usize = 3;

/// [`qrawl_any_url`](super::qrawl_any_url) over **many** URLs — concurrent,
/// best-effort, and optionally crawling `ctx.depth` levels of children deep.
///
/// Every URL becomes a [`PageContent`] by the same steps as `qrawl_any_url`.
/// A URL that can't be fetched is dropped, so the result holds only
/// successes: level by level, in completion order within a level.
///
/// - `ctx.depth == 0`: exactly the given URLs.
/// - `ctx.depth >= 1`: also each fetched page's children (`map_children`), up
///   to `ctx.depth` levels (at most `MAX_DEPTH`). Each page is crawled once,
///   at the shallowest level that reaches it.
/// - `allow_domains`/`block_domains`: a URL on a filtered host fails its fetch,
///   so it's never returned or expanded.
/// - `allow_urls`/`block_urls`: filter seeds and children before their fetch.
/// - `include_schemas`/`exclude_schemas`: filter the *results*. A page counts
///   (toward `limit` too) only if its structured data includes an include-type
///   (empty = any) and none of the exclude-types; a page that doesn't count is
///   still expanded for its children.
/// - `limit`: stop once this many results are in, cancelling the fetches in
///   flight; `None` = no cap.
///
/// ```no_run
/// use qrawl::templates::qrawl_many_urls;
/// use qrawl::types::Context;
///
/// # async fn run(seeds: Vec<String>) {
/// // Just these URLs.
/// let pages = qrawl_many_urls(seeds.clone(), Context::fast()).await;
/// // One level deep, recipe pages only, stop after 30.
/// let ctx = Context::fast()
///     .with_depth(1)
///     .with_limit(30)
///     .with_include_schemas(vec!["Recipe".to_string()]);
/// let recipes = qrawl_many_urls(seeds, ctx).await;
/// # }
/// ```
pub async fn qrawl_many_urls(urls: Vec<String>, ctx: Context) -> Vec<PageContent> {
    let limit = ctx.limit;
    let depth = ctx.depth.min(MAX_DEPTH);
    let (include_schemas, exclude_schemas) =
        (ctx.include_schemas.clone(), ctx.exclude_schemas.clone());
    let canonical_set = |urls: &[String]| -> HashSet<CanonicalUrl> {
        urls.iter().map(|url| CanonicalUrl::new(url)).collect()
    };
    let (allow_urls, block_urls) = (
        canonical_set(&ctx.allow_urls),
        canonical_set(&ctx.block_urls),
    );
    let admit = move |url: &CanonicalUrl| {
        (allow_urls.is_empty() || allow_urls.contains(url)) && !block_urls.contains(url)
    };

    let pages = crawl(ctx, urls, depth, admit, move |target, level| async move {
        let (page, html) = page_content(&target)
            .await
            .map_err(|e| log::debug!("{} failed: {}", target.address, e))
            .ok()?;
        log::info!("crawled {}", target.address);
        let children = match html {
            Some(html) if level < depth => map_children(&html, &target.address).await,
            _ => Vec::new(),
        };
        Some((page, children))
    })
    // The schema filter decides results only: a roundup that doesn't match
    // still yields its matching children.
    .filter_map(move |(_level, page)| {
        let passes = schema_passes(&page.structured, &include_schemas, &exclude_schemas);
        async move { passes.then_some(page) }
    });
    match limit {
        Some(limit) => pages.take(limit).collect().await,
        None => pages.collect().await,
    }
}

/// Whether a page's structured data carries one of the `include` types (empty
/// = any) and none of the `exclude` types. Matching is normalization-aware
/// (short name / IRI / CURIE) via [`classify_has_schema_type`].
pub(super) fn schema_passes(structured: &Jsonld, include: &[String], exclude: &[String]) -> bool {
    let has_any = |types: &[String]| {
        types
            .iter()
            .any(|schema_type| classify_has_schema_type(structured, schema_type))
    };
    (include.is_empty() || has_any(include)) && !has_any(exclude)
}
