use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use futures_util::stream::{FuturesUnordered, StreamExt};

use crate::tools::classify::classify_has_schema_type;
use crate::tools::extract::{extract_page_content, extract_social_content, extract_social_parts};
use crate::tools::fetch::{fetch_social, fetch_strategy, is_url_allowed};
use crate::tools::map::map_children;
use crate::tools::normalize::normalize_social;
use crate::types::{fetch_cache_new, CanonicalUrl, Context, Jsonld, PageContent, CTX, FETCH_CACHE};

/// Upper bound on `qrawl_many_urls` qrawl depth — a guardrail against a runaway
/// expansion; `ctx.depth` is clamped to this.
const MAX_DEPTH: usize = 3;

/// Fetch and normalize **many** URLs concurrently — fast, best-effort, and
/// optionally qrawling `ctx.depth` levels deep.
///
/// Runs a bounded concurrent qrawl (up to `ctx.concurrency` fetches in flight,
/// never sequential):
/// - `ctx.depth == 0`: fetch + extract exactly the given URLs (no child discovery).
/// - `ctx.depth >= 1`: also `map_children` each fetched page and follow those, up
///   to `ctx.depth` levels (clamped to `MAX_DEPTH`), deduped by canonical URL.
///
/// A recognized social *item* routes through its dedicated side door (Reddit
/// `.json` / oEmbed), same as [`qrawl_any_url`](super::qrawl_any_url); everything
/// else takes the **lean** page path — one fetch and a single `scrape_all`
/// parse (unified schema and Open Graph, no body text). URLs that fail to
/// classify or fetch are dropped, so the returned `Vec` holds only successes, in
/// completion order (not input order).
///
/// All knobs live on [`Context`]:
/// - `allow_domains`/`block_domains` filter *throughout* — prefetch, on seeds
///   and every child, so blocked hosts are never fetched.
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
/// ```ignore
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
///     .with_schemas(vec!["Recipe".to_string()]);
/// let recipes = qrawl_many_urls(seeds, ctx).await;
/// # }
/// ```
pub async fn qrawl_many_urls(urls: Vec<String>, ctx: Context) -> Vec<PageContent> {
    let ctx_arc = Arc::new(ctx);
    let cache = fetch_cache_new();
    let ctx_for_qrawl = ctx_arc.clone();

    // Scope CTX + FETCH_CACHE once around the whole qrawl: every concurrent
    // fetch reads the same `ctx` and shares one fetch cache, so a URL reached
    // via two parents isn't fetched twice. Domain filters live on `ctx`, so the
    // fetch layer enforces them too (defense in depth vs. the prefetch check).
    CTX.scope(ctx_arc, async move {
        FETCH_CACHE
            .scope(cache, async move { qrawl(urls, &ctx_for_qrawl).await })
            .await
    })
    .await
}

