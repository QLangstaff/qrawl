#[cfg(test)]
mod tests {
    use crate::tools::extract::*;

    #[test]
    fn test_extract_emails_basic() {
        let html = r#"
            <html>
                <body>
                    <a href="mailto:john@example.com">Email John</a>
                    <p>Contact us at support@example.com</p>
                </body>
            </html>
        "#;

        let emails = extract_emails(html);
        assert!(emails.len() >= 2);
        assert!(emails.contains(&"john@example.com".to_string()));
        assert!(emails.contains(&"support@example.com".to_string()));
    }

    #[test]
    fn test_extract_phones_basic() {
        let html = r#"
            <html>
                <body>
                    <a href="tel:555-123-4567">Call us</a>
                    <p>Phone: (555) 987-6543</p>
                </body>
            </html>
        "#;

        let phones = extract_phones(html);
        assert!(phones.len() >= 2);
    }

    #[test]
    fn test_extract_metadata_prefers_specific_fields() {
        let metadata = vec![
            ("title".to_string(), "Generic Title".to_string()),
            ("og:title".to_string(), "OG Title".to_string()),
            (
                "twitter:title".to_string(),
                "Twitter Title".to_string(),
            ),
            (
                "description".to_string(),
                "Generic Description".to_string(),
            ),
            (
                "og:description".to_string(),
                "OG Description".to_string(),
            ),
            (
                "og:image".to_string(),
                "https://example.com/image.png".to_string(),
            ),
            (
                "author".to_string(),
                "Jane Smith".to_string(),
            ),
            (
                "article:published_time".to_string(),
                "2024-01-01".to_string(),
            ),
        ];

        let result = extract_metadata(&metadata);
        assert_eq!(result.title, Some("OG Title".to_string()));
        assert_eq!(
            result.description,
            Some("OG Description".to_string())
        );
        assert_eq!(
            result.image,
            Some("https://example.com/image.png".to_string())
        );
        assert_eq!(result.author, Some("Jane Smith".to_string()));
        assert_eq!(
            result.published_date,
            Some("2024-01-01".to_string())
        );
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
        assert_eq!(
            preview.description,
            Some("Twitter Description".to_string())
        );
        assert_eq!(
            preview.image,
            Some("https://secure.example.com/image.jpg".to_string())
        );
    }

    #[test]
    fn test_extract_schema_types_collects_unique_values() {
        let jsonld = vec![
            serde_json::json!({
                "@type": ["Recipe", "Article"]
            }),
            serde_json::json!({
                "@type": "Article"
            }),
            serde_json::json!({
                "@type": ["HowTo", "Recipe"]
            }),
        ];

        let mut types = extract_schema_types(&jsonld);
        types.sort();
        assert_eq!(types, vec!["Article", "HowTo", "Recipe"]);
    }

    #[test]
    fn test_extract_emails_deduplicates_results() {
        let html = r#"
            <html>
                <body>
                    <a href="mailto:info@example.com">Email</a>
                    <p>Contact: info@example.com</p>
                </body>
            </html>
        "#;

        let emails = extract_emails(html);
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0], "info@example.com");
    }

    #[test]
    fn test_extract_phones_normalizes_formats() {
        let html = r#"
            <html>
                <body>
                    <a href="tel:+1-555-123-4567">Call</a>
                    <span>+1 (555) 123-4567</span>
                </body>
            </html>
        "#;

        let mut phones = extract_phones(html);
        phones.sort();
        phones.dedup();
        assert_eq!(phones, vec!["+15551234567"]);
    }
}
