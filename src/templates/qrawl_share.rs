//! Share-extension pipeline.
//!
//! Turns a shared URL (plus optional caption) into a [`PageContent`] by running
//! [`qrawl_any_url`](super::qrawl_any_url) on it and layering the host caption
//! on top. A shared URL is *just a URL* — the extraction is the universal
//! pipeline; only the caption (and the degrade-to-caption when nothing fetches)
//! is share-specific.

use crate::errors::QrawlError;
use crate::tools::extract::extract_social_parts;
use crate::types::{CanonicalUrl, ContentSource, Context, PageContent};

use super::qrawl_any_url::qrawl_any_url;

/// Turn a shared URL (plus optional caption) into a [`PageContent`].
///
/// Runs the universal pipeline ([`qrawl_any_url`](super::qrawl_any_url)) on
/// `url`, then uses the caption as the body fallback when the page carried no
/// text of its own. Degrades to the caption alone when nothing could be
/// fetched; only errors when the URL cannot be parsed.
///
/// ```ignore
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
    let canonical = CanonicalUrl::new(url);
    // Validate up front so a *fetch* miss can degrade to the caption while a
    // *malformed* URL still errors (`qrawl_any_url` conflates the two).
    if url::Url::parse(canonical.as_str()).is_err() {
        return Err(QrawlError::new(format!("invalid URL: {canonical}")));
    }
    let caption = caption
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // The universal pipeline handles ANY url; a total miss degrades to the
    // caption alone rather than erroring (the URL already parsed).
    let page = match qrawl_any_url(url, ctx).await {
        Ok(page) => page,
        Err(_) => share_payload_only(&canonical),
    };
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
