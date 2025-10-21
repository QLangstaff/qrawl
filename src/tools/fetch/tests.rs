#[cfg(test)]
mod tests {
    use crate::tools::fetch::fetch_auto;

    #[tokio::test]
    async fn fetch_invalid_url_returns_error() {
        let result = fetch_auto("http://invalid-domain-that-does-not-exist.local").await;
        assert!(result.is_err());
    }

    // Note: Header tests have been moved to headers.rs module tests
    // Strategy-specific tests are in strategies.rs
    // Client building tests are in client.rs
    // Retry logic tests are in retry.rs
}
