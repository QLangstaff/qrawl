use super::types::FetchProfile;
use super::utils::check_domain_filter;
use crate::errors::QrawlError;
use reqwest::{redirect, Client};
use std::time::Duration;

const REDIRECT_LIMIT: usize = 10;
const POOL_IDLE_TIMEOUT_SEC: u64 = 90;
/// 2× headroom over the per-host cap, so a burst of completions can all be
/// reused. More is wasted: the semaphore already caps in-flight requests.
const POOL_MAX_IDLE_PER_HOST: usize = 2 * super::strategies::PER_HOST_CONCURRENCY;

/// Build a reqwest client optimized for the given profile.
///
/// No default timeout is set here: every request applies its own per-attempt timeout via `RequestBuilder::timeout(...)`, derived from the active `FetchStrategy`, without rebuilding the client.
pub(crate) fn build_client_for_profile(profile: FetchProfile) -> Result<Client, QrawlError> {
    let builder = Client::builder()
        .cookie_store(true)
        .redirect(redirect_policy(REDIRECT_LIMIT))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .pool_idle_timeout(Duration::from_secs(POOL_IDLE_TIMEOUT_SEC))
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST);

    // Minimal profile: simpler client
    let builder = match profile {
        FetchProfile::Minimal => builder
            .cookie_store(false) // No cookies for minimal
            .redirect(redirect_policy(5)), // Fewer redirects
        _ => builder,
    };

    builder
        .build()
        .map_err(|e| QrawlError::new(format!("Failed to build client: {}", e)))
}

/// Follow at most `limit` redirects, and none to a host the domain filter
/// rejects, so a blocked host is never requested.
fn redirect_policy(limit: usize) -> redirect::Policy {
    redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= limit {
            return attempt.error("too many redirects");
        }
        match check_domain_filter(attempt.url().as_str()) {
            Ok(()) => attempt.follow(),
            Err(e) => attempt.error(e),
        }
    })
}
