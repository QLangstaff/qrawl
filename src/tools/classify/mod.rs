//! Classify Tools

mod tests;

use crate::types::Jsonld;
use crate::utils::schema_types;

/// Classify whether a page's structured data includes a given schema.org type,
/// on any entity at any depth: a `Recipe` under a `WebPage`'s `mainEntity`, or
/// a `Person` as an article's `author`. Both sides compare as short names,
/// ignoring case, so IRIs / CURIEs / bare terms match interchangeably.
pub fn classify_has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    let target = schema_types(&serde_json::json!({ "@type": schema_type }));
    !target.is_empty()
        && jsonld
            .iter()
            .flat_map(declared_types)
            .any(|declared| target.iter().any(|t| t.eq_ignore_ascii_case(&declared)))
}

/// Every `@type` declared in `value` and the values under it. A `@value`
/// object is a typed literal (`xsd:date`), not an entity.
fn declared_types(value: &serde_json::Value) -> Vec<String> {
    let mut types = Vec::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(fields) if !fields.contains_key("@value") => {
                types.extend(schema_types(value));
                pending.extend(fields.values());
            }
            serde_json::Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    types
}
