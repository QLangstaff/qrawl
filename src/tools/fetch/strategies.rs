use super::client::build_client;
use super::headers::headers_for_strategy;
use super::types::*;
use super::utils::*;
use reqwest::Client;
use std::time::Instant;
use url::Url;

/// Fetch HTML from a URL using adaptive strategy (tries multiple approaches until one succeeds).
///
/// Tries strategies in order: Minimal → Browser → Mobile → Stealth → Extreme
/// Each strategy attempted twice: once without referer, once with referer (except Extreme which has custom logic).
///
/// Returns FetchResult with HTML and metadata on success, error message on failure.
pub(crate) async fn fetch_with_adaptive_strategy(url: &str) -> Result<FetchResult, String> {
    let start = Instant::now();
    let strategies = [
        FetchStrategy::Minimal,
        FetchStrategy::Browser,
        FetchStrategy::Mobile,
        FetchStrategy::Stealth,
        FetchStrategy::Extreme,
    ];

    // Extract origin for referer header
    let origin = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| format!("{}://{}/", u.scheme(), h)));

    let mut last_error = None;
    let mut attempts = 0;

    for (strategy_idx, &strategy) in strategies.iter().enumerate() {
        // Extreme has its own custom multi-step logic
        if strategy == FetchStrategy::Extreme {
            attempts += 1;
            match fetch_extreme(url).await {
                Ok(html) => {
                    return Ok(FetchResult {
                        html,
                        strategy_used: FetchStrategy::Extreme,
                        duration_ms: start.elapsed().as_millis() as u64,
                        attempts,
                    });
                }
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        let client = match build_client(strategy) {
            Ok(c) => c,
            Err(e) => {
                last_error = Some(format!("Client build failed: {}", e));
                continue;
            }
        };

        // First try: without referer
        attempts += 1;
        match fetch_with_strategy_and_referer(&client, url, strategy, None).await {
            Ok(html) => {
                return Ok(FetchResult {
                    html,
                    strategy_used: strategy,
                    duration_ms: start.elapsed().as_millis() as u64,
                    attempts,
                });
            }
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
                attempts += 1;
                match fetch_with_strategy_and_referer(&client, url, strategy, Some(referer)).await {
                    Ok(html) => {
                        return Ok(FetchResult {
                            html,
                            strategy_used: strategy,
                            duration_ms: start.elapsed().as_millis() as u64,
                            attempts,
                        });
                    }
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

/// Fetch with extreme bot evasion: session building, external referrers, long delays.
///
/// This is the most aggressive strategy, used when sites employ sophisticated bot detection
/// like PerimeterX or DataDome that track session context and referrer trust.
///
/// Techniques:
/// 1. Visit homepage to build session/cookies
/// 2. Try with Google search referrer (organic traffic simulation)
/// 3. Try with Facebook referrer (social media simulation)
/// 4. Long delays between attempts (500-1500ms)
/// 5. Final Stealth fallback
async fn fetch_extreme(url: &str) -> Result<String, String> {
    let stealth_client =
        build_client(FetchStrategy::Stealth).map_err(|e| format!("Client build failed: {}", e))?;

    // Step 1: Extract homepage URL for session building
    let homepage = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| format!("{}://{}/", u.scheme(), h)))
        .ok_or_else(|| "Invalid URL for extreme fetch".to_string())?;

    // Step 2: Visit homepage to build session context (cookies, tracking tokens)
    // Ignore errors - session building is best-effort
    let _ =
        fetch_with_strategy_and_referer(&stealth_client, &homepage, FetchStrategy::Stealth, None)
            .await;

    // Delay to simulate human browsing (homepage → article navigation)
    tokio::time::sleep(tokio::time::Duration::from_millis(500 + jitter_ms(200))).await;

    // Step 3: Try with Google search referrer (appear as organic search traffic)
    let google_referrer = format!(
        "https://www.google.com/search?q=site:{}",
        Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "recipe".to_string())
    );

    if let Ok(html) = fetch_with_strategy_and_referer(
        &stealth_client,
        url,
        FetchStrategy::Stealth,
        Some(&google_referrer),
    )
    .await
    {
        return Ok(html);
    }

    // Step 4: Long delay to simulate slow human connection
    tokio::time::sleep(tokio::time::Duration::from_millis(1000 + jitter_ms(500))).await;

    // Step 5: Try with Facebook referrer (appear as social media traffic)
    if let Ok(html) = fetch_with_strategy_and_referer(
        &stealth_client,
        url,
        FetchStrategy::Stealth,
        Some("https://www.facebook.com/"),
    )
    .await
    {
        return Ok(html);
    }

    // Step 6: Another delay before final attempt
    tokio::time::sleep(tokio::time::Duration::from_millis(800 + jitter_ms(400))).await;

    // Step 7: Final attempt with Stealth strategy (no special referrer)
    fetch_with_strategy_and_referer(&stealth_client, url, FetchStrategy::Stealth, None).await
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
    if let Err(reason) = validate_response(status_code, &html) {
        return Err(format!(
            "Failed to fetch with strategy {:?}: status {} ({})",
            strategy,
            status_code.as_u16(),
            reason
        ));
    }

    Ok(html)
}

/// Fetch HTML with a single strategy (no fallback retries).
///
/// Use this when you want speed over reliability. Ideal for bulk fetching
/// where some failures are acceptable (e.g., fetching 50 recipe URLs for search results).
///
/// Returns FetchResult with HTML and metadata.
pub(crate) async fn fetch_with_single_strategy(
    url: &str,
    strategy: FetchStrategy,
) -> Result<FetchResult, String> {
    let start = Instant::now();

    if strategy == FetchStrategy::Adaptive {
        // Adaptive means use the full adaptive_fetch flow
        return fetch_with_adaptive_strategy(url).await;
    }

    if strategy == FetchStrategy::Extreme {
        // Extreme uses special multi-step logic (has its own timing)
        let html = fetch_extreme(url).await?;
        return Ok(FetchResult {
            html,
            strategy_used: FetchStrategy::Extreme,
            duration_ms: start.elapsed().as_millis() as u64,
            attempts: 1,
        });
    }

    let client = build_client(strategy).map_err(|e| format!("Client build failed: {}", e))?;

    // Single attempt without referer (fastest)
    let html = fetch_with_strategy_and_referer(&client, url, strategy, None).await?;

    Ok(FetchResult {
        html,
        strategy_used: strategy,
        duration_ms: start.elapsed().as_millis() as u64,
        attempts: 1,
    })
}
