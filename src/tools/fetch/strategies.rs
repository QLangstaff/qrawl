use super::client::build_client_for_profile;
use super::headers::headers_for_profile;
use super::types::FetchProfile;
use super::utils::{check_domain_filter, check_status, validate_response};
use crate::errors::QrawlError;
use crate::types::FetchStrategy;
use crate::utils::host_of;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use reqwest::{Client, Response};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static CLIENT_CACHE: Lazy<DashMap<FetchProfile, Client>> = Lazy::new(DashMap::new);

/// The profile that last succeeded per host.
static HOST_PROFILE_CACHE: Lazy<DashMap<String, FetchProfile>> = Lazy::new(DashMap::new);

/// Per-host concurrency gate. One permit is acquired per URL and held through
/// the full profile cascade, so retries don't consume extra slots.
pub(super) static HOST_SEMAPHORES: Lazy<DashMap<String, Arc<Semaphore>>> = Lazy::new(DashMap::new);

/// Max simultaneous in-flight fetches per host, low enough that a burst at one
/// domain doesn't trip its rate limiter. `Context::concurrency` caps total
/// fan-out; this caps per-host fan-out independently.
pub(super) const PER_HOST_CONCURRENCY: usize = 8;

/// The profiles a strategy tries, in order.
fn profiles(strategy: FetchStrategy) -> &'static [FetchProfile] {
    match strategy {
        FetchStrategy::Fast => &[FetchProfile::Minimal],
        FetchStrategy::Auto => &[
            FetchProfile::Minimal,
            FetchProfile::Windows,
            FetchProfile::IOS,
        ],
    }
}

/// Per-attempt timeout: `Fast` fails fast on its single attempt, `Auto` is
/// patient across its cascade.
fn attempt_timeout(strategy: FetchStrategy) -> Duration {
    match strategy {
        FetchStrategy::Fast => Duration::from_secs(8),
        FetchStrategy::Auto => Duration::from_secs(30),
    }
}

/// Acquire a permit for this host. `None` when the URL has no host, which skips
/// the cap.
pub(super) async fn acquire_host_permit(host: Option<&str>) -> Option<OwnedSemaphorePermit> {
    let host = host?;
    let sema = HOST_SEMAPHORES
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(PER_HOST_CONCURRENCY)))
        .clone();
    sema.acquire_owned().await.ok()
}

fn client_for_profile(profile: FetchProfile) -> Result<Client, QrawlError> {
    if let Some(client) = CLIENT_CACHE.get(&profile) {
        return Ok(client.clone());
    }
    let client = build_client_for_profile(profile)?;
    CLIENT_CACHE.insert(profile, client.clone());
    Ok(client)
}

/// Fetch a page as text, validated as scrapable HTML.
pub(super) async fn fetch_html(url: &str, strategy: FetchStrategy) -> Result<String, QrawlError> {
    cascade(url, strategy, None, |response| async move {
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| QrawlError::new(format!("Failed to read response: {}", e)))?;
        validate_response(status, &body)?;
        Ok(body)
    })
    .await
}

/// Fetch raw bytes (images, PDFs, JSON). Skips the HTML validation, and sends
/// `referer` for hosts with hotlink protection.
pub(super) async fn fetch_bytes(
    url: &str,
    strategy: FetchStrategy,
    referer: Option<&str>,
) -> Result<Vec<u8>, QrawlError> {
    cascade(url, strategy, referer, |response| async move {
        check_status(response.status())?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| QrawlError::new(format!("Failed to read response bytes: {}", e)))?;
        Ok(bytes.to_vec())
    })
    .await
}

/// The strategy's profiles with the host's last successful one moved first, so
/// a host that needs a browser profile skips the attempts that fail, and a host
/// that stops accepting it still gets every other profile.
pub(super) fn attempt_order(
    profiles: &[FetchProfile],
    cached: Option<FetchProfile>,
) -> Vec<FetchProfile> {
    let mut order = profiles.to_vec();
    if let Some(index) = cached.and_then(|cached| order.iter().position(|p| *p == cached)) {
        order[..=index].rotate_right(1);
    }
    order
}

/// Send `url` with each of the strategy's profiles, in [`attempt_order`],
/// until `read` accepts a response.
async fn cascade<T, Read, Fut>(
    url: &str,
    strategy: FetchStrategy,
    referer: Option<&str>,
    read: Read,
) -> Result<T, QrawlError>
where
    Read: Fn(Response) -> Fut,
    Fut: Future<Output = Result<T, QrawlError>>,
{
    let start = Instant::now();
    let host = host_of(url);
    let _permit = acquire_host_permit(host.as_deref()).await;

    let cached = host
        .as_ref()
        .and_then(|host| HOST_PROFILE_CACHE.get(host).map(|profile| *profile));
    let profiles = attempt_order(profiles(strategy), cached);

    let mut errors = Vec::new();
    for (attempt, profile) in profiles.iter().enumerate() {
        let result = match send(url, *profile, referer, attempt_timeout(strategy)).await {
            // A redirect can land on a host the request's own URL didn't name.
            Ok(response) => match check_domain_filter(response.url().as_str()) {
                Ok(()) => read(response).await,
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        match result {
            Ok(value) => {
                if let Some(host) = host {
                    HOST_PROFILE_CACHE.insert(host, *profile);
                }
                log::info!(
                    "fetched {} via {:?} in {}ms ({} attempts)",
                    url,
                    profile,
                    start.elapsed().as_millis(),
                    attempt + 1
                );
                return Ok(value);
            }
            Err(e) => errors.push(format!("{:?}: {}", profile, e)),
        }
    }

    Err(QrawlError::new(format!(
        "All {} profiles failed: [{}]",
        errors.len(),
        errors.join("; ")
    )))
}

async fn send(
    url: &str,
    profile: FetchProfile,
    referer: Option<&str>,
    timeout: Duration,
) -> Result<Response, QrawlError> {
    let mut headers = headers_for_profile(profile);
    if let Some(referer) = referer {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(referer) {
            headers.insert(reqwest::header::REFERER, value);
        }
    }
    client_for_profile(profile)?
        .get(url)
        .headers(headers)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| QrawlError::new(format!("HTTP request failed: {}", e)))
}
