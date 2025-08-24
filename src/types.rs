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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum HttpVersion {
    Http1Only,
    Http2Only,
    #[default]
    Http2WithHttp1Fallback, // Default - try HTTP/2, fallback to HTTP/1.1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum BotEvadeStrategy {
    UltraMinimal, // just User-Agent, no other headers
    Minimal,      // Basic headers: Accept, Accept-Language, Accept-Encoding
    Standard,     // Current qrawl approach (full browser simulation)
    Advanced,     // Enhanced browser fingerprint with security headers
    #[default]
    Adaptive,     // Try multiple approaches automatically
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchConfig {
    pub user_agents: Vec<String>,
    pub default_headers: HeaderSet,
    pub http_version: HttpVersion,
    pub bot_evasion_strategy: BotEvadeStrategy,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelectors {
    pub title: Vec<Sel>,
    pub headings: Vec<Sel>,
    pub paragraphs: Vec<Sel>,
    pub images: Vec<Sel>,
    pub links: Vec<Sel>,
    pub lists: Vec<Sel>,
    pub tables: Vec<Sel>,
}

impl Default for FieldSelectors {
    fn default() -> Self {
        Self {
            title: vec![
                Sel("h1".into()),
                Sel(".title".into()),
                Sel(".entry-title".into()),
            ],
            headings: vec![
                Sel("h2".into()),
                Sel("h3".into()),
                Sel("h4".into()),
                Sel("h5".into()),
                Sel("h6".into()),
            ],
            paragraphs: vec![Sel("p".into())],
            images: vec![Sel("img".into())],
            links: vec![Sel("a[href]".into())],
            lists: vec![Sel("ul".into()), Sel("ol".into())],
            tables: vec![Sel("table".into())],
        }
    }
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
            enabled: true,
            scope: FollowScope::SameDomain,
            allow_domains: vec![],
            max: 100,
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
    pub json_ld_schemas: Vec<String>,
    pub open_graph: BTreeMap<String, String>,
    pub twitter_cards: BTreeMap<String, String>,
    pub areas: Vec<AreaPolicy>,
}

/// Handy wrapper when you want to print or pass "config" as a single object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub fetch: FetchConfig,
    pub scrape: ScrapeConfig,
}

/// Performance characteristics learned during policy inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceProfile {
    pub optimal_timeout_ms: u64,
    pub working_strategy: BotEvadeStrategy,
    pub avg_response_size_bytes: u64,
    pub strategies_tried: Vec<BotEvadeStrategy>,
    pub strategies_failed: Vec<BotEvadeStrategy>,
    pub last_tested_at: DateTime<Utc>,
    pub success_rate: f64, // 0.0 to 1.0
}

/// Canonical in-memory policy type (simple & derived)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub domain: Domain,
    pub fetch: FetchConfig,
    pub scrape: ScrapeConfig,
    pub performance_profile: PerformanceProfile,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    Heading { text: String, level: u8 },
    Paragraph { text: String },
    Image { src: String, alt: Option<String> },
    Link { href: String, text: String },
    List { items: Vec<String> },
    Table { rows: Vec<Vec<String>> },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AreaContent {
    pub role: AreaRole,
    pub root_selector_matched: String,
    pub title: Option<String>,
    pub content: Vec<ContentBlock>,
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
