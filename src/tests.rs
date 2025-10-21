//! Tests

use crate::tools::fetch::fetch_auto;
use crate::types::Context;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chain_basic() {
        let urls = vec!["https://example.com".to_string()];
        let ctx = Context::new().with_concurrency(1);

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
        let ctx = Context::new();

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
        let ctx = Context::new().with_concurrency(1);

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
        let ctx = Context::new().with_concurrency(1);

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
        assert!(results.len() > 0);
        let (url, data) = &results[0];
        assert!(!url.is_empty());
        assert_eq!(url, data); // After map_children, both url and data are the same URL
        println!("Found {} children", results.len());
    }

    #[test]
    fn test_context_builder() {
        let ctx = Context::new()
            .with_concurrency(50)
            .with_block_domains(&["reddit.com"]);

        assert_eq!(ctx.concurrency, 50);
        assert_eq!(ctx.block_domains, Some(vec!["reddit.com".to_string()]));
    }

    #[test]
    fn test_context_as_options() {
        let ctx = Context::new()
            .with_allow_domains(&["example.com"])
            .with_block_domains(&["reddit.com"]);

        let opts = ctx.as_options();
        assert!(opts.is_some());

        let opts = opts.unwrap();
        assert_eq!(opts.allow_domains, Some(vec!["example.com".to_string()]));
        assert_eq!(opts.block_domains, Some(vec!["reddit.com".to_string()]));
    }
}
