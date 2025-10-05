use crate::tools::types::{Jsonld, Metadata};
use serde::{Deserialize, Serialize};

/// Scrape result.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeResult {
    pub body: String,
    #[serde(default)]
    pub jsonld: Jsonld,
    #[serde(default)]
    pub metadata: Metadata,
}
