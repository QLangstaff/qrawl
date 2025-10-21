use crate::tools::fetch::fetch_auto;
use crate::types::Context;

/// Qrawl children from URLs.
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
