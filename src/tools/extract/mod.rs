//! Extract Tools

mod products;
mod tests;
pub mod types;
mod utils;

use crate::types::{Html, Jsonld, Metadata};
pub use types::{ExtractPreviewResult, ExtractProductResult, ExtractProductVariant};

/// Extract schema.org `@type` values from JSON-LD.
pub fn extract_schema_types(jsonld: &Jsonld) -> Vec<String> {
    let mut types = Vec::new();

    for value in jsonld {
        if let Some(type_value) = value.get("@type") {
            match type_value {
                serde_json::Value::String(s) => utils::push_unique(&mut types, s.to_string()),
                serde_json::Value::Array(arr) => {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            utils::push_unique(&mut types, s.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    types
}

/// Extract Open Graph preview (title, description, image) from metadata.
pub fn extract_og_preview(metadata: &Metadata) -> ExtractPreviewResult {
    ExtractPreviewResult {
        title: utils::find_metadata_value(metadata, &["title", "og:title", "twitter:title"]),
        description: utils::find_metadata_value(
            metadata,
            &["description", "og:description", "twitter:description"],
        ),
        image: utils::find_metadata_value(
            metadata,
            &["og:image", "twitter:image", "og:image:secure_url"],
        ),
    }
}

/// Extract products from a page's schema.org data (`Product` / `ProductGroup`,
/// anywhere in the graph — including `ItemList`s on category pages), falling
/// back to Open Graph `product:*` tags when there is none. `url` is the page
/// URL, used to resolve relative product / image links.
///
/// Prices are normalized to numbers (lowest offer, plus `price_max` for a
/// range); availability to its schema.org short name; dimensions and other
/// descriptive properties land in `attributes` (e.g. `"width": "60 in"`).
pub fn extract_products(
    jsonld: &Jsonld,
    metadata: &Metadata,
    url: &str,
) -> Vec<ExtractProductResult> {
    products::extract_products(jsonld, metadata, url)
}

/// Extract email addresses from HTML.
pub async fn extract_emails(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::extract_email_elements(&html))
        .await
        .expect("extract_emails: spawn_blocking failed")
}

/// Extract phone numbers from HTML.
pub async fn extract_phones(html: &Html) -> Vec<String> {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::extract_phone_elements(&html))
        .await
        .expect("extract_phones: spawn_blocking failed")
}
