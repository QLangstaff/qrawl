//! Map Tools

mod tests;
mod utils;

/// Map URLs from HTML.
pub async fn map_page(html: &str, base_url: &str) -> Vec<String> {
    let html = html.to_string();
    let base_url = base_url.to_string();
    tokio::task::spawn_blocking(move || {
        let base = match url::Url::parse(&base_url) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let doc = scraper::Html::parse_document(&html);
        let link_selector = match scraper::Selector::parse("a[href]") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        doc.select(&link_selector)
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
                if url.scheme() == "http" || url.scheme() == "https" {
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
pub async fn map_children(html: &str, base_url: &str) -> Vec<String> {
    let html = html.to_string();
    let base_url = base_url.to_string();
    let options = crate::pipelines::get_options();
    tokio::task::spawn_blocking(move || {
        let siblings = utils::map_siblings(&html, &base_url, &options);
        let itemlist = utils::map_itemlist(&html, &base_url, &options);
        crate::merge!(siblings, itemlist)
    })
    .await
    .expect("map_children: spawn_blocking failed")
}
