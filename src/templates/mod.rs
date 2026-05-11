//! Example Templates

use crate::tools::fetch::fetch_strategy;
use crate::types::Context;

/// Get children from URLs.
pub async fn qrawl_children(
    urls: Vec<String>,
    ctx: Context,
) -> Result<Vec<(String, String)>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_strategy ->
        map_children ->
        clean_urls ->
        fetch_strategy
    }
    .await;

    Ok(result)
}

/// Get emails from URLs.
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Result<Vec<String>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_strategy ->
        map_children ->
        clean_urls ->
        fetch_strategy ->
        map_page ->
        clean_urls ->
        fetch_strategy ->
        extract_emails ->
        clean_emails
    }
    .await;

    Ok(result.into_iter().map(|(_, email)| email).collect())
}
