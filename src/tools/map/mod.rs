//! Map link relationships and resolve URLs.
//!
//! See README.md for full documentation.

pub mod cli;
#[cfg(test)]
mod tests;
mod utils;

use scraper::{Html, Selector};
use url::Url;

/// Map URLs from HTML fragments to absolute URLs.
///
/// For each HTML fragment, finds the FIRST link that can be successfully
/// resolved to an absolute URL. Skips fragments with no resolvable links.
///
/// # Arguments
/// * `html_fragments` - HTML fragments (e.g., from parse_children)
/// * `base_url` - Base URL for resolving relative URLs
///
/// # Returns
/// Vec of absolute URL strings (one per fragment with resolvable link)
///
/// # Examples
/// ```rust
/// use qrawl::tools::{parse::parse_children, map::map_urls};
///
/// # fn example() {
/// let html = r#"<html>...</html>"#;
/// let base = "https://example.com/page";
///
/// let children = parse_children(html);
/// let urls = map_urls(&children, base);
/// # }
/// ```
pub fn map_urls(html_fragments: &[String], base_url: &str) -> Vec<String> {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };

    let link_selector = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    html_fragments
        .iter()
        .filter_map(|html| {
            let doc = Html::parse_fragment(html);

            // Collect all valid links from this fragment
            let links: Vec<(String, Url)> = doc
                .select(&link_selector)
                .filter_map(|link| {
                    let href = link.value().attr("href")?.trim();

                    // Handle protocol-relative URLs (//example.com/path)
                    let url = if href.starts_with("//") {
                        // Prepend the base URL's scheme
                        let full_href = format!("{}:{}", base.scheme(), href);
                        Url::parse(&full_href).ok()?
                    } else {
                        // Try absolute first, then relative
                        Url::parse(href).ok().or_else(|| base.join(href).ok())?
                    };

                    // Only accept HTTP and HTTPS schemes
                    let scheme = url.scheme();
                    if scheme != "http" && scheme != "https" {
                        return None;
                    }

                    Some((href.to_string(), url))
                })
                .collect();

            if links.is_empty() {
                return None;
            }

            // Return first valid link
            links.into_iter().next().map(|(_, url)| url.to_string())
        })
        .collect()
}
