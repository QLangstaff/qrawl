#[cfg(test)]
mod tests {
    use crate::tools::extract::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_extract_emails_basic() {
        let html = r#"
            <html>
                <body>
                    <a href="mailto:john@example.com">Email John</a>
                    <p>Contact us at support@example.com</p>
                </body>
            </html>
        "#;

        let emails = extract_emails(html).await;
        assert!(emails.len() >= 2);
        assert!(emails.contains(&"john@example.com".to_string()));
        assert!(emails.contains(&"support@example.com".to_string()));
    }

    #[tokio::test]
    async fn test_extract_phones_basic() {
        let html = r#"
            <html>
                <body>
                    <a href="tel:555-123-4567">Call us</a>
                    <p>Phone: (555) 987-6543</p>
                </body>
            </html>
        "#;

        let phones = extract_phones(html).await;
        assert!(phones.len() >= 2);
    }

    #[test]
    fn test_extract_og_preview_uses_metadata_fallbacks() {
        let metadata = vec![
            ("og:title".to_string(), "OG Title".to_string()),
            (
                "twitter:description".to_string(),
                "Twitter Description".to_string(),
            ),
            (
                "og:image:secure_url".to_string(),
                "https://secure.example.com/image.jpg".to_string(),
            ),
        ];

        let preview = extract_og_preview(&metadata);
        assert_eq!(preview.title, Some("OG Title".to_string()));
        assert_eq!(preview.description, Some("Twitter Description".to_string()));
        assert_eq!(
            preview.image,
            Some("https://secure.example.com/image.jpg".to_string())
        );
    }

    #[test]
    fn test_extract_schema_types_collects_unique_values() {
        let jsonld = vec![
            json!({
                "@type": ["Recipe", "Article"]
            }),
            json!({
                "@type": "Article"
            }),
            json!({
                "@type": ["HowTo", "Recipe"]
            }),
        ];

        let mut types = extract_schema_types(&jsonld);
        types.sort();
        assert_eq!(types, vec!["Article", "HowTo", "Recipe"]);
    }

    #[tokio::test]
    async fn test_extract_emails_collects_raw_results() {
        let html = r#"
            <html>
                <body>
                    <a href="mailto:info@example.com">Email</a>
                    <p>Contact: info@example.com</p>
                </body>
            </html>
        "#;

        let emails = extract_emails(html).await;
        assert_eq!(
            emails,
            vec!["info@example.com", "info@example.com"]
                .into_iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_extract_phones_preserves_formats() {
        let html = r#"
            <html>
                <body>
                    <a href="tel:+1-555-123-4567">Call</a>
                    <span>+1 (555) 123-4567</span>
                </body>
            </html>
        "#;

        let phones = extract_phones(html).await;
        assert_eq!(phones.len(), 2); // Raw formats retained for downstream cleaning
        assert!(phones.contains(&"+1-555-123-4567".to_string()));
        assert!(phones.contains(&"+1 (555) 123-4567".to_string()));
    }
}
