//! Map Tools

mod tests;
mod utils;

use crate::selectors::LINK_SELECTOR;

/// Map URLs from HTML.
pub async fn map_page(html: &str, url: &str) -> Vec<String> {
    let html = html.to_string();
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        let base = match url::Url::parse(&url) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let doc = scraper::Html::parse_document(&html);

        doc.select(&LINK_SELECTOR)
            .filter_map(|link| {
                let href = link
                    .value()
                    .attr("href")?
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();

                // Handle protocol-relative URLs (//example.com/path)
                let url = if href.starts_with("//") {
                    let full_href = format!("{}:{}", base.scheme(), href);
                    url::Url::parse(&full_href).ok()?
                } else {
                    url::Url::parse(href)
                        .ok()
                        .or_else(|| base.join(href).ok())?
                };

                // Only accept HTTP and HTTPS schemes
                if matches!(url.scheme(), "http" | "https") {
                    Some(url.to_string())
                } else {
                    None
                }
            })
            .collect()
    })
    .await
    .expect("map_page: spawn_blocking failed")
}

/// Map child URLs from HTML.
pub async fn map_children(html: &str, url: &str) -> Vec<String> {
    let html = html.to_string();
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        let siblings = utils::map_siblings(&html, &url);
        let itemlist = utils::map_itemlist(&html, &url);
        let mut result = crate::merge!(siblings, itemlist);
        if result.is_empty() {
            result = vec![url];
        }
        result
    })
    .await
    .expect("map_children: spawn_blocking failed")
}
