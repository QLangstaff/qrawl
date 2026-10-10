//! Classify Tools

mod tests;
mod utils;

use crate::types::Jsonld;

/// Classify whether a page's structured data includes a given schema.org type.
pub fn classify_has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    utils::has_schema_type(jsonld, schema_type)
}
