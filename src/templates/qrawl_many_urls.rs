use std::collections::HashSet;

use futures_util::stream::StreamExt;

use crate::tools::classify::classify_has_schema_type;
use crate::tools::fetch::is_url_allowed;
use crate::tools::map::map_children;
use crate::types::{CanonicalUrl, Context, Jsonld, PageContent};

use super::utils::{crawl, resolve_url};

/// Upper bound on crawl depth — a guardrail against a runaway expansion;
/// `ctx.depth` is clamped to this.
const MAX_DEPTH: usize = 3;

/// Fetch and normalize **many** URLs concurrently — fast, best-effort, and
/// optionally crawling `ctx.depth` levels deep.
///
/// Runs a bounded concurrent crawl (up to `ctx.concurrency` fetches in flight,
/// never sequential):
/// - `ctx.depth == 0`: fetch + extract exactly the given URLs (no child discovery).
/// - `ctx.depth >= 1`: also `map_children` each fetched page and follow those, up
///   to `ctx.depth` levels (clamped to `MAX_DEPTH`), deduped by canonical URL.
///
/// Each URL resolves the way [`qrawl_any_url`](super::qrawl_any_url) resolves
/// it — a recognized social *item* through its side door, everything else
/// through the page — except that a crawl doesn't carry bodies
/// (`PageContent::body` is `None`). URLs that fail to fetch are dropped, so
/// the returned `Vec` holds only successes, in completion order (not input
/// order).
///
/// All knobs live on [`Context`]:
/// - `allow_domains`/`block_domains` and `allow_urls`/`block_urls` filter
///   *before* fetch, on seeds and every child, so a blocked URL is never
///   fetched or returned.
/// - `include_schemas`/`exclude_schemas` filter the *results* after fetch: a page
///   counts (and counts toward `limit`) only if its structured data includes an
///   include-type (empty = any) and none of the exclude-types; non-matching
///   pages are still expanded for their children. Side-door hits carry no
///   structured data, so a non-empty `include_schemas` drops them — a Recipe
///   filter keeps blog pages, not TikTok/oEmbed items.
/// - `limit` caps matching results and **stops early** once reached (in-flight
///   fetches are cancelled); `None` = no cap. Bulk callers pass `Context::fast()`,
///   whose per-attempt fetch timeout is inherently short to cap tail latency.
///
/// ```no_run
/// use qrawl::templates::qrawl_many_urls;
/// use qrawl::types::Context;
///
/// # async fn run(seeds: Vec<String>) {
/// // Direct: just these URLs.
/// let pages = qrawl_many_urls(seeds.clone(), Context::fast()).await;
/// // Expand one level, recipe pages only, stop after 30.
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
    let (allow_domains, block_domains) = (ctx.allow_domains.clone(), ctx.block_domains.clone());
    let canonical_set = |urls: &[String]| -> HashSet<CanonicalUrl> {
        urls.iter().map(|url| CanonicalUrl::new(url)).collect()
    };
    let (allow_urls, block_urls) = (
        canonical_set(&ctx.allow_urls),
        canonical_set(&ctx.block_urls),
    );
    let admit = move |url: &CanonicalUrl| {
        is_url_allowed(url.as_str(), &allow_domains, &block_domains)
            && (allow_urls.is_empty() || allow_urls.contains(url))
            && !block_urls.contains(url)
    };

    let pages = crawl(
        ctx,
        urls,
        depth,
        admit,
        |target, with_children| async move {
            let (mut page, html) = resolve_url(&target)
                .await
                .map_err(|e| log::debug!("{} failed: {}", target.address, e))
                .ok()?;
            log::info!("crawled {} ({:?})", target.address, page.source);
            page.body = None;
            // A side-door hit has no page to map, so it's a leaf.
            let children = match html {
                Some(html) if with_children => map_children(&html, &target.address).await,
                _ => Vec::new(),
            };
            Some((page, children))
        },
    )
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