/// Bounded-concurrency, depth-bounded breadth-first qrawl. Yields one
/// [`PageContent`] per successfully fetched matching page and stops as soon as
/// `ctx.limit` is reached (cancelling in-flight fetches). Every URL is gated on
/// the domain allow/block lists *and* the URL allow/block lists. Must run inside
/// a `CTX` / `FETCH_CACHE` scope.
async fn qrawl(urls: Vec<String>, ctx: &Context) -> Vec<PageContent> {
    let concurrency = ctx.concurrency.max(1);
    let depth = ctx.depth.min(MAX_DEPTH);
    let allow_d = (!ctx.allow_domains.is_empty()).then_some(ctx.allow_domains.as_slice());
    let block_d = (!ctx.block_domains.is_empty()).then_some(ctx.block_domains.as_slice());
    let allow_u: Option<HashSet<String>> = (!ctx.allow_urls.is_empty())
        .then(|| ctx.allow_urls.iter().map(|u| normalize_social(u)).collect());

    // A canonical URL passes the gate when its host clears the domain allow/block
    // lists AND (if a URL allowlist is set) it's explicitly allowed.
    let gate = |canonical: &str| -> bool {
        is_url_allowed(canonical, allow_d, block_d)
            && allow_u.as_ref().map_or(true, |set| set.contains(canonical))
    };

    // Seed the dedup set with `block_urls` (canonical): they're skipped prefetch
    // at every level and never returned. `seen` does double duty (dedup + URL
    // blocklist), intentionally.
    let mut seen: HashSet<String> = ctx.block_urls.iter().map(|u| normalize_social(u)).collect();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    for url in urls.iter().map(|u| normalize_social(u)) {
        if gate(&url) && seen.insert(url.clone()) {
            queue.push_back((url, 0));
        }
    }

    let mut in_flight = FuturesUnordered::new();
    let mut results: Vec<PageContent> = Vec::new();

    loop {
        // Saturate up to `concurrency` fetches in flight.
        while in_flight.len() < concurrency {
            match queue.pop_front() {
                Some((url, level)) => in_flight.push(process_node(url, level, level < depth)),
                None => break,
            }
        }
        let Some(node) = in_flight.next().await else {
            break;
        };
        let Some((page, children, level)) = node else {
            continue; // classify/fetch failure — drop it
        };

        // Schema filter decides RESULTS; children are expanded regardless, so a
        // non-matching roundup still yields its matches.
        if schema_passes(&page.structured, &ctx.include_schemas, &ctx.exclude_schemas) {
            results.push(page);
            if ctx.limit.is_some_and(|cap| results.len() >= cap) {
                break; // early-exit: in-flight fetches are cancelled on drop
            }
        }
        for child in children {
            let canonical = normalize_social(&child);
            if gate(&canonical) && seen.insert(canonical.clone()) {
                queue.push_back((canonical, level + 1));
            }
        }
    }

    results
}

/// Whether a page's structured data passes the allow/block schema filters.
/// Empty `allow` = any type; a page is dropped if it carries any `block` type.
/// Matching is normalization-aware (short name / IRI / CURIE) via
/// [`classify_has_schema_type`].
pub(super) fn schema_passes(structured: &Jsonld, allow: &[String], block: &[String]) -> bool {
    let allowed = allow.is_empty()
        || allow
            .iter()
            .any(|t| classify_has_schema_type(structured, t));
    let blocked = block
        .iter()
        .any(|t| classify_has_schema_type(structured, t));
    allowed && !blocked
}

/// Fetch + lean-extract one node, plus its child URLs when `with_children`.
/// The same tool composition as [`qrawl_any_url`](super::qrawl_any_url) — the
/// social door, else the page path — with the crawl's own policies on top:
/// bodies aren't carried, and the page path yields children to expand (a
/// side-door hit is a leaf). `None` on fetch failure (the crawler drops it).
/// `level` is echoed back so the crawler can tag discovered children `level + 1`.
async fn process_node(
    url: String,
    level: usize,
    with_children: bool,
) -> Option<(PageContent, Vec<String>, usize)> {
    let url = CanonicalUrl::new(&url);
    let social = extract_social_parts(url.as_str());

    // Trace each crawled URL (seeds at depth 0, discovered children at depth
    // 1+) so `RUST_LOG=qrawl=info` surfaces the full set. Off by default.
    if let Some(s) = social.as_ref() {
        if let Some((source, bytes)) = fetch_social(s, &url).await {
            if let Some(page) =
                extract_social_content(&url, s, source, &String::from_utf8_lossy(&bytes))
            {
                log::info!("depth {} {} (side door)", level, url.as_str());
                return Some((page, Vec::new(), level));
            }
        }
    }

    let html = fetch_strategy(url.as_str()).await.ok()?;
    log::info!("depth {} {}", level, url.as_str());
    let mut page = extract_page_content(&url, social.as_ref(), &html).await;
    // Lean bulk policy: bodies aren't carried across a crawl.
    page.body = None;
    let children = if with_children {
        map_children(&html, url.as_str()).await
    } else {
        Vec::new()
    };
    Some((page, children, level))
}
