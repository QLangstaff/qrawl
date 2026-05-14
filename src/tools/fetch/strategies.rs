use super::client::build_client_for_profile;
use super::headers::headers_for_profile;
use super::profile::FetchProfile;
use super::types::*;
use super::utils::*;
use crate::types::get_fetch_timeout;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use reqwest::Client;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static CLIENT_CACHE: Lazy<Arc<DashMap<FetchProfile, Client>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// Last-successful fetch profile per host. Public for instrumentation only —
/// mutating it from outside this module is unsupported and may break the
/// adaptive cascade.
#[doc(hidden)]
pub static HOST_PROFILE_CACHE: Lazy<Arc<DashMap<String, FetchProfile>>> =
    Lazy::new(|| Arc::new(DashMap::new()));

/// Per-host concurrency gate. Limits how many in-flight fetches may target a
/// single host simultaneously. One permit is acquired per URL and held through
/// the full profile cascade, so retries don't consume extra slots.
static HOST_SEMAPHORES: Lazy<DashMap<String, Arc<Semaphore>>> = Lazy::new(DashMap::new);

/// Max simultaneous in-flight fetches per host. Set low enough that a
/// popular-domain burst from a search batch doesn't trip the host's rate
/// limiter. Global concurrency (`Context::concurrency`) caps total fan-out;
/// this caps per-host fan-out independently.
pub const PER_HOST_CONCURRENCY: usize = 8;

/// Counts outgoing HTTP attempts. Used by perf tests to verify the per-pipeline
/// fetch cache prevents duplicate network calls. Not part of the supported API
/// — concurrent readers may see interleaved counts from unrelated work.
#[doc(hidden)]
pub static HTTP_ATTEMPTS: AtomicUsize = AtomicUsize::new(0);

const ADAPTIVE_PROFILES: [FetchProfile; 3] = [
    FetchProfile::Minimal,
    FetchProfile::Windows,
    FetchProfile::IOS,
];

fn host_from_url(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

/// Acquire a permit for this host. Returns `None` when the URL has no host
/// (opaque, file://, etc.) — in that case we skip the cap entirely.
async fn acquire_host_permit(host: Option<&str>) -> Option<OwnedSemaphorePermit> {
    let host = host?;
    let sema = HOST_SEMAPHORES
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(PER_HOST_CONCURRENCY)))
        .clone();
    sema.acquire_owned()
        .await
        .ok()
}

