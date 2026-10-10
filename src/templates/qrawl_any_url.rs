use crate::errors::QrawlError;
use crate::types::{Context, PageContent};

use super::utils::{parse_url, resolve_url};

/// Fetch and normalize **one** URL thoroughly into a [`PageContent`].
///
/// A recognized social *item* (Reddit post, TikTok/YouTube/Pinterest/Vimeo
/// video or pin) goes through its dedicated side door (Reddit `.json` /
/// oEmbed); everything else — blogs, profiles, Instagram/Facebook, and any
/// non-social page — is fetched and scraped for structured data (JSON-LD +
/// Microdata + RDFa + Microformats2), `<meta>` tags, the social links on the
/// page, and body HTML.
///
/// Domain-agnostic: the caller maps the result to its own schema (see
/// [`PageContent`]). Honors `ctx`'s fetch strategy and domain filters. Returns
/// `Err` on an unparseable input URL, before any network, or when neither a
/// side door nor a page fetch yields anything.
///
/// ```no_run
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
    let target = parse_url(url)?;
    let (page, _html) = ctx.scope(resolve_url(&target)).await?;
    Ok(page)
}
