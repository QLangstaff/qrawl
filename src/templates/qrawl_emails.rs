use crate::errors::QrawlError;
use crate::tools::extract::extract_emails;
use crate::tools::map::{map_children, map_page};
use crate::tools::normalize::{normalize_emails, normalize_urls};
use crate::types::Context;

/// Get emails from URLs — crawl each seed's children and their pages, then
/// extract and dedupe every email across the crawl.
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Result<Vec<String>, QrawlError> {
    Ok(crate::chain! {
        urls, ctx =>
        all(normalize_urls) -> fetch -> map(map_children) ->
        all(normalize_urls) -> fetch -> map(map_page) ->
        all(normalize_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
    })
}
