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

        // Always strip www. prefix to normalize domains
        if idna.starts_with("www.") && idna.len() > 4 {
            idna[4..].to_string()
        } else {
            idna
        }
    }

    /// Create domain from host string
    pub fn new(host: &str) -> Self {
        Domain(Self::canonicalize(host))
    }

    /// Create domain from URL string
    pub fn from_url(url: &str) -> crate::Result<Self> {
        let parsed = Url::parse(url).map_err(|_| {
            crate::QrawlError::validation_error("url", &format!("invalid URL: {}", url))
        })?;
        parsed
            .domain()
            .map(|d| Domain(Self::canonicalize(d)))
            .ok_or(crate::QrawlError::validation_error(
                "url",
                "URL missing domain",
            ))
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
    // Service-specific errors
    FetchError {
        url: String,
        reason: String,
    },
    InferenceError {
        domain: String,
        operation: String,
        reason: String,
    },
    ValidationError {
        field: String,
        reason: String,
    },
    StorageError {
        operation: String,
        reason: String,
    },
    Other(String),
}

/* Display + Error for nicer to_string() */
impl fmt::Display for QrawlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QrawlError::FetchError { url, reason } => write!(f, "fetch error for {url}: {reason}"),
            QrawlError::InferenceError {
                domain,
                operation,
                reason,
            } => {
                write!(
                    f,
                    "inference error for {domain} during {operation}: {reason}"
                )
            }
            QrawlError::ValidationError { field, reason } => {
                write!(f, "validation error for {field}: {reason}")
            }
            QrawlError::StorageError { operation, reason } => {
                write!(f, "storage error during {operation}: {reason}")
            }
            QrawlError::Other(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for QrawlError {}

impl QrawlError {
    /// Create a fetch error for HTTP/network issues
    pub fn fetch_error(url: &str, reason: &str) -> Self {
        QrawlError::FetchError {
            url: url.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create an inference error for policy creation issues
    pub fn inference_error(domain: &str, operation: &str, reason: &str) -> Self {
        QrawlError::InferenceError {
            domain: domain.to_string(),
            operation: operation.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a validation error for invalid data
    pub fn validation_error(field: &str, reason: &str) -> Self {
        QrawlError::ValidationError {
            field: field.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create a storage error for file operations
    pub fn storage_error(operation: &str, reason: &str) -> Self {
        QrawlError::StorageError {
            operation: operation.to_string(),
            reason: reason.to_string(),
        }
    }
}

/* Conversions so `?` works smoothly */
impl From<std::io::Error> for QrawlError {
    fn from(e: std::io::Error) -> Self {
        // Map I/O errors to storage errors since they're typically file operations
        QrawlError::storage_error("file_operation", &e.to_string())
    }
}
impl From<serde_json::Error> for QrawlError {
    fn from(e: serde_json::Error) -> Self {
        // Map JSON errors to storage errors since they're typically policy/data serialization
        QrawlError::storage_error("json_serialization", &e.to_string())
    }
}
impl From<reqwest::Error> for QrawlError {
    fn from(e: reqwest::Error) -> Self {
        // Map HTTP client errors to fetch errors with generic URL (context-specific code should use fetch_error directly)
        QrawlError::fetch_error("unknown_url", &format!("HTTP client error: {}", e))
    }
}

/* ------------ Policy Validation (from policy.rs) ------------ */

impl Policy {
    /// Quick syntactic checks (no defaults).
    /// We enforce: at least one UA; at least one area; at least one field selector across title/headings/paragraphs.
    pub fn validate(&self) -> Result<()> {
        if self.fetch.user_agents.is_empty() {
            return Err(QrawlError::validation_error(
                "fetch.user_agents",
                "must not be empty",
            ));
        }
        if self.scrape.areas.is_empty() {
            return Err(QrawlError::validation_error(
                "scrape.areas",
                "must not be empty",
            ));
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
            return Err(QrawlError::validation_error("scrape.areas.fields", "at least one selector (title/headings/paragraphs/images/links/lists/tables) is required"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_new_basic() {
        assert_eq!(Domain::new("example.com").0, "example.com");
        assert_eq!(Domain::new("github.com").0, "github.com");
    }

    #[test]
    fn test_domain_new_canonicalization() {
        // Test case normalization
        assert_eq!(Domain::new("Example.com").0, "example.com");
        assert_eq!(Domain::new("GITHUB.COM").0, "github.com");
        assert_eq!(Domain::new("MiXeD.CaSe.CoM").0, "mixed.case.com");

        // Test www stripping
        assert_eq!(Domain::new("www.example.com").0, "example.com");
        assert_eq!(Domain::new("www.github.com").0, "github.com");
        assert_eq!(Domain::new("WWW.EXAMPLE.COM").0, "example.com");

        // Test combined case + www
        assert_eq!(Domain::new("WWW.Example.COM").0, "example.com");
        assert_eq!(Domain::new("www.GitHub.com").0, "github.com");
    }

    #[test]
    fn test_domain_new_edge_cases() {
        // Don't strip www if it's the whole domain
        assert_eq!(Domain::new("www").0, "www");
        assert_eq!(Domain::new("www.").0, "www.");

        // Always strip www when there's something after it
        assert_eq!(Domain::new("www.a").0, "a");
        assert_eq!(Domain::new("www.ab").0, "ab");
        assert_eq!(Domain::new("www.abc").0, "abc");

        // Subdomains with www
        assert_eq!(Domain::new("api.www.example.com").0, "api.www.example.com");
        assert_eq!(Domain::new("www.api.example.com").0, "api.example.com");
    }

    #[test]
    fn test_domain_from_url_basic() {
        assert_eq!(
            Domain::from_url("https://example.com").unwrap().0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("http://github.com/user/repo").unwrap().0,
            "github.com"
        );
        assert_eq!(
            Domain::from_url("https://api.example.com/v1/data")
                .unwrap()
                .0,
            "api.example.com"
        );
    }

    #[test]
    fn test_domain_from_url_canonicalization() {
        // Test case normalization in URLs
        assert_eq!(
            Domain::from_url("https://Example.com").unwrap().0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("HTTP://GITHUB.COM").unwrap().0,
            "github.com"
        );

        // Test www stripping in URLs
        assert_eq!(
            Domain::from_url("https://www.example.com").unwrap().0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("http://www.github.com/user").unwrap().0,
            "github.com"
        );

        // Test combined case + www in URLs
        assert_eq!(
            Domain::from_url("HTTPS://WWW.Example.COM/path").unwrap().0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("http://www.GitHub.com").unwrap().0,
            "github.com"
        );
    }

    #[test]
    fn test_domain_from_url_with_ports_and_paths() {
        assert_eq!(
            Domain::from_url("https://example.com:8080").unwrap().0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("http://www.example.com:3000/api/v1")
                .unwrap()
                .0,
            "example.com"
        );
        assert_eq!(
            Domain::from_url("https://API.Example.COM:443/data?q=test")
                .unwrap()
                .0,
            "api.example.com"
        );
    }

    #[test]
    fn test_domain_from_url_errors() {
        // Invalid URLs
        assert!(Domain::from_url("not-a-url").is_err());
        assert!(Domain::from_url("").is_err());
        assert!(Domain::from_url("://missing-scheme").is_err());
        assert!(Domain::from_url("https://").is_err());

        // URLs without domains
        assert!(Domain::from_url("file:///path/to/file").is_err());
        assert!(Domain::from_url("data:text/plain,hello").is_err());
    }

    #[test]
    fn test_domain_consistency() {
        // Same domain should be created consistently regardless of input method
        let domain1 = Domain::new("example.com");
        let domain2 = Domain::from_url("https://example.com").unwrap();
        assert_eq!(domain1, domain2);

        // Case variations should normalize to same domain
        let domain3 = Domain::new("Example.COM");
        let domain4 = Domain::from_url("HTTPS://Example.COM/path").unwrap();
        assert_eq!(domain3, domain4);
        assert_eq!(domain1, domain3);

        // www variations should normalize to same domain
        let domain5 = Domain::new("www.example.com");
        let domain6 = Domain::from_url("https://www.example.com").unwrap();
        assert_eq!(domain5, domain6);
        assert_eq!(domain1, domain5);
    }

    #[test]
    fn test_domain_policy_file_consistency() {
        // These should all create the same policy filename
        let domains = vec![
            Domain::new("example.com"),
            Domain::new("Example.com"),
            Domain::new("EXAMPLE.COM"),
            Domain::new("www.example.com"),
            Domain::new("www.Example.com"),
            Domain::new("WWW.EXAMPLE.COM"),
            Domain::from_url("https://example.com").unwrap(),
            Domain::from_url("HTTP://Example.com").unwrap(),
            Domain::from_url("https://www.example.com").unwrap(),
            Domain::from_url("HTTPS://WWW.Example.COM/path?query=1").unwrap(),
        ];

        let expected = "example.com";
        for domain in domains {
            assert_eq!(
                domain.0, expected,
                "Domain {:?} should normalize to {}",
                domain.0, expected
            );
        }
    }

    #[test]
    fn test_domain_real_world_cases() {
        // Test real-world domains that users might encounter
        assert_eq!(
            Domain::from_url("https://www.GitHub.com/user/repo")
                .unwrap()
                .0,
            "github.com"
        );
        assert_eq!(
            Domain::from_url("HTTP://News.YCombinator.com").unwrap().0,
            "news.ycombinator.com"
        );
        assert_eq!(
            Domain::from_url("https://WWW.REDDIT.COM/r/rust").unwrap().0,
            "reddit.com"
        );
        assert_eq!(
            Domain::from_url("http://stackoverflow.COM/questions/123")
                .unwrap()
                .0,
            "stackoverflow.com"
        );

        // Subdomains should be preserved but canonicalized
        assert_eq!(
            Domain::from_url("https://API.GitHub.com").unwrap().0,
            "api.github.com"
        );
        assert_eq!(
            Domain::from_url("https://www.api.example.COM").unwrap().0,
            "api.example.com"
        );
    }
}
