use serde::{Deserialize, Serialize};

/// Extract preview result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractPreviewResult {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}
