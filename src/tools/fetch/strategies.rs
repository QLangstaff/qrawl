use super::client::build_client;
use super::headers::headers_for_strategy;
use super::retry::{is_valid_response, jitter_ms};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

/// Strategy for fetching content with different levels of bot evasion (internal only).
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FetchStrategy {
    Minimal,
    Browser,
    Mobile,
    Stealth,
}

/// Fetch HTML from a URL using adaptive strategy (tries multiple approaches until one succeeds).
///
/// Tries strategies in order: Minimal → Browser → Mobile → Stealth
/// Each strategy attempted twice: once without referer, once with referer.
///
/// Returns HTML content on success, error message on failure.
pub(crate) async fn adaptive_fetch(url: &str) -> Result<String, String> {
    let strategies = [
        FetchStrategy::Minimal,
        FetchStrategy::Browser,
        FetchStrategy::Mobile,
        FetchStrategy::Stealth,
    ];

    // Extract origin for referer header
    let origin = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| format!("{}://{}/", u.scheme(), h)));

    let mut last_error = None;

    for (strategy_idx, &strategy) in strategies.iter().enumerate() {
        let client = match build_client(strategy) {
            Ok(c) => c,
            Err(e) => {
                last_error = Some(format!("Client build failed: {}", e));
                continue;
            }
        };

        // First try: without referer
        match fetch_with_strategy_and_referer(&client, url, strategy, None).await {
            Ok(html) => return Ok(html),
            Err(e) => {
                last_error = Some(e);

                // Small delay after first attempt
                if strategy_idx == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(80 + jitter_ms(120)))
                        .await;
                }
            }
        }

        // Second try: with referer (skip for Minimal strategy to keep it truly minimal)
        if strategy != FetchStrategy::Minimal {
            if let Some(ref referer) = origin {
                match fetch_with_strategy_and_referer(&client, url, strategy, Some(referer)).await {
                    Ok(html) => return Ok(html),
                    Err(e) => {
                        last_error = Some(e);
                    }
                }
            }

            // Delay between attempts within same strategy
            tokio::time::sleep(tokio::time::Duration::from_millis(120 + jitter_ms(160))).await;
        }

        // Delay between strategies (except after last)
        if strategy_idx < strategies.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(300 + jitter_ms(200))).await;
        }
    }

    // All strategies failed
    Err(last_error.unwrap_or_else(|| format!("Failed to fetch url {}", url)))
}

/// Fetch URL with a specific strategy and optional referer header.
/// Uses is_valid_response to check for bot challenges and invalid content.
async fn fetch_with_strategy_and_referer(
    client: &Client,
    url: &str,
    strategy: FetchStrategy,
    referer: Option<&str>,
) -> Result<String, String> {
    // Build headers for this strategy
    let mut headers = headers_for_strategy(strategy);

    // Add referer if provided
    if let Some(ref_url) = referer {
        if let Ok(ref_value) = reqwest::header::HeaderValue::from_str(ref_url) {
            headers.insert(reqwest::header::REFERER, ref_value);
        }
    }

    // Make request
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // Check status
    let status_code = response.status();

    // Get body
    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    // Validate response content (checks for bot challenges, access denied, invalid content, etc.)
    // Returns false, allowing fallback to next strategy
    if !is_valid_response(status_code, &html) {
        return Err(format!(
            "Failed to fetch with strategy {:?}: invalid response (status {})",
            strategy,
            status_code.as_u16()
        ));
    }

    Ok(html)
}
