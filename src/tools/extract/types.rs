use serde::{Deserialize, Serialize};

/// Extract body result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractBodyResult {
    #[serde(default)]
    pub headings: Vec<String>,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub links: Vec<String>,
}

/// Extract JSON-LD result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractJsonldResult {
    #[serde(default)]
    pub schema_types: Vec<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    pub date_published: Option<String>,
    pub date_modified: Option<String>,
}

/// Extract metadata result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractMetadataResult {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub modified_date: Option<String>,
    pub keywords: Option<String>,
    pub language: Option<String>,
    pub site_name: Option<String>,
    pub canonical_url: Option<String>,
    pub page_type: Option<String>,
}

/// Extract preview result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractPreviewResult {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// Extract recipe result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRecipeResult {
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    #[serde(default)]
    pub ingredients: Vec<String>,
    #[serde(default)]
    pub instructions: Vec<String>,
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub total_time: Option<String>,
    pub servings: Option<String>,
}
