use super::client::build_client_for_profile;
use super::headers::headers_for_profile;
use super::profile::FetchProfile;
use super::types::*;
use super::utils::*;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};

static CLIENT_CACHE: Lazy<Arc<DashMap<FetchProfile, Client>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

const ADAPTIVE_PROFILES: [FetchProfile; 3] = [
    FetchProfile::Minimal,
    FetchProfile::Windows,
    FetchProfile::IOS,
];

/// Fast: Minimal
pub(super) async fn fetch_fast_with_client(url: &str) -> Result<FetchResult, String> {
    let profile = FetchProfile::Minimal;
    let client = get_or_build_client(profile, Some(&CLIENT_CACHE))?;
    let start = Instant::now();

    match fetch_with_client(&client, url, profile).await {
        Ok(html) => Ok(FetchResult {
            html,
            profile_used: profile,
            duration_ms: start.elapsed().as_millis() as u64,
            attempts: 1,
        }),
        Err(e) => Err(e),
    }
}

/// Auto: Minimal → Windows → IOS
pub(super) async fn fetch_auto_with_client(url: &str) -> Result<FetchResult, String> {
    let start = Instant::now();
    let mut all_errors = Vec::new();

    for (idx, profile) in ADAPTIVE_PROFILES.iter().enumerate() {
        let client = get_or_build_client(*profile, Some(&CLIENT_CACHE))?;

        match fetch_with_client(&client, url, *profile).await {
            Ok(html) => {
                return Ok(FetchResult {
                    html,
                    profile_used: *profile,
                    duration_ms: start.elapsed().as_millis() as u64,
                    attempts: idx + 1,
                });
            }
            Err(e) => {
                all_errors.push(format!("{:?}: {}", profile, e));

                // Minimal delay between profiles (50-100ms)
                if idx < ADAPTIVE_PROFILES.len() - 1 {
                    tokio::time::sleep(Duration::from_millis(50 + jitter_ms(50))).await;
                }
            }
        }
    }

    Err(format!(
        "All {} profiles failed: [{}]",
        ADAPTIVE_PROFILES.len(),
        all_errors.join("; ")
    ))
}

/// Fetch with client (no referer).
async fn fetch_with_client(
    client: &Client,
    url: &str,
    profile: FetchProfile,
) -> Result<String, String> {
    fetch_with_client_and_referer(client, url, profile, None).await
}

/// Fetch with client and optional referer header.
async fn fetch_with_client_and_referer(
    client: &Client,
    url: &str,
    profile: FetchProfile,
    referer: Option<&str>,
) -> Result<String, String> {
    // Build headers for this profile
    let mut headers = headers_for_profile(profile);

    // Add referer if provided
    if let Some(ref_url) = referer {
        if let Ok(ref_value) = reqwest::header::HeaderValue::from_str(ref_url) {
            headers.insert(reqwest::header::REFERER, ref_value);
        }
    }

    // Send request
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Validate response
    validate_response(status, &body)?;

    Ok(body)
}

/// Get or build client for profile (uses cache if available).
fn get_or_build_client(
    profile: FetchProfile,
    cache: Option<&Arc<DashMap<FetchProfile, Client>>>,
) -> Result<Client, String> {
    if let Some(cache) = cache {
        if let Some(client_ref) = cache.get(&profile) {
            return Ok(client_ref.clone());
        }

        // Not in cache, build and cache it
        let client = build_client_for_profile(profile)?;
        cache.insert(profile, client.clone());
        Ok(client)
    } else {
        // No cache, just build
        build_client_for_profile(profile)
    }
}
