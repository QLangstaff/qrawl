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

        let emails = extract_emails(&html.into()).await;
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

        let phones = extract_phones(&html.into()).await;
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

        let emails = extract_emails(&html.into()).await;
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

        let phones = extract_phones(&html.into()).await;
        assert_eq!(phones.len(), 2); // Raw formats retained for downstream cleaning
        assert!(phones.contains(&"+1-555-123-4567".to_string()));
        assert!(phones.contains(&"+1 (555) 123-4567".to_string()));
    }

    // ----- extract_products -----

    fn meta(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_extract_products_jsonld_product() {
        let jsonld = vec![json!({
            "@type": "Product",
            "name": "Round Dining Table",
            "brand": {"@type": "Brand", "name": "Acme"},
            "sku": "RT-60",
            "description": "<p>Solid oak &amp; walnut.</p>",
            "image": ["/img/a.jpg", {"@type": "ImageObject", "url": "//cdn.ex.com/b.jpg"}, "/img/a.jpg"],
            "width": {"@type": "QuantitativeValue", "value": 60, "unitCode": "INH"},
            "color": "Natural",
            "additionalProperty": [{"@type": "PropertyValue", "name": "Seats", "value": "6"}],
            "aggregateRating": {"ratingValue": "4.6", "reviewCount": 132},
            "offers": {
                "@type": "Offer",
                "price": "1,299.00",
                "priceCurrency": "USD",
                "availability": "https://schema.org/InStock"
            }
        })];
        let products = extract_products(&jsonld, &vec![], "https://ex.com/p/table");
        assert_eq!(products.len(), 1);
        let p = &products[0];
        assert_eq!(p.name.as_deref(), Some("Round Dining Table"));
        assert_eq!(p.url.as_deref(), Some("https://ex.com/p/table"));
        assert_eq!(p.brand.as_deref(), Some("Acme"));
        assert_eq!(p.sku.as_deref(), Some("RT-60"));
        assert_eq!(p.description.as_deref(), Some("Solid oak & walnut."));
        assert_eq!(p.price, Some(1299.0));
        assert_eq!(p.price_max, None);
        assert_eq!(p.currency.as_deref(), Some("USD"));
        assert_eq!(p.availability.as_deref(), Some("InStock"));
        assert_eq!(
            p.images,
            vec!["https://ex.com/img/a.jpg", "https://cdn.ex.com/b.jpg"]
        );
        assert_eq!(p.rating, Some(4.6));
        assert_eq!(p.review_count, Some(132));
        assert_eq!(p.attributes["width"], "60 in");
        assert_eq!(p.attributes["color"], "Natural");
        assert_eq!(p.attributes["Seats"], "6");
    }

    #[test]
    fn test_extract_products_aggregate_offer_and_offer_array() {
        let jsonld = vec![
            json!({
                "@type": "Product",
                "name": "Wine Glass",
                "offers": {"@type": "AggregateOffer", "lowPrice": 12, "highPrice": "48.5", "priceCurrency": "USD"}
            }),
            json!({
                "@type": "Product",
                "name": "Tumbler",
                "offers": [
                    {"price": "20", "priceCurrency": "EUR", "availability": "https://schema.org/OutOfStock"},
                    {"price": "15", "priceCurrency": "EUR", "availability": "https://schema.org/InStock"}
                ]
            }),
        ];
        let products = extract_products(&jsonld, &vec![], "https://ex.com/glass");
        assert_eq!(products[0].price, Some(12.0));
        assert_eq!(products[0].price_max, Some(48.5));
        assert_eq!(products[1].price, Some(15.0));
        assert_eq!(products[1].price_max, Some(20.0));
        assert_eq!(products[1].currency.as_deref(), Some("EUR"));
        // Any purchasable offer wins over the first-listed state.
        assert_eq!(products[1].availability.as_deref(), Some("InStock"));
    }

    #[test]
    fn test_extract_products_price_specification() {
        let jsonld = vec![json!({
            "@type": "Product",
            "name": "Lamp",
            "offers": {"@type": "Offer", "priceSpecification": {"@type": "UnitPriceSpecification", "price": 89.99, "priceCurrency": "USD"}}
        })];
        let p = &extract_products(&jsonld, &vec![], "https://ex.com/lamp")[0];
        assert_eq!(p.price, Some(89.99));
        assert_eq!(p.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn test_extract_products_product_group_variants() {
        let jsonld = vec![json!({
            "@type": "ProductGroup",
            "name": "Pedestal Table",
            "hasVariant": [
                {
                    "@type": "Product",
                    "name": "Pedestal Table 48\"",
                    "sku": "PT-48",
                    "url": "/p/pedestal?variant=48",
                    "image": "/img/48.jpg",
                    "size": "48 in",
                    "offers": {"price": "899", "priceCurrency": "USD", "availability": "https://schema.org/InStock"}
                },
                {
                    "@type": "Product",
                    "name": "Pedestal Table 60\"",
                    "sku": "PT-60",
                    "size": "60 in",
                    "offers": {"price": "1199", "priceCurrency": "USD", "availability": "https://schema.org/BackOrder"}
                }
            ]
        })];
        let products = extract_products(&jsonld, &vec![], "https://ex.com/p/pedestal");
        // Variants stay inside the group, not separate results.
        assert_eq!(products.len(), 1);
        let p = &products[0];
        assert_eq!(p.price, Some(899.0));
        assert_eq!(p.price_max, Some(1199.0));
        assert_eq!(p.currency.as_deref(), Some("USD"));
        assert_eq!(p.images, vec!["https://ex.com/img/48.jpg"]);
        assert_eq!(p.variants.len(), 2);
        assert_eq!(
            p.variants[0].url.as_deref(),
            Some("https://ex.com/p/pedestal?variant=48")
        );
        assert_eq!(p.variants[1].sku.as_deref(), Some("PT-60"));
        assert_eq!(p.variants[1].attributes["size"], "60 in");
        assert_eq!(p.variants[1].availability.as_deref(), Some("BackOrder"));
    }

    #[test]
    fn test_extract_products_item_list_on_listing_page() {
        let jsonld = vec![
            json!({"@type": "BreadcrumbList", "itemListElement": []}),
            json!({
                "@type": "ItemList",
                "itemListElement": [
                    {"@type": "ListItem", "position": 1, "item": {"@type": "Product", "name": "A", "url": "https://ex.com/a", "offers": {"price": 10}}},
                    {"@type": "ListItem", "position": 2, "item": {"@type": "Product", "name": "B", "@id": "https://ex.com/b#product"}}
                ]
            }),
        ];
        let products = extract_products(&jsonld, &vec![], "https://ex.com/category");
        let urls: Vec<_> = products.iter().map(|p| p.url.as_deref().unwrap()).collect();
        assert_eq!(urls, vec!["https://ex.com/a", "https://ex.com/b"]);
    }

    #[test]
    fn test_extract_products_open_graph_fallback() {
        let metadata = meta(&[
            ("og:type", "product"),
            ("og:title", "Coupe Glass"),
            ("og:image", "https://cdn.ex.com/coupe.jpg"),
            ("product:price:amount", "24.00"),
            ("product:price:currency", "USD"),
            ("product:availability", "in stock"),
        ]);
        let products = extract_products(&vec![], &metadata, "https://ex.com/coupe");
        assert_eq!(products.len(), 1);
        let p = &products[0];
        assert_eq!(p.name.as_deref(), Some("Coupe Glass"));
        assert_eq!(p.price, Some(24.0));
        assert_eq!(p.availability.as_deref(), Some("InStock"));
        assert_eq!(p.images, vec!["https://cdn.ex.com/coupe.jpg"]);
    }

    #[test]
    fn test_extract_products_open_graph_fills_missing_images() {
        let jsonld = vec![json!({"@type": "Product", "name": "Vase", "offers": {"price": 30}})];
        let metadata = meta(&[("og:image", "https://cdn.ex.com/vase.jpg")]);
        let p = &extract_products(&jsonld, &metadata, "https://ex.com/vase")[0];
        assert_eq!(p.images, vec!["https://cdn.ex.com/vase.jpg"]);
    }

    #[test]
    fn test_extract_products_none_on_non_product_page() {
        let jsonld = vec![json!({"@type": "Article", "headline": "Hi"})];
        let metadata = meta(&[("og:type", "article"), ("og:title", "Hi")]);
        assert!(extract_products(&jsonld, &metadata, "https://ex.com/blog").is_empty());
    }

    #[test]
    fn test_extract_products_ignores_zero_price() {
        let jsonld = vec![
            json!({"@type": "Product", "name": "Call for price", "offers": {"price": "0.00"}}),
        ];
        let p = &extract_products(&jsonld, &vec![], "https://ex.com/x")[0];
        assert_eq!(p.price, None);
    }
}
