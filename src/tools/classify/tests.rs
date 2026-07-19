#![cfg(test)]
use crate::tools::classify::*;

// ----- classify_is_social_url (host predicate) -----

#[test]
fn is_social_url_true_for_known_platforms() {
    assert!(classify_is_social_url("https://tiktok.com/@user/video/123"));
    assert!(classify_is_social_url("https://reddit.com/r/rust"));
    assert!(classify_is_social_url("https://youtu.be/dQw4w9WgXcQ"));
    assert!(classify_is_social_url(
        "https://www.pinterest.co.uk/pin/999"
    ));
    // Recognized platform, opaque path — still a social URL.
    assert!(classify_is_social_url("https://vm.tiktok.com/ZMabcdef"));
}

#[test]
fn is_social_url_false_for_web_and_garbage() {
    assert!(!classify_is_social_url("https://example.com/foo/bar"));
    assert!(!classify_is_social_url(
        "https://www.allrecipes.com/recipe/10813"
    ));
    assert!(!classify_is_social_url("http://")); // a URL, but has no host
    assert!(!classify_is_social_url("not a url"));
    assert!(!classify_is_social_url("mailto:hi@example.com"));
}

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
