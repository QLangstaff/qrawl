//! Classify Tools

mod tests;
mod utils;

use crate::types::Jsonld;

/// Classify whether a page's structured data includes a given schema.org type,
/// on any entity at any depth: a `Recipe` under a `WebPage`'s `mainEntity`, or
/// a `Person` as an article's `author`.
pub fn classify_has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    utils::has_schema_type(jsonld, schema_type)
}