/// Fast: Minimal
pub(super) async fn fetch_fast_with_client(url: &str) -> Result<FetchResult, String> {
    let host = host_from_url(url);
    let _permit = acquire_host_permit(host.as_deref()).await;

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

    let host = host_from_url(url);
    let _permit = acquire_host_permit(host.as_deref()).await;
    let starting_idx = host
        .as_ref()
        .and_then(|h| HOST_PROFILE_CACHE.get(h).map(|v| *v))
        .and_then(|cached| ADAPTIVE_PROFILES.iter().position(|p| *p == cached))
        .unwrap_or(0);

    for (offset, profile) in ADAPTIVE_PROFILES[starting_idx..].iter().enumerate() {
        let client = get_or_build_client(*profile, Some(&CLIENT_CACHE))?;

        match fetch_with_client(&client, url, *profile).await {
            Ok(html) => {
                if let Some(ref h) = host {
                    HOST_PROFILE_CACHE.insert(h.clone(), *profile);
                }
                return Ok(FetchResult {
                    html,
                    profile_used: *profile,
                    duration_ms: start.elapsed().as_millis() as u64,
                    attempts: offset + 1,
                });
            }
            Err(e) => {
                all_errors.push(format!("{:?}: {}", profile, e));
            }
        }
    }

    Err(format!(
        "All {} profiles failed: [{}]",
        ADAPTIVE_PROFILES.len() - starting_idx,
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

    HTTP_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

    // Send request with per-request timeout (reads from Context::fetch_timeout
    // if in scope, else DEFAULT_FETCH_TIMEOUT).
    let response = client
        .get(url)
        .headers(headers)
        .timeout(get_fetch_timeout())
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

/// Fetch raw bytes with client + profile + optional referer.
///
/// Same wire protocol as the HTML fetcher (profile headers, HTTP_ATTEMPTS counter,
/// optional Referer for hotlink protection) but skips the HTML-specific body
/// validation and returns `Vec<u8>` for binary content.
async fn fetch_bytes_with_client_and_referer(
    client: &Client,
    url: &str,
    profile: FetchProfile,
    referer: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut headers = headers_for_profile(profile);

    if let Some(ref_url) = referer {
        if let Ok(ref_value) = reqwest::header::HeaderValue::from_str(ref_url) {
            headers.insert(reqwest::header::REFERER, ref_value);
        }
    }

    HTTP_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

    let response = client
        .get(url)
        .headers(headers)
        .timeout(get_fetch_timeout())
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("status {}", status.as_u16()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response bytes: {}", e))?;

    Ok(bytes.to_vec())
}

/// Fast bytes: single Minimal-profile attempt.
pub(super) async fn fetch_bytes_fast_with_client(
    url: &str,
    referer: Option<&str>,
) -> Result<Vec<u8>, String> {
    let host = host_from_url(url);
    let _permit = acquire_host_permit(host.as_deref()).await;

    let profile = FetchProfile::Minimal;
    let client = get_or_build_client(profile, Some(&CLIENT_CACHE))?;
    fetch_bytes_with_client_and_referer(&client, url, profile, referer).await
}

/// Auto bytes: same Minimal → Windows → iOS cascade as HTML fetch, with host
/// profile cache and no inter-attempt sleep.
pub(super) async fn fetch_bytes_auto_with_client(
    url: &str,
    referer: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut all_errors = Vec::new();

    let host = host_from_url(url);
    let _permit = acquire_host_permit(host.as_deref()).await;
    let starting_idx = host
        .as_ref()
        .and_then(|h| HOST_PROFILE_CACHE.get(h).map(|v| *v))
        .and_then(|cached| ADAPTIVE_PROFILES.iter().position(|p| *p == cached))
        .unwrap_or(0);

    for profile in ADAPTIVE_PROFILES[starting_idx..].iter() {
        let client = get_or_build_client(*profile, Some(&CLIENT_CACHE))?;

        match fetch_bytes_with_client_and_referer(&client, url, *profile, referer).await {
            Ok(bytes) => {
                if let Some(ref h) = host {
                    HOST_PROFILE_CACHE.insert(h.clone(), *profile);
                }
                return Ok(bytes);
            }
            Err(e) => all_errors.push(format!("{:?}: {}", profile, e)),
        }
    }

    Err(format!(
        "All {} profiles failed: [{}]",
        ADAPTIVE_PROFILES.len() - starting_idx,
        all_errors.join("; ")
    ))
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

#[cfg(test)]
mod host_cap_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn host_cap_serializes_excess_callers() {
        // Reset the semaphore for this host to avoid pollution from other tests.
        HOST_SEMAPHORES.remove("cap-test.invalid");

        // Hold-time chosen so that an unbounded run would finish well under
        // `PER_HOST_CONCURRENCY * hold`, while a capped run cannot.
        let hold = Duration::from_millis(40);
        let total = PER_HOST_CONCURRENCY + 4;

        let start = Instant::now();
        let mut handles = Vec::with_capacity(total);
        for _ in 0..total {
            handles.push(tokio::spawn(async move {
                let permit = acquire_host_permit(Some("cap-test.invalid"))
                    .await
                    .expect("permit must issue");
                tokio::time::sleep(hold).await;
                drop(permit);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let elapsed = start.elapsed();

        // With a cap of N, (N + 4) tasks each holding for `hold` must take at
        // least 2 * hold (two "rounds"). Without the cap it'd be ~1 * hold.
        assert!(
            elapsed >= hold * 2,
            "per-host cap didn't serialize: elapsed={:?}, expected >= {:?}",
            elapsed,
            hold * 2
        );
    }

    #[tokio::test]
    async fn host_cap_skipped_when_url_has_no_host() {
        let permit = acquire_host_permit(None).await;
        assert!(permit.is_none(), "no-host URLs should bypass the cap");
    }
}
