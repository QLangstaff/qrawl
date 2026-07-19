//! Classify Tools

mod tests;
mod utils;

use crate::types::Jsonld;

/// Classify whether a URL's host is a recognized social platform (TikTok, Reddit, …).
pub fn classify_is_social_url(url: &str) -> bool {
    utils::is_social_url(url)
}

/// Classify whether a page's structured data includes a given schema.org type.
pub fn classify_has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    utils::has_schema_type(jsonld, schema_type)
}
