use crate::types::Jsonld;
use crate::utils::schema_types;

/// Whether any entity in `jsonld` carries `schema_type`. Both sides compare
/// as short names, ignoring case, so IRIs / CURIEs / bare terms match
/// interchangeably.
pub(super) fn has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    let target = schema_types(&serde_json::json!({ "@type": schema_type }));
    !target.is_empty()
        && jsonld
            .iter()
            .flat_map(schema_types)
            .any(|declared| target.iter().any(|t| t.eq_ignore_ascii_case(&declared)))
}
