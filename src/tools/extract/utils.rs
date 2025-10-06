use scraper::{Html, Selector};
use serde_json::Value;
use std::sync::LazyLock;

// Lazy static selectors - compiled once for the entire program
static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid selector"));
static IMG_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("img[src], img[data-src], img[data-hi-res-src]").expect("valid selector")
});
static HEADING_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1, h2, h3, h4, h5, h6").expect("valid selector"));
static PARAGRAPH_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("p").expect("valid selector"));

/// Generic: Extract HTML elements.
fn extract_html_elements<F>(doc: &Html, selector: &Selector, mut extractor: F) -> Vec<String>
where
    F: FnMut(&scraper::ElementRef) -> Option<String>,
{
    doc.select(selector)
        .filter_map(|el| extractor(&el))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract all headings from HTML document.
pub fn extract_headings(doc: &Html) -> Vec<String> {
    extract_html_elements(doc, &HEADING_SELECTOR, |el| {
        Some(el.text().collect::<String>().trim().to_string())
    })
}

/// Extract all paragraphs from HTML document.
pub fn extract_paragraphs(doc: &Html) -> Vec<String> {
    extract_html_elements(doc, &PARAGRAPH_SELECTOR, |el| {
        Some(el.text().collect::<String>().trim().to_string())
    })
}

/// Extract all image src attributes from parsed HTML document.
pub fn extract_images(doc: &Html) -> Vec<String> {
    extract_html_elements(doc, &IMG_SELECTOR, |el| {
        el.value()
            .attr("src")
            .or_else(|| el.value().attr("data-src"))
            .or_else(|| el.value().attr("data-hi-res-src"))
            .map(String::from)
    })
}

/// Extract all href attributes from parsed HTML document.
pub fn extract_links(doc: &Html) -> Vec<String> {
    extract_html_elements(doc, &LINK_SELECTOR, |el| {
        el.value().attr("href").map(String::from)
    })
}

/// Find the first non-empty value for any of the given keys in metadata pairs.
pub fn find_metadata_value(pairs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        for (k, v) in pairs {
            if k.eq_ignore_ascii_case(key) {
                let cleaned = v.trim().to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

/// Find all JSON-LD objects matching the specified schema type.
pub fn find_jsonld_type<'a>(jsonld: &'a [Value], schema_type: &str) -> Vec<&'a Value> {
    jsonld
        .iter()
        .filter(|obj| matches_schema_type(obj, schema_type))
        .collect()
}

fn matches_schema_type(obj: &Value, target_type: &str) -> bool {
    match obj.get("@type") {
        Some(Value::String(s)) => s.eq_ignore_ascii_case(target_type),
        Some(Value::Array(arr)) => arr.iter().any(|v| {
            v.as_str()
                .map(|s| s.eq_ignore_ascii_case(target_type))
                .unwrap_or(false)
        }),
        _ => false,
    }
}

/// Helper to extract string from Value.
#[inline]
pub fn as_str_string(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

/// Generic JSON value extractor for single values.
/// Handles common patterns: String, Object with url/name field, Array (first element).
pub fn extract_json_single<F>(
    value: Option<&Value>,
    object_field: &str,
    transformer: F,
) -> Option<String>
where
    F: Fn(&Value) -> Option<String>,
{
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Array(arr) => arr.first().and_then(|v| transformer(v)),
        Value::Object(obj) => obj
            .get(object_field)
            .and_then(|v| v.as_str())
            .map(String::from),
        v => transformer(v),
    }
}

/// Generic JSON value extractor for arrays.
/// Handles both single values and arrays, with optional transformation of object elements.
pub fn extract_json_array<F>(value: Option<&Value>, mut object_transformer: F) -> Vec<String>
where
    F: FnMut(&Value) -> Option<String>,
{
    match value {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Object(_) => object_transformer(v),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Extract all schema types from JSON-LD @type field.
/// Handles both single string and array of strings.
pub fn extract_schema_types(obj: &Value) -> Vec<String> {
    extract_json_array(obj.get("@type"), |_| None)
}

/// Extract fields from a JSON-LD object (works for any schema type).
/// Used by extract_jsonld.
pub fn extract_jsonld_fields(obj: &Value) -> super::ExtractJsonldResult {
    super::ExtractJsonldResult {
        schema_types: extract_schema_types(obj),
        name: obj
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                obj.get("headline")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }),
        description: obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
        image: extract_json_single(obj.get("image"), "url", as_str_string),
        url: obj
            .get("url")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| obj.get("@id").and_then(|v| v.as_str()).map(String::from)),
        author: extract_json_single(obj.get("author"), "name", as_str_string),
        date_published: obj
            .get("datePublished")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                obj.get("uploadDate")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }),
        date_modified: obj
            .get("dateModified")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// Extract fields from a Recipe JSON-LD object.
/// Used by extract_recipes.
pub fn extract_recipe_fields(recipe: &Value) -> super::ExtractRecipeResult {
    super::ExtractRecipeResult {
        name: recipe
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from),
        description: recipe
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
        image: extract_json_single(recipe.get("image"), "url", as_str_string),
        ingredients: extract_json_array(recipe.get("recipeIngredient"), |_| None),
        instructions: extract_json_array(recipe.get("recipeInstructions"), |obj| {
            obj.as_object()
                .and_then(|o| o.get("text"))
                .and_then(|t| t.as_str())
                .map(String::from)
        }),
        prep_time: recipe
            .get("prepTime")
            .and_then(|v| v.as_str())
            .map(String::from),
        cook_time: recipe
            .get("cookTime")
            .and_then(|v| v.as_str())
            .map(String::from),
        total_time: recipe
            .get("totalTime")
            .and_then(|v| v.as_str())
            .map(String::from),
        servings: recipe.get("recipeYield").and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }),
    }
}
