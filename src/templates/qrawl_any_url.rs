use crate::errors::QrawlError;
use crate::types::Context;

use super::types::PageContent;
use super::utils::{page_content, parse_url, Children};

/// Turn **any** URL into a [`PageContent`]: the one template, a plain
/// composition of the tools that runs the same steps for a blog post, a store
/// page, and a TikTok video.
///
/// 1. `fetch`: the page (`fetch_strategy`) and, when the platform serves one,
///    the item's embed (`fetch_embed`).
/// 2. `scrape`: both into the same layers — body, metadata pairs, schema.org
///    entities (`scrape_all`, `scrape_embed`).
/// 3. `map` + `normalize`: the page's links, canonical and deduped, kept when
///    they're social profiles or items.
/// 4. `extract`: the content fields (`extract_preview`) and the URL's social
///    facts (`extract_social_parts`).
///
/// Domain-agnostic: the caller maps the result to its own schema. Honors
/// `ctx`'s fetch strategy and domain filters. Returns `Err` on an input that
/// isn't an HTTP URL, before any network, or when neither the page nor an
/// embed could be fetched.
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
    let (page, _children) = ctx.scope(page_content(&target, Children::Skip)).await?;
    Ok(page)
}
