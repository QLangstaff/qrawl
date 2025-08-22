use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Domain(pub String);

impl Domain {
    /// Canonicalize host to a stable key: lowercase + IDNA/Punycode
    fn canonicalize(host: &str) -> String {
        let lower = host.to_ascii_lowercase();
        idna::domain_to_ascii(&lower).unwrap_or(lower)
    }

    pub fn from_url(url: &Url) -> Option<Self> {
        url.domain().map(|d| Domain(Self::canonicalize(d)))
    }

    /// Build a Domain from raw user text (CLI, API callers, etc.)
    pub fn from_raw(host: &str) -> Self {
        Domain(Self::canonicalize(host))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderSet(pub BTreeMap<String, String>);
impl HeaderSet {
    pub fn empty() -> Self {
        Self(BTreeMap::new())
    }
    pub fn with(mut self, k: &str, v: &str) -> Self {
        self.0.insert(k.to_string(), v.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlConfig {
    pub user_agents: Vec<String>,
    pub default_headers: HeaderSet,
    pub respect_robots_txt: bool,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sel(pub String);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum AreaRole {
    Main,
    Section,
    Sidebar,
    Header,
    Footer,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldSelectors {
    pub title: Vec<Sel>,
    pub headings: Vec<Sel>,
    pub paragraphs: Vec<Sel>,
    pub images: Vec<Sel>,
    pub links: Vec<Sel>,
    pub lists: Vec<Sel>,
    pub tables: Vec<Sel>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FollowScope {
    SameDomain,
    AnyDomain,
    AllowList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowLinks {
    pub enabled: bool,
    pub scope: FollowScope,
    pub allow_domains: Vec<String>,
    pub max: u32,
    pub dedupe: bool,
}
impl Default for FollowLinks {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: FollowScope::SameDomain,
            allow_domains: vec![],
            max: 10,
            dedupe: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaPolicy {
    pub roots: Vec<Sel>,
    pub exclude_within: Vec<Sel>,
    pub role: AreaRole,
    pub fields: FieldSelectors,
    pub is_repeating: bool,
    pub follow_links: FollowLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeConfig {
    pub extract_json_ld: bool,
    pub areas: Vec<AreaPolicy>,
}

/// Handy wrapper when you want to print or pass "config" as a single object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub crawl: CrawlConfig,
    pub scrape: ScrapeConfig,
}

/// Canonical in-memory policy type (simple & derived)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub domain: Domain,
    pub crawl: CrawlConfig,
    pub scrape: ScrapeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkOut {
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageOut {
    pub src: String,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AreaContent {
    pub role: AreaRole,
    pub root_selector_matched: String,
    pub title: Option<String>,
    pub headings: Vec<String>,
    pub paragraphs: Vec<String>,
    pub images: Vec<ImageOut>,
    pub links: Vec<LinkOut>,
    pub lists: Vec<Vec<String>>,
    pub tables: Vec<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageExtraction {
    pub url: String,
    pub domain: String,
    pub areas: Vec<AreaContent>,
    pub json_ld: Vec<serde_json::Value>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionBundle {
    pub parent: PageExtraction,
    pub children: Vec<PageExtraction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}
