#![doc = include_str!("./README.md")]

pub mod types;
mod utils;

pub use types::*;
use utils::*;

use crate::tools::types::{Jsonld, Metadata};

/// Scrape everything (body + JSON-LD + metadata) from HTML.
pub fn scrape(html: &str) -> ScrapeResult {
    ScrapeResult {
        body: scrape_body_content(html),
        jsonld: scrape_jsonld_scripts(html),
        metadata: scrape_metadata_tags(html),
    }
}

/// Scrape body from HTML.
pub fn scrape_body(html: &str) -> String {
    scrape_body_content(html)
}

/// Scrape JSON-LD from HTML.
pub fn scrape_jsonld(html: &str) -> Jsonld {
    scrape_jsonld_scripts(html)
}

/// Scrape metadata from HTML.
pub fn scrape_metadata(html: &str) -> Metadata {
    scrape_metadata_tags(html)
}
