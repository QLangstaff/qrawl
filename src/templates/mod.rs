//! Example Templates

use crate::tools::fetch::fetch_auto;
use crate::types::Context;

/// Get children from URLs.
pub async fn qrawl_children(
    urls: Vec<String>,
    ctx: Context,
) -> Result<Vec<(String, String)>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_auto ->
        map_children ->
        clean_urls ->
        fetch_auto
    }
    .await;

    Ok(result)
}

/// Get emails from URLs.
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Result<Vec<String>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_auto ->
        map_children ->
        clean_urls ->
        fetch_auto ->
        map_page ->
        clean_urls ->
        fetch_auto ->
        extract_emails ->
        clean_emails
    }
    .await;

    Ok(result.into_iter().map(|(_, email)| email).collect())
}
