use crate::types::Context;
use crate::tools::fetch::fetch_auto;

/// Qrawl emails from URLs.
pub async fn qrawl_emails(
    urls: Vec<String>,
    ctx: Context,
) -> Result<Vec<String>, String> {
    let result = chain! {
        urls, ctx =>
        clean_urls ->
        fetch_auto ->
        extract_emails ->
        clean_emails
    }
    .await;

    Ok(result
        .into_iter()
        .flat_map(|(_, emails)| emails)
        .collect())
}
