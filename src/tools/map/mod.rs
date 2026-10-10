//! Map Tools

mod tests;
mod utils;

use crate::types::Html;
use crate::utils::{with_document, LINK_SELECTOR};

/// Map URLs from HTML: every `http(s)` link on the page, resolved against
/// `url`.
pub async fn map_page(html: &Html, url: &str) -> Vec<String> {
    let Ok(base) = url::Url::parse(url) else {
        return Vec::new();
    };
    with_document(html, move |doc| {
        doc.select(&LINK_SELECTOR)
            .filter_map(|link| utils::resolve_href(link.value().attr("href")?, &base))
            .map(String::from)
            .collect()
    })
    .await
}

/// Map child URLs from HTML: the items of the page's main listing (repeated
/// siblings, a JSON-LD `ItemList`). Empty when the page lists nothing.
pub async fn map_children(html: &Html, url: &str) -> Vec<String> {
    let url = url.to_string();
    with_document(html, move |doc| {
        [
            utils::map_siblings_from_doc(doc, &url),
            utils::map_itemlist_from_doc(doc, &url),
        ]
        .concat()
    })
    .await
}
