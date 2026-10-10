use crate::tools::extract::extract_emails;
use crate::tools::map::{map_children, map_page};
use crate::tools::normalize::{normalize_emails, normalize_urls};
use crate::types::Context;

/// Get emails from URLs — fetch each seed's children, then every page those
/// children link to (on any host), and extract and dedupe the emails on those
/// last pages.
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Vec<String> {
    crate::chain! {
        urls, ctx =>
        all(normalize_urls) -> fetch -> map(map_children) ->
        all(normalize_urls) -> fetch -> map(map_page) ->
        all(normalize_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
    }
}
