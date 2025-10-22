use super::profile::FetchProfile;
use reqwest::{redirect, Client};
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const REDIRECT_LIMIT: usize = 10;
const POOL_IDLE_TIMEOUT_SEC: u64 = 90;
const POOL_MAX_IDLE_PER_HOST: usize = 200; // Support high concurrency

/// Build a reqwest client optimized for the given profile.
pub(crate) fn build_client_for_profile(profile: FetchProfile) -> Result<Client, String> {
    let builder = Client::builder()
        .cookie_store(true)
        .redirect(redirect::Policy::limited(REDIRECT_LIMIT))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
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
