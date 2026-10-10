//! The universal single-URL pipeline: try the social door
//! ([`fetch_social`] → [`extract_social_content`]), else fetch and extract the
//! page ([`fetch_strategy`] → [`extract_page_content`]). Pure composition —
//! all social routing, parsing, and assembly live in the fetch and extract
//! tools.

use std::sync::Arc;

use crate::errors::QrawlError;
use crate::tools::extract::{extract_page_content, extract_social_content, extract_social_parts};
use crate::tools::fetch::{fetch_social, fetch_strategy};
use crate::types::{
    fetch_cache_new, CanonicalUrl, Context, PageContent, SocialParts, CTX, FETCH_CACHE,
};

/// Validate + classify a single URL, then run `body` inside this pipeline's
/// `CTX` + `FETCH_CACHE` scope. The fetch layer reads `ctx` task-locally and
/// silently ignores it if no scope is active, so the scope must wrap every
/// fetch. Returns `Err` on an unparseable URL, before any network.
async fn scoped_url<F, Fut, T>(url: &str, ctx: Context, body: F) -> Result<T, QrawlError>
where
    F: FnOnce(CanonicalUrl, Option<SocialParts>) -> Fut,
    Fut: std::future::Future<Output = Result<T, QrawlError>>,
{
    let canonical = CanonicalUrl::new(url);
    if url::Url::parse(canonical.as_str()).is_err() {
        return Err(QrawlError::new(format!("invalid URL: {canonical}")));
    }
    let social = extract_social_parts(canonical.as_str());
    let ctx_arc = Arc::new(ctx);
    let cache = fetch_cache_new();
    CTX.scope(ctx_arc, async move {
        FETCH_CACHE
            .scope(cache, async move { body(canonical, social).await })
            .await
    })
    .await
}

/// Fetch and normalize **one** URL thoroughly into a [`PageContent`].
///
/// Classifies the URL, then resolves it: a recognized social *item* (Reddit
/// post, TikTok/YouTube/Pinterest/Vimeo video or pin) goes through its dedicated
/// side door (Reddit `.json` / oEmbed); everything else — food blogs, profiles,
/// Instagram/Facebook, and any non-social page — is fetched and scraped for
/// structured data (JSON-LD + Microdata + RDFa), Open Graph metadata, the
/// social links found on the page, and body HTML.
///
/// Domain-agnostic: the caller maps the result to its own schema (see
/// [`PageContent`]). Honors `ctx` (fetch strategy / timeout / domain filters),
/// run inside this pipeline's `CTX` + `FETCH_CACHE` scope. Returns `Err` on an
/// unparseable input URL, or when nothing — neither a side door nor a page
/// fetch — yields anything.
///
/// ```ignore
/// use qrawl::templates::qrawl_any_url;
/// use qrawl::types::Context;
///
/// # async fn run() {
/// let page = qrawl_any_url("https://www.allrecipes.com/recipe/10813", Context::auto())
///     .await
///     .unwrap();
/// // page.structured carries the schema.org Recipe, if the page exposes one.
/// # }
/// ```
pub async fn qrawl_any_url(url: &str, ctx: Context) -> Result<PageContent, QrawlError> {
    scoped_url(url, ctx, |url, social| async move {
        // A recognized social *item* goes through its dedicated door first. On
        // a miss (no door, door failed, nothing meaningful) fall through to
        // the page path.
        if let Some(s) = social.as_ref() {
            if let Some((source, bytes)) = fetch_social(s, &url).await {
                if let Some(page) =
                    extract_social_content(&url, s, source, &String::from_utf8_lossy(&bytes))
                {
                    return Ok(page);
                }
            }
        }
        let html = fetch_strategy(url.as_str()).await?;
        // Trace crawled pages; a single URL is always depth 0, matching the
        // crawl spine's trace.
        log::info!("depth 0 {}", url.as_str());
        Ok(extract_page_content(&url, social.as_ref(), &html).await)
    })
    .await
}
