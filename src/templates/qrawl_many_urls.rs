use std::collections::HashSet;

use futures_util::stream::StreamExt;

use crate::tools::classify::classify_has_schema_type;
use crate::tools::normalize::CanonicalUrl;
use crate::types::{Context, Jsonld};

use super::types::{CrawlOptions, PageContent};
use super::utils::{crawl, page_content, Children, Visited};

/// [`qrawl_any_url`](super::qrawl_any_url) over **many** URLs — concurrent,
/// best-effort, and optionally crawling their children.
///
/// Every URL becomes a [`PageContent`] by the same steps as `qrawl_any_url`.
/// A URL that can't be fetched is dropped, so the result holds only
/// successes: level by level, in completion order within a level.
///
/// `options` ([`CrawlOptions`]):
/// - depth 0 (the default): exactly the given URLs. Depth N: also each fetched
///   page's children (`map_children`), N levels deep. Each page is returned
///   once, at the shallowest level that reaches it, however many of the URLs
///   redirect to it.
/// - allow/block URLs also apply to the URL a redirect lands on.
/// - allow/block URLs: filter seeds and children before their fetch.
/// - include/exclude schemas: filter the *results*. A page counts (toward the
///   limit too) only if its structured data includes an include-type (empty =
///   any) and none of the exclude-types, on any entity at any depth; a page
///   that doesn't count is still expanded for its children.
/// - limit: stop once this many pages are returned, cancelling the fetches in
///   flight.
///
/// `ctx`'s domain lists apply to every fetch, so a page on a filtered host is
/// never requested, returned, or expanded.
///
/// ```no_run
/// use qrawl::templates::{qrawl_many_urls, CrawlOptions};
/// use qrawl::types::Context;
///
/// # async fn run(seeds: Vec<String>) {
/// // Just these URLs.
/// let pages = qrawl_many_urls(seeds.clone(), CrawlOptions::default(), Context::fast()).await;
/// // One level deep, recipe pages only, stop after 30.
/// let options = CrawlOptions::default()
///     .with_depth(1)
///     .with_limit(30)
///     .with_include_schemas(vec!["Recipe".to_string()]);
/// let recipes = qrawl_many_urls(seeds, options, Context::fast()).await;
/// # }
/// ```
pub async fn qrawl_many_urls(
    urls: Vec<String>,
    options: CrawlOptions,
    ctx: Context,
) -> Vec<PageContent> {
    let CrawlOptions {
        depth,
        limit,
        allow_urls,
        block_urls,
        include_schemas,
        exclude_schemas,
    } = options;
    let canonical_set = |urls: &[String]| -> HashSet<CanonicalUrl> {
        urls.iter().map(|url| CanonicalUrl::new(url)).collect()
    };
    let (allow_urls, block_urls) = (canonical_set(&allow_urls), canonical_set(&block_urls));
    let admit = move |url: &CanonicalUrl| {
        (allow_urls.is_empty() || allow_urls.contains(url)) && !block_urls.contains(url)
    };

    let pages = crawl(ctx, urls, depth, admit, move |target, level| async move {
        let children = if level < depth {
            Children::Map
        } else {
            Children::Skip
        };
        let (page, children) = page_content(&target, children)
            .await
            .map_err(|e| log::debug!("{} failed: {}", target.address, e))
            .ok()?;
        log::info!("crawled {}", page.address);
        Some(Visited {
            url: page.url.clone(),
            item: page,
            children,
        })
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
