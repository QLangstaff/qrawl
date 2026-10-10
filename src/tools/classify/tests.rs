#![cfg(test)]
use crate::tools::classify::*;

// ----- classify_is_social_url (host predicate) -----

// ----- classify_has_schema_type (membership predicate) -----

#[test]
fn has_schema_type_matches_any_entity_type() {
    let jsonld = vec![
        serde_json::json!({"@type": "WebSite", "url": "x"}),
        serde_json::json!({"@type": "Recipe", "name": "Soup"}),
    ];
    assert!(classify_has_schema_type(&jsonld, "Recipe"));
    assert!(classify_has_schema_type(&jsonld, "WebSite"));
    assert!(!classify_has_schema_type(&jsonld, "Product"));
}

#[test]
fn has_schema_type_normalizes_both_sides() {
    // Stored as a full IRI; queried as a bare term and a CURIE.
    let jsonld = vec![serde_json::json!({"@type": "https://schema.org/Recipe"})];
    assert!(classify_has_schema_type(&jsonld, "Recipe"));
    assert!(classify_has_schema_type(&jsonld, "schema:Recipe"));
}

#[test]
fn has_schema_type_checks_multi_type_arrays() {
    let jsonld = vec![serde_json::json!({"@type": ["LocalBusiness", "Restaurant"]})];
    assert!(classify_has_schema_type(&jsonld, "Restaurant"));
    assert!(classify_has_schema_type(&jsonld, "LocalBusiness"));
}

#[test]
fn has_schema_type_false_on_empty_or_typeless() {
    assert!(!classify_has_schema_type(&vec![], "Recipe"));
    let jsonld = vec![serde_json::json!({"name": "no type"})];
    assert!(!classify_has_schema_type(&jsonld, "Recipe"));
}
