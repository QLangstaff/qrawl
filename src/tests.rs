//! Tests

use crate::tools::fetch::fetch_auto;
use crate::types::{Context, CTX};
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chain_basic() {
        let urls = vec!["https://example.com".to_string()];
        let ctx = Context::auto().with_concurrency(1);

        let results = chain! {
            urls, ctx =>
            clean_urls ->
            fetch_auto
        }
        .await;

        assert_eq!(results.len(), 1);
        let (url, html) = &results[0];
        assert_eq!(url, "https://example.com");
        assert!(!html.is_empty());
    }

    #[tokio::test]
    async fn test_chain_clean_urls() {
        let urls = vec![
            "https://example.com".to_string(),
            "HTTPS://EXAMPLE.COM".to_string(),     // Duplicate
            "https://www.example.com".to_string(), // Duplicate (www)
        ];
        let ctx = Context::auto();

        let results = chain! {
            urls, ctx =>
            clean_urls
        }
        .await;

        // Should deduplicate to 1 URL
        assert_eq!(results.len(), 1);
        let (url, data) = &results[0];
        assert_eq!(url, "https://example.com");
        assert_eq!(data, "https://example.com");
    }

    #[tokio::test]
    async fn test_chain_full_chain() {
        let urls = vec!["https://example.com".to_string()];
        let ctx = Context::auto().with_concurrency(1);

        let results = chain! {
            urls, ctx =>
            clean_urls ->
            fetch_auto ->
            clean_html
        }
        .await;

        assert_eq!(results.len(), 1);
        let (url, html) = &results[0];
        assert_eq!(url, "https://example.com");
        assert!(!html.is_empty());
        // HTML should be cleaned (no scripts)
        assert!(!html.contains("<script"));
    }

    #[tokio::test]
    async fn test_chain_map_children() {
        let urls = vec![
            "https://www.delish.com/holiday-recipes/halloween/g2471/halloween-drink-recipes/"
                .to_string(),
        ];
        let ctx = Context::auto().with_concurrency(1);

        let results = chain! {
            urls, ctx =>
            clean_urls ->
            fetch_auto ->
            clean_html ->
            map_children ->
            clean_urls
        }
        .await;

        // Should find child URLs (returns Vec<(String, String)> where both are URLs)
        assert!(!results.is_empty());
        let (url, data) = &results[0];
        assert!(!url.is_empty());
        assert_eq!(url, data); // After map_children, both url and data are the same URL
        println!("Found {} children", results.len());
    }

    #[test]
    fn test_context_builder() {
        let ctx = Context::auto()
            .with_concurrency(50)
            .with_block_domains(&["reddit.com"]);

        assert_eq!(ctx.concurrency, 50);
        assert_eq!(ctx.block_domains, Some(vec!["reddit.com".to_string()]));
    }

    #[test]
    fn test_context_fast_strategy() {
        use crate::types::FetchStrategy;

        assert_eq!(Context::auto().fetch_strategy, FetchStrategy::Auto);
        assert_eq!(Context::fast().fetch_strategy, FetchStrategy::Fast);
        assert_eq!(
            Context::fast().with_concurrency(50).concurrency,
            50,
            "fast() must chain with other builders"
        );
    }

    #[tokio::test]
    async fn test_fetch_respects_block_domains() {
        let ctx = Context::auto().with_block_domains(&["example.com"]);
        let ctx = Arc::new(ctx);
        let err = CTX
            .scope(ctx, async { fetch_auto("https://example.com").await })
            .await
            .expect_err("blocked host should return Err before any HTTP work");
        assert!(
            err.contains("blocked by domain filter"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_respects_allow_domains() {
        let ctx = Context::auto().with_allow_domains(&["allowed-only.test"]);
        let ctx = Arc::new(ctx);
        let err = CTX
            .scope(ctx, async { fetch_auto("https://example.com").await })
            .await
            .expect_err("non-allowed host should return Err before any HTTP work");
        assert!(
            err.contains("blocked by domain filter"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_allow_domains_rejects_unparseable_url() {
        // With an allowlist set, an unparseable URL must fail closed — we can't
        // confirm the host matches the allowlist, so we reject.
        let ctx = Context::auto().with_allow_domains(&["example.com"]);
        let ctx = Arc::new(ctx);
        let err = CTX
            .scope(ctx, async { fetch_auto("not-a-url").await })
            .await
            .expect_err("unparseable URL must be rejected when allowlist is set");
        assert!(
            err.contains("blocked by domain filter"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_context_domain_filters() {
        let ctx = Context::auto()
            .with_allow_domains(&["example.com"])
            .with_block_domains(&["reddit.com"]);

        assert_eq!(ctx.allow_domains, Some(vec!["example.com".to_string()]));
        assert_eq!(ctx.block_domains, Some(vec!["reddit.com".to_string()]));
    }

    #[test]
    fn test_context_fetch_timeout() {
        use crate::types::DEFAULT_FETCH_TIMEOUT;
        use std::time::Duration;

        assert_eq!(Context::auto().fetch_timeout, DEFAULT_FETCH_TIMEOUT);
        assert_eq!(Context::fast().fetch_timeout, DEFAULT_FETCH_TIMEOUT);
        assert_eq!(
            Context::fast()
                .with_fetch_timeout(Duration::from_secs(5))
                .fetch_timeout,
            Duration::from_secs(5)
        );
    }
}
