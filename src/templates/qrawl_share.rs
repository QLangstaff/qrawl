use crate::errors::QrawlError;
use crate::tools::extract::extract_social_parts;
use crate::types::{CanonicalUrl, ContentSource, Context, PageContent};

use super::utils::{parse_url, resolve_url};

/// Turn a shared URL (plus optional caption) into a [`PageContent`].
///
/// Resolves `url` the way [`qrawl_any_url`](super::qrawl_any_url) does, then
/// uses the caption as the text when the page carried none. Degrades to the
/// caption alone when nothing could be fetched; errors only when the URL
/// cannot be parsed.
///
/// ```no_run
/// use qrawl::templates::qrawl_share;
/// use qrawl::types::Context;
///
/// # async fn run() {
/// let page = qrawl_share(
///     "https://www.tiktok.com/@user/video/12345",
///     Some("check this out".to_string()),
///     Context::fast(),
/// )
/// .await
/// .unwrap();
/// # }
/// ```
pub async fn qrawl_share(
    url: &str,
    caption: Option<String>,
    ctx: Context,
) -> Result<PageContent, QrawlError> {
    let url = parse_url(url)?;
    let page = match ctx.scope(resolve_url(&url)).await {
        Ok((page, _html)) => page,
        Err(_) => share_payload_only(&url),
    };
    let caption = caption
        .as_deref()
        .map(str::trim)
        .filter(|caption| !caption.is_empty())
        .map(str::to_string);
    Ok(with_caption(page, caption))
}

/// Layer a share caption onto a page: the caption is the body fallback when the
/// page carried no text of its own. A caption has no title of its own —
/// inventing one from its first line would be misleading — so `title` is left as
/// the page found it.
pub(super) fn with_caption(mut page: PageContent, caption: Option<String>) -> PageContent {
    if page.text.is_none() {
        page.text = caption;
    }
    page
}

/// A minimal [`PageContent`] for a URL nothing could be fetched from — carries
/// only the URL, its classification, and the classified handle as author, tagged
/// [`ContentSource::SharePayloadOnly`].
pub(super) fn share_payload_only(url: &CanonicalUrl) -> PageContent {
    let social = extract_social_parts(url.as_str());
    PageContent::new(url.clone(), ContentSource::SharePayloadOnly).with_social(social.as_ref())
}
