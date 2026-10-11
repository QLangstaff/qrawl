//! Map Tools

mod tests;
mod utils;

use crate::types::Html;
use crate::utils::{with_document, LINK_SELECTOR};

/// Map URLs from HTML: every `http(s)` link on the page, resolved against
/// `url`.
pub async fn map_page(html: &Html, url: &str) -> Vec<String> {
    let url = url.to_string();
    with_document(html, move |doc| map_page_from_doc(doc, &url)).await
}

/// [`map_page`] on a parsed document.
pub(crate) fn map_page_from_doc(doc: &scraper::Html, url: &str) -> Vec<String> {
    let Ok(base) = url::Url::parse(url) else {
        return Vec::new();
    };
    doc.select(&LINK_SELECTOR)
        .filter_map(|link| utils::resolve_href(link.value().attr("href")?, &base))
        .map(String::from)
        .collect()
}

/// Map child URLs from HTML: the items of the page's main listing (repeated
/// siblings, a JSON-LD `ItemList`). Empty when the page lists nothing.
pub async fn map_children(html: &Html, url: &str) -> Vec<String> {
    let url = url.to_string();
    with_document(html, move |doc| map_children_from_doc(doc, &url)).await
}

/// [`map_children`] on a parsed document.
pub(crate) fn map_children_from_doc(doc: &scraper::Html, url: &str) -> Vec<String> {
    [
        utils::map_siblings_from_doc(doc, url),
        utils::map_itemlist_from_doc(doc, url),
    ]
    .concat()
}
