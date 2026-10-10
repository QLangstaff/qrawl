use serde_json::Value;

use crate::hosts::host_of;
use crate::types::{Jsonld, SocialPlatform};

/// Whether `url`'s host is a recognized social platform.
pub(super) fn is_social_url(url: &str) -> bool {
    host_of(url).is_some_and(|host| SocialPlatform::from_host(&host).is_some())
}

// ---------------------------------------------------------------------------
// Schema-type membership
//
// `has_schema_type` checks whether a page's structured data includes a given
// schema.org `@type`. Reads `@type` straight from the `Jsonld` values (never
// calls `extract`/`scrape`, so `classify` stays a leaf). The short-name
// normalization mirrors scrape's `short_type`; a second trivial copy is fine —
// hoist a shared helper only if it appears a third time.
// ---------------------------------------------------------------------------

/// Whether any entity in `jsonld` carries `schema_type`. Both sides are
/// normalized to short names, so IRIs / CURIEs / bare terms match
/// interchangeably.
pub(super) fn has_schema_type(jsonld: &Jsonld, schema_type: &str) -> bool {
    let Some(target) = short_type(schema_type) else {
        return false;
    };
    jsonld.iter().flat_map(entity_types).any(|ty| ty == target)
}

/// Short `@type` names of one entity (handles string, array, and full IRIs).
fn entity_types(value: &Value) -> Vec<String> {
    match value.get("@type") {
        Some(Value::String(s)) => short_type(s).into_iter().collect(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .filter_map(short_type)
            .collect(),
        _ => Vec::new(),
    }
}

/// Last non-empty segment of a type, splitting on `:`/`/`/`#` so a full IRI
/// (`https://schema.org/Recipe`), CURIE (`schema:Recipe`), or bare term all
/// yield `Recipe`.
fn short_type(ty: &str) -> Option<String> {
    ty.rsplit([':', '/', '#'])
        .find(|seg| !seg.is_empty())
        .map(str::to_string)
}
