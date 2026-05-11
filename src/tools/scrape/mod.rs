//! Scrape Tools

use crate::types::{Jsonld, Metadata};

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

/// Scrape both JSON-LD and metadata in a single parse.
///
/// Equivalent to `tokio::join!(scrape_jsonld(html), scrape_metadata(html))` but
/// parses the HTML once instead of twice. Prefer this when you need both.
pub async fn scrape_all(html: &str) -> (Jsonld, Metadata) {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        let document = scraper::Html::parse_document(&html);
        (
            utils::scrape_jsonld_from_doc(&document),
            utils::scrape_metadata_from_doc(&document),
        )
    })
    .await
    .expect("scrape_all: spawn_blocking failed")
}
