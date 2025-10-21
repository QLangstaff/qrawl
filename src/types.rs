//! Shared Types

use serde_json::Value;
use std::sync::Arc;

/// Context to chain tools
#[derive(Debug, Clone)]
pub struct Context {
    pub allow_domains: Option<Vec<String>>,
    pub block_domains: Option<Vec<String>>,
    pub concurrency: usize,
}

impl Context {
    pub fn new() -> Self {
        Self {
            allow_domains: None,
            block_domains: None,
            concurrency: 200,
        }
    }

    pub fn with_allow_domains(mut self, domains: &[&str]) -> Self {
        self.allow_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn with_block_domains(mut self, domains: &[&str]) -> Self {
        self.block_domains = Some(domains.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    /// Convert context to Options for tools that need it.
    pub fn as_options(&self) -> Option<Options> {
        if self.allow_domains.is_some() || self.block_domains.is_some() {
            Some(Options {
                allow_domains: self.allow_domains.clone(),
                block_domains: self.block_domains.clone(),
            })
        } else {
            None
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

tokio::task_local! {
    pub static CTX: Arc<Context>;
}

pub fn get_options() -> Options {
    CTX.try_with(|ctx| ctx.as_options())
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn get_concurrency() -> usize {
    CTX.try_with(|ctx| ctx.concurrency).ok().unwrap_or(200)
}

/// Options to customize tool behavior
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
