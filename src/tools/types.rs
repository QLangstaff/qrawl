//! Shared Types

use serde_json::Value;

/// Options for pipelines.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Domains to allow (whitelist mode)
    pub allow_domains: Option<Vec<String>>,
    /// Domains to block (blacklist mode)
    pub block_domains: Option<Vec<String>>,
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_domains(mut self, domains: &[&str]) -> Self {
        self.allow_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn block_domains(mut self, domains: &[&str]) -> Self {
        self.block_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }
}

/// JSON-LD array of schema.org objects.
pub type Jsonld = Vec<Value>;

/// Metadata key-value pairs.
pub type Metadata = Vec<(String, String)>;
