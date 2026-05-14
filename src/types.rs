//! Shared Types

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Fetch strategy for pipeline `fetch_*` steps.
///
/// - `Auto` (default): Minimal → Windows → iOS fetch strategy cascade.
/// - `Fast`: Minimal fetch strategy only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStrategy {
    Auto,
    Fast,
}

/// Default fetch timeout.
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Default concurrency.
pub const DEFAULT_CONCURRENCY: usize = 1000;

/// Context to chain tools
#[derive(Debug, Clone)]
pub struct Context {
    pub fetch_strategy: FetchStrategy,
    pub fetch_timeout: Duration,
    pub concurrency: usize,
    pub allow_domains: Option<Vec<String>>,
    pub block_domains: Option<Vec<String>>,
}

impl Context {
    /// Context with the Minimal→Windows→iOS fetch strategy cascade.
    pub fn auto() -> Self {
        Self {
            fetch_strategy: FetchStrategy::Auto,
            fetch_timeout: DEFAULT_FETCH_TIMEOUT,
            concurrency: DEFAULT_CONCURRENCY,
            allow_domains: None,
            block_domains: None,
        }
    }

    /// Context with the Minimal fetch strategy only.
    pub fn fast() -> Self {
        Self {
            fetch_strategy: FetchStrategy::Fast,
            ..Self::auto()
        }
    }

    /// Override the per-request fetch timeout. Applied to each profile attempt
    /// via reqwest's `RequestBuilder::timeout`. Bulk workloads typically set
    /// this aggressively (e.g., 5s) to cap tail latency; single-URL workflows
    /// leave it at the default to give slow sites a chance.
    pub fn with_fetch_timeout(mut self, timeout: Duration) -> Self {
        self.fetch_timeout = timeout;
        self
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn with_allow_domains(mut self, domains: &[&str]) -> Self {
        self.allow_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn with_block_domains(mut self, domains: &[&str]) -> Self {
        self.block_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }
}

tokio::task_local! {
    pub static CTX: Arc<Context>;
    /// Per-pipeline fetch cache (canonical URL -> HTML). Populated by fetch functions
    /// on success and consulted on subsequent calls within the same `chain!` invocation.
    pub static FETCH_CACHE: Arc<DashMap<String, String>>;
}

pub fn fetch_cache_new() -> Arc<DashMap<String, String>> {
    Arc::new(DashMap::new())
}

// Keys are canonicalized so callers that happen to fetch the same logical URL
// via two different surface forms (e.g. `https://Example.com/` vs
// `https://example.com`) hit the same cache entry. `canonicalize_url` is
// idempotent, so this is a no-op for callers that already canonicalize.
pub fn fetch_cache_get(url: &str) -> Option<String> {
    let key = crate::tools::clean::canonicalize_url(url);
    FETCH_CACHE
        .try_with(|cache| cache.get(&key).map(|v| v.clone()))
        .ok()
        .flatten()
}

pub fn fetch_cache_put(url: &str, html: &str) {
    let key = crate::tools::clean::canonicalize_url(url);
    let _ = FETCH_CACHE.try_with(|cache| {
        cache.insert(key, html.to_string());
    });
}

pub fn get_fetch_strategy() -> FetchStrategy {
    CTX.try_with(|ctx| ctx.fetch_strategy)
        .ok()
        .unwrap_or(FetchStrategy::Auto)
}

pub fn get_fetch_timeout() -> Duration {
    CTX.try_with(|ctx| ctx.fetch_timeout)
        .ok()
        .unwrap_or(DEFAULT_FETCH_TIMEOUT)
}

/// JSON-LD array of schema.org objects.
pub type Jsonld = Vec<Value>;

/// Metadata key-value pairs.
pub type Metadata = Vec<(String, String)>;
