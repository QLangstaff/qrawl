use crate::errors::QrawlError;
use crate::tools::classify::classify_has_schema_type;
use crate::tools::map::map_children;
use crate::tools::normalize::normalize_urls;
use crate::tools::scrape::scrape_jsonld;
use crate::types::{Context, Html};

/// Crawl from `urls` and keep only the pages whose structured data includes
/// `schema_type` — the content-type filter for a crawl (the boolean-filter
/// analogue of how [`qrawl_emails`](super::qrawl_emails) extracts).
///
/// A plain [`chain!`](crate::chain): crawl each seed's child pages, then
/// `filter` those whose structured data includes `schema_type` — matched by
/// [`classify_has_schema_type`], so a full IRI / CURIE / bare term are
/// equivalent. Returns the surviving `(url, html)` pages, so the caller can
/// extract / transform them without re-fetching.
///
/// ```ignore
/// use qrawl::templates::qrawl_schema;
/// use qrawl::types::Context;
///
/// # async fn run() {
/// // Every Product page reachable from the storefront.
/// let products =
///     qrawl_schema(vec!["https://shop.example".into()], "Product", Context::fast())
///         .await
///         .unwrap();
/// # }
/// ```
pub async fn qrawl_schema(
    urls: Vec<String>,
    schema_type: &str,
    ctx: Context,
) -> Result<Vec<(String, String)>, QrawlError> {
    let target = schema_type.to_string();
    let pages = crate::chain! {
        urls, ctx =>
        all(normalize_urls) -> fetch -> map(map_children) ->
        all(normalize_urls) -> fetch ->
        filter(move |html: Html| {
            let target = target.clone();
            async move { classify_has_schema_type(&scrape_jsonld(&html).await, &target) }
        })
    };
    Ok(pages
        .into_iter()
        .map(|(url, html)| (url, html.into_inner()))
        .collect())
}
