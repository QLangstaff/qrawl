use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Domain(pub String);

impl Domain {
    /// Canonicalize host to a stable key: lowercase + IDNA/Punycode + strip www
    fn canonicalize(host: &str) -> String {
        let lower = host.to_ascii_lowercase();
        let idna = idna::domain_to_ascii(&lower).unwrap_or(lower);

        // Strip www. prefix to normalize domains
        if idna.starts_with("www.") && idna.len() > 4 {
            idna[4..].to_string()
        } else {
            idna
        }
    }

    pub fn from_url(url: &Url) -> Option<Self> {
        url.domain().map(|d| Domain(Self::canonicalize(d)))
    }

    /// Build a Domain from raw user text (CLI, API callers, etc.)
    pub fn from_raw(host: &str) -> Self {
        Domain(Self::canonicalize(host))
    }

    /// Parse URL and extract domain in one step, with proper error handling
    pub fn parse_from_url(url: &str) -> crate::Result<(Url, Self)> {
        let parsed_url = Url::parse(url).map_err(|_| crate::QrawlError::InvalidUrl(url.into()))?;
        let domain = Self::from_url(&parsed_url).ok_or(crate::QrawlError::MissingDomain)?;
        Ok((parsed_url, domain))
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

    /// Standard browser headers for bot evasion strategies
    pub fn for_strategy(strategy: &BotEvadeStrategy) -> Self {
        match strategy {
            BotEvadeStrategy::UltraMinimal => Self::empty(),
            BotEvadeStrategy::Minimal => Self::empty()
                .with("Accept", "text/html,application/xhtml+xml")
                .with("Accept-Language", "en-US,en;q=0.9"),
            _ => Self::empty()
                .with(
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )
                .with("Accept-Language", "en-US,en;q=0.9")
                .with("Accept-Encoding", "gzip, deflate, br")
                .with("Connection", "keep-alive"),
        }
    }

    /// Legacy browser headers (used in seed inference)
    pub fn legacy_browser() -> Self {
        Self::empty()
            .with("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
            .with("Accept-Encoding", "gzip, deflate, br")
            .with("Accept-Language", "en-US,en;q=0.5")
            .with("Connection", "keep-alive")
            .with("DNT", "1")
            .with("Upgrade-Insecure-Requests", "1")
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
    Adaptive, // Try multiple approaches automatically
}

impl BotEvadeStrategy {
    /// Get user agent strings for this bot evasion strategy
    pub fn user_agents(&self) -> Vec<String> {
        match self {
            BotEvadeStrategy::UltraMinimal => vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".into(),
            ],
            _ => vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".into(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".into(),
            ],
        }
    }
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

/* ------------ Error Types (from error.rs) ------------ */

use std::fmt;

pub type Result<T> = std::result::Result<T, QrawlError>;

#[derive(Debug)]
pub enum QrawlError {
    InvalidUrl(String),
    MissingDomain,
    MissingPolicy(String),
    Other(String),
}

/* Display + Error for nicer to_string() */
impl fmt::Display for QrawlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QrawlError::InvalidUrl(u) => write!(f, "invalid url: {u}"),
            QrawlError::MissingDomain => write!(f, "missing domain in URL"),
            QrawlError::MissingPolicy(domain) => write!(f, "no policy for domain {domain}"),
            QrawlError::Other(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for QrawlError {}

/* Conversions so `?` works smoothly */
impl From<std::io::Error> for QrawlError {
    fn from(e: std::io::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}
impl From<serde_json::Error> for QrawlError {
    fn from(e: serde_json::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}
impl From<reqwest::Error> for QrawlError {
    fn from(e: reqwest::Error) -> Self {
        QrawlError::Other(e.to_string())
    }
}

/* ------------ Policy Validation (from policy.rs) ------------ */

impl Policy {
    /// Quick syntactic checks (no defaults).
    /// We enforce: at least one UA; at least one area; at least one field selector across title/headings/paragraphs.
    pub fn validate(&self) -> Result<()> {
        if self.fetch.user_agents.is_empty() {
            return Err(QrawlError::Other(
                "crawl.user_agents must not be empty".into(),
            ));
        }
        if self.scrape.areas.is_empty() {
            return Err(QrawlError::Other("scrape.areas must not be empty".into()));
        }
        let mut any_field = false;
        for a in &self.scrape.areas {
            if !(a.fields.title.is_empty()
                && a.fields.headings.is_empty()
                && a.fields.paragraphs.is_empty()
                && a.fields.images.is_empty()
                && a.fields.links.is_empty()
                && a.fields.lists.is_empty()
                && a.fields.tables.is_empty())
            {
                any_field = true;
                break;
            }
        }
        if !any_field {
            return Err(QrawlError::Other("at least one selector (title/headings/paragraphs/images/links/lists/tables) is required".into()));
        }
        Ok(())
    }
}
