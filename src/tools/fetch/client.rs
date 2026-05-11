use super::profile::FetchProfile;
use reqwest::{redirect, Client};
use std::time::Duration;

const REDIRECT_LIMIT: usize = 10;
const POOL_IDLE_TIMEOUT_SEC: u64 = 90;
/// Match `PER_HOST_CONCURRENCY` (from `strategies.rs`) with 2× headroom so a brief burst of completions can all be reused. Anything more is wasted — in-flight requests per host are already capped by the semaphore.
const POOL_MAX_IDLE_PER_HOST: usize = 16;

/// Build a reqwest client optimized for the given profile.
///
/// No default timeout is set here: every request applies its own timeout via `RequestBuilder::timeout(get_fetch_timeout())` so callers can override per `Context::with_fetch_timeout(...)` without rebuilding the client.
pub(crate) fn build_client_for_profile(profile: FetchProfile) -> Result<Client, String> {
    let builder = Client::builder()
        .cookie_store(true)
        .redirect(redirect::Policy::limited(REDIRECT_LIMIT))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .pool_idle_timeout(Duration::from_secs(POOL_IDLE_TIMEOUT_SEC))
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST);

    // Minimal profile: simpler client
    let builder = match profile {
        FetchProfile::Minimal => builder
            .cookie_store(false) // No cookies for minimal
            .redirect(redirect::Policy::limited(5)), // Fewer redirects
        _ => builder,
    };

    builder
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))
}
