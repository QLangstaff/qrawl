use futures_util::stream::StreamExt;

use crate::tools::extract::extract_emails;
use crate::tools::fetch::fetch_strategy;
use crate::tools::map::{map_children, map_page};
use crate::tools::normalize::normalize_emails;
use crate::types::Context;

use super::utils::crawl;

/// Get emails from URLs — the deduped emails on each seed, its children, and
/// every page those children link to (on any host).
pub async fn qrawl_emails(urls: Vec<String>, ctx: Context) -> Vec<String> {
    let emails: Vec<Vec<String>> = crawl(
        ctx,
        urls,
        2,
        |_url| true,
        |target, level| async move {
            let html = fetch_strategy(&target.address)
                .await
                .map_err(|e| log::debug!("fetch failed {}: {}", target.address, e))
                .ok()?;
            let links = match level {
                0 => map_children(&html, &target.address).await,
                _ => map_page(&html, &target.address).await,
            };
            Some((extract_emails(&html).await, links))
        },
    )
    .map(|(_level, emails)| emails)
    .collect()
    .await;
    normalize_emails(&emails.concat())
}
