use serde::{Deserialize, Serialize};

/// Extract preview result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractPreviewResult {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// Extract product result — one product (or product group) from a page's
/// schema.org data, with an Open Graph fallback. Empty fields are omitted from
/// the JSON so results stay compact for LLM consumption.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProductResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Absolute product URL (falls back to the page URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Plain-text description (HTML tags stripped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Lowest price across offers / variants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    /// Highest price, only when it differs from `price` (a price range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_max: Option<f64>,
    /// ISO 4217 currency code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// schema.org availability short name (`InStock`, `OutOfStock`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<String>,
    /// Absolute image URLs, deduplicated, in page order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_count: Option<u64>,
    /// Descriptive attributes: `color`, `material`, `size`, `width`, `height`,
    /// `depth`, `weight`, plus any `additionalProperty` name/value pairs.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attributes: std::collections::BTreeMap<String, String>,
    /// Variants of a `ProductGroup` (sizes, finishes, …).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<ExtractProductVariant>,
}

/// One variant of a product group.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProductVariant {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attributes: std::collections::BTreeMap<String, String>,
}
