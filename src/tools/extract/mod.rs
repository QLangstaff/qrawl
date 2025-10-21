//! Extract Tools

mod tests;
pub mod types;
mod utils;

use crate::tools::types::{Jsonld, Metadata};
pub use types::ExtractPreviewResult;

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

/// Extract email addresses from HTML.
pub fn extract_emails(html: &str) -> Vec<String> {
    utils::extract_email_elements(html)
}

/// Extract phone numbers from HTML.
pub fn extract_phones(html: &str) -> Vec<String> {
    utils::extract_phone_elements(html)
}
