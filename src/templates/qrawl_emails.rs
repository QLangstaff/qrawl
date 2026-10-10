use futures_util::stream::StreamExt;

use crate::tools::extract::extract_emails;
use crate::tools::fetch::fetch_strategy;
use crate::tools::map::{map_children, map_page};
use crate::tools::normalize::normalize_emails;
use crate::types::Context;

use super::utils::crawl;

/// Get emails from URLs — fetch each seed's children, then every page those
/// children link to (on any host), and extract and dedupe the emails on those
/// last pages.
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
            Some(match level {
                0 => (Vec::new(), map_children(&html, &target.address).await),
                1 => (Vec::new(), map_page(&html, &target.address).await),
                _ => (extract_emails(&html).await, Vec::new()),
            })
        },
    )
    .map(|(_level, emails)| emails)
    .collect()
    .await;
    normalize_emails(&emails.concat())
}
