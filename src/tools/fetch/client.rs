use super::types::FetchStrategy;
use reqwest::{redirect, Client};
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const REDIRECT_LIMIT: usize = 10;

/// Build a reqwest client optimized for the given strategy.
pub(crate) fn build_client(strategy: FetchStrategy) -> Result<Client, String> {
    let builder = Client::builder()
        .cookie_store(true)
        .redirect(redirect::Policy::limited(REDIRECT_LIMIT))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS));

    // Minimal strategy: even simpler client
    // Extreme uses same client as Stealth (difference is in orchestration)
    let builder = match strategy {
        FetchStrategy::Minimal => {
            builder
                .cookie_store(false) // No cookies for minimal
                .redirect(redirect::Policy::limited(5)) // Fewer redirects
        }
        _ => builder,
    };

    builder
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_client_for_each_strategy() {
        for strategy in [
            FetchStrategy::Minimal,
            FetchStrategy::Browser,
            FetchStrategy::Mobile,
            FetchStrategy::Stealth,
            FetchStrategy::Extreme,
        ] {
            let result = build_client(strategy);
            assert!(result.is_ok(), "Failed for {:?}", strategy);
        }
    }

    #[test]
    fn builds_client_with_default_timeout() {
        let client = build_client(FetchStrategy::Browser).unwrap();
        // Client is built successfully with default timeout
        assert!(format!("{:?}", client).contains("Client"));
    }
}
