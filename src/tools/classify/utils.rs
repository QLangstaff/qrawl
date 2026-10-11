use crate::types::Jsonld;
use crate::utils::schema_types;

/// Whether any entity in `jsonld`, at any depth, carries `schema_type`: a
/// `Recipe` under a `WebPage`'s `mainEntity` counts. Both sides compare as
/// short names, ignoring case, so IRIs / CURIEs / bare terms match
/// interchangeably.
pub(super) fn has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    let target = schema_types(&serde_json::json!({ "@type": schema_type }));
    !target.is_empty()
        && jsonld
            .iter()
            .flat_map(declared_types)
            .any(|declared| target.iter().any(|t| t.eq_ignore_ascii_case(&declared)))
}

/// Every `@type` declared in `value` and the values under it.
fn declared_types(value: &serde_json::Value) -> Vec<String> {
    let mut types = Vec::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(fields) => {
                types.extend(schema_types(value));
                pending.extend(fields.values());
            }
            serde_json::Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    types
}
