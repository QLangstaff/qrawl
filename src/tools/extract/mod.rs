pub mod types;
mod utils;

pub use types::*;
use utils::*;

use crate::tools::types::{Jsonld, Metadata};
use scraper::Html;

/// Extract common body elements from HTML.
pub fn extract_body(html: &str) -> ExtractBodyResult {
    let doc = Html::parse_fragment(html);

    ExtractBodyResult {
        headings: extract_headings(&doc),
        paragraphs: extract_paragraphs(&doc),
        images: extract_images(&doc),
        links: extract_links(&doc),
    }
}

/// Extract common JSON-LD objects from parsed JSON-LD data.
///
/// ## Arguments
/// * `jsonld` - Parsed JSON-LD data from the page
/// * `schema_type` - Optional filter by schema type (e.g., "Article", "Product", "Event")
///
/// ## Returns
/// Vec of all matching JSON-LD objects. Use `.first()` if you only need one.
///
/// ## Examples
/// ```ignore
/// // Get all JSON-LD objects
/// let all = extract_jsonld(jsonld, None);
///
/// // Get only Article objects
/// let articles = extract_jsonld(jsonld, Some("Article"));
///
/// // Get first object only
/// let first = extract_jsonld(jsonld, None).first();
/// ```
pub fn extract_jsonld(jsonld: &Jsonld, schema_type: Option<&str>) -> Vec<ExtractJsonldResult> {
    let matches = match schema_type {
        Some(type_name) => find_jsonld_type(jsonld, type_name),
        None => jsonld.iter().collect(),
    };
    matches.into_iter().map(extract_jsonld_fields).collect()
}

/// Extract all available fields from metadata.
pub fn extract_metadata(metadata: &Metadata) -> ExtractMetadataResult {
    ExtractMetadataResult {
        title: find_metadata_value(metadata, &["title", "og:title", "twitter:title"]),
        description: find_metadata_value(
            metadata,
            &["description", "og:description", "twitter:description"],
        ),
        image: find_metadata_value(
            metadata,
            &["og:image", "twitter:image", "og:image:secure_url"],
        ),
        author: find_metadata_value(metadata, &["author", "article:author"]),
        published_date: find_metadata_value(
            metadata,
            &["article:published_time", "pubdate", "date"],
        ),
        modified_date: find_metadata_value(metadata, &["article:modified_time", "lastmod"]),
        keywords: find_metadata_value(metadata, &["keywords"]),
        language: find_metadata_value(metadata, &["lang", "og:locale"]),
        site_name: find_metadata_value(metadata, &["og:site_name"]),
        canonical_url: find_metadata_value(metadata, &["canonical", "og:url"]),
        page_type: find_metadata_value(metadata, &["og:type", "article:type"]),
    }
}

/// Extract Open Graph preview (title, description, image) from metadata.
pub fn extract_preview(metadata: &Metadata) -> ExtractPreviewResult {
    ExtractPreviewResult {
        title: find_metadata_value(metadata, &["title", "og:title", "twitter:title"]),
        description: find_metadata_value(
            metadata,
            &["description", "og:description", "twitter:description"],
        ),
        image: find_metadata_value(
            metadata,
            &["og:image", "twitter:image", "og:image:secure_url"],
        ),
    }
}

/// Extract recipe data from JSON-LD Recipe schema.
pub fn extract_recipes(jsonld: &Jsonld) -> Vec<ExtractRecipeResult> {
    find_jsonld_type(jsonld, "Recipe")
        .into_iter()
        .map(extract_recipe_fields)
        .collect()
}
