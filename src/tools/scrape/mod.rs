//! Scrape Tools

use crate::tools::types::{Jsonld, Metadata};

mod utils;

/// Scrape body content from HTML.
pub async fn scrape_body(html: &str) -> String {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_body_content(&html))
        .await
        .expect("scrape_body: spawn_blocking failed")
}

/// Scrape JSON-LD scripts from HTML.
pub async fn scrape_jsonld(html: &str) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_jsonld_scripts(&html))
        .await
        .expect("scrape_jsonld: spawn_blocking failed")
}

/// Scrape metadata tags from HTML.
pub async fn scrape_metadata(html: &str) -> Metadata {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_metadata_tags(&html))
        .await
        .expect("scrape_metadata: spawn_blocking failed")
}
