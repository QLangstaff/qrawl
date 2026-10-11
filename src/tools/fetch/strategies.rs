use super::client::build_client_for_profile;
use super::headers::headers_for_profile;
use super::types::FetchProfile;
use super::utils::{check_status, validate_response};
use crate::errors::QrawlError;
use crate::types::FetchStrategy;
use crate::utils::host_of;
use dashmap::DashMap;
use encoding_rs::{Encoding, UTF_8};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Response};
use std::future::Future;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static CLIENT_CACHE: LazyLock<DashMap<FetchProfile, Client>> = LazyLock::new(DashMap::new);

/// The profile that last succeeded per host.
static HOST_PROFILE_CACHE: LazyLock<DashMap<String, FetchProfile>> = LazyLock::new(DashMap::new);

/// Per-host concurrency gate. One permit is acquired per URL and held through
/// the full profile cascade, so retries don't consume extra slots.
pub(super) static HOST_SEMAPHORES: LazyLock<DashMap<String, Arc<Semaphore>>> =
    LazyLock::new(DashMap::new);

/// Max simultaneous in-flight fetches per host, low enough that a burst at one
/// domain doesn't trip its rate limiter. `Context::concurrency` caps total
/// fan-out; this caps per-host fan-out independently.
pub(super) const PER_HOST_CONCURRENCY: usize = 8;

/// Hosts the two per-host maps remember before forgetting the idle ones, so a
/// long-running process that crawls the open web doesn't grow without bound.
const MAX_TRACKED_HOSTS: usize = 10_000;

/// Bytes a response body may carry. A body is held whole before it's read, so
/// one past this is refused, not cut: a cut page would read as a broken one.
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// The profiles a strategy tries, in order.
fn profiles(strategy: FetchStrategy) -> &'static [FetchProfile] {
    match strategy {
        FetchStrategy::Fast => &[FetchProfile::Minimal],
        FetchStrategy::Auto => &[
            FetchProfile::Minimal,
            FetchProfile::Windows,
            FetchProfile::Ios,
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
    if HOST_SEMAPHORES.len() >= MAX_TRACKED_HOSTS {
        // A semaphore only the map holds has no fetch waiting on it.
        HOST_SEMAPHORES.retain(|_, sema| Arc::strong_count(sema) > 1);
        HOST_PROFILE_CACHE.clear();
    }
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

/// Fetch a page as text, validated as scrapable HTML, with the URL that
/// served it.
pub(super) async fn fetch_html(
    url: &str,
    strategy: FetchStrategy,
) -> Result<(String, String), QrawlError> {
    cascade(url, strategy, None, |mut response| async move {
        let status = response.status();
        let served_by = response.url().to_string();
        let encoding = declared_encoding(&response);
        let bytes = read_body(&mut response).await?;
        let (body, _, _) = encoding.decode(&bytes);
        validate_response(status, &body)?;
        Ok((served_by, body.into_owned()))
    })
    .await
}

/// The character encoding the `Content-Type` header declares, as `reqwest`
/// reads it; UTF-8 when it declares none it knows.
fn declared_encoding(response: &Response) -> &'static Encoding {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|content_type| {
            content_type.split(';').find_map(|parameter| {
                let (name, value) = parameter.split_once('=')?;
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then_some(value.trim())
            })
        })
        .and_then(|charset| Encoding::for_label(charset.trim_matches('"').as_bytes()))
        .unwrap_or(UTF_8)
}

/// The body, up to [`MAX_RESPONSE_BYTES`]: one declared or found longer is
/// refused as it arrives, so a page can't be made to hold more than that.
async fn read_body(response: &mut Response) -> Result<Vec<u8>, QrawlError> {
    if let Some(length) = response.content_length() {
        if length > MAX_RESPONSE_BYTES as u64 {
            return Err(QrawlError::TooLarge(MAX_RESPONSE_BYTES));
        }
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| QrawlError::Request(format!("reading the response: {}", e)))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(QrawlError::TooLarge(MAX_RESPONSE_BYTES));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Fetch raw bytes (images, PDFs, JSON). Skips the HTML validation, and sends
/// `referer` for hosts with hotlink protection.
pub(super) async fn fetch_bytes(
    url: &str,
    strategy: FetchStrategy,
    referer: Option<&str>,
) -> Result<Vec<u8>, QrawlError> {
    cascade(url, strategy, referer, |mut response| async move {
        check_status(response.status())?;
        read_body(&mut response).await
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
            Ok(response) => read(response).await,
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
            // A missing page is missing under every profile, a redirect the
            // domain filter refused stays refused, a size is the server's, and
            // a loop a cookie-keeping client ran into is a loop.
            Err(e) if e.is_final() => return Err(e),
            Err(e @ QrawlError::TooManyRedirects(_)) if profile.keeps_cookies() => return Err(e),
            Err(e) => {
                log::debug!("{} via {:?} failed: {}", url, profile, e);
                errors.push(e);
            }
        }
    }

    match errors.len() {
        1 => Err(errors.remove(0)),
        _ => Err(QrawlError::AllProfilesFailed(errors)),
    }
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
        .map_err(|e| {
            // A redirect the domain filter refused surfaces as a reqwest error.
            match std::error::Error::source(&e).and_then(|s| s.downcast_ref::<QrawlError>()) {
                Some(blocked) => blocked.clone(),
                None if e.is_redirect() => QrawlError::TooManyRedirects(url.to_string()),
                None => QrawlError::Request(e.to_string()),
            }
        })
}
