use std::collections::{HashSet, VecDeque};

use futures_util::stream::{FuturesUnordered, StreamExt};

use crate::tools::classify::classify_has_schema_type;
use crate::tools::fetch::is_url_allowed;
use crate::tools::map::map_children;
use crate::types::{CanonicalUrl, Context, Jsonld, PageContent};

use super::utils::resolve_url;

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
    ctx.clone().scope(crawl(urls, &ctx)).await
}

/// Breadth-first crawl with `ctx.concurrency` nodes in flight. Stops as soon
/// as `ctx.limit` is reached, which drops — and so cancels — the fetches still
/// in flight.
async fn crawl(urls: Vec<String>, ctx: &Context) -> Vec<PageContent> {
    let depth = ctx.depth.min(MAX_DEPTH);
    let allow_urls: HashSet<CanonicalUrl> = ctx
        .allow_urls
        .iter()
        .map(|url| CanonicalUrl::new(url))
        .collect();
    // A blocked URL starts out seen, so it's never queued.
    let mut seen: HashSet<CanonicalUrl> = ctx
        .block_urls
        .iter()
        .map(|url| CanonicalUrl::new(url))
        .collect();
    let mut queue: VecDeque<(CanonicalUrl, usize)> = VecDeque::new();
    let mut enqueue = |queue: &mut VecDeque<(CanonicalUrl, usize)>, url: &str, level: usize| {
        let url = CanonicalUrl::new(url);
        let is_allowed = is_url_allowed(url.as_str(), &ctx.allow_domains, &ctx.block_domains)
            && (allow_urls.is_empty() || allow_urls.contains(&url));
        if is_allowed && seen.insert(url.clone()) {
            queue.push_back((url, level));
        }
    };
    for url in &urls {
        enqueue(&mut queue, url, 0);
    }

    let mut in_flight = FuturesUnordered::new();
    let mut results: Vec<PageContent> = Vec::new();

    loop {
        while in_flight.len() < ctx.concurrency.get() {
            match queue.pop_front() {
                Some((url, level)) => in_flight.push(crawl_node(url, level, level < depth)),
                None => break,
            }
        }
        let Some(node) = in_flight.next().await else {
            break;
        };
        let Some((page, children, level)) = node else {
            continue;
        };

        // The schema filter decides results only: a roundup that doesn't match
        // still yields its matching children.
        if schema_passes(&page.structured, &ctx.include_schemas, &ctx.exclude_schemas) {
            results.push(page);
            if ctx.limit.is_some_and(|limit| results.len() >= limit) {
                break;
            }
        }
        for child in &children {
            enqueue(&mut queue, child, level + 1);
        }
    }

    results
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

/// Resolve one node, plus its child URLs when `with_children` — a side-door
/// hit has no page to map, so it's a leaf. `None` when nothing could be
/// fetched. `level` is returned so the crawl can queue the children at
/// `level + 1`.
async fn crawl_node(
    url: CanonicalUrl,
    level: usize,
    with_children: bool,
) -> Option<(PageContent, Vec<String>, usize)> {
    let (mut page, html) = resolve_url(&url)
        .await
        .map_err(|e| log::debug!("depth {} {} failed: {}", level, url, e))
        .ok()?;
    log::info!("depth {} {} ({:?})", level, url, page.source);
    page.body = None;
    let children = match html {
        Some(html) if with_children => map_children(&html, url.as_str()).await,
        _ => Vec::new(),
    };
    Some((page, children, level))
}
