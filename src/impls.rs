use crate::{
    engine::{Fetcher as FetcherT, Scraper as ScraperT},
    error::*,
    types::*,
};
use async_trait::async_trait;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL,
    CONNECTION, REFERER, UPGRADE_INSECURE_REQUESTS, USER_AGENT,
};
use reqwest::Client as AsyncClient;
use scraper::{ElementRef, Html, Selector};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

/* ===========================
FETCHER
=========================== */

pub struct ReqwestFetcher;

impl ReqwestFetcher {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn build_client_for_policy(&self, cfg: &FetchConfig) -> Result<Client> {
        if matches!(cfg.bot_evasion_strategy, BotEvadeStrategy::UltraMinimal) {
            return Ok(Client::builder().timeout(Duration::from_secs(30)).build()?);
        }

        // For other strategies, use full-featured client
        let mut builder = Client::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(10));

        // Configure HTTP version based on policy
        match cfg.http_version {
            HttpVersion::Http1Only => {
                builder = builder.http1_only();
            }
            HttpVersion::Http2Only => {
                builder = builder.http2_prior_knowledge();
            }
            HttpVersion::Http2WithHttp1Fallback => {
                // Default reqwest behavior - try HTTP/2, fallback to HTTP/1.1
                // No additional configuration needed
            }
        }

        Ok(builder.build()?)
    }

    fn build_async_client_for_policy(&self, cfg: &FetchConfig) -> Result<AsyncClient> {
        if matches!(cfg.bot_evasion_strategy, BotEvadeStrategy::UltraMinimal) {
            return Ok(AsyncClient::builder()
                .timeout(Duration::from_secs(30))
                .build()?);
        }

        // For other strategies, use full-featured client
        let mut builder = AsyncClient::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(10));

        // Configure HTTP version based on policy
        match cfg.http_version {
            HttpVersion::Http1Only => {
                builder = builder.http1_only();
            }
            HttpVersion::Http2Only => {
                builder = builder.http2_prior_knowledge();
            }
            HttpVersion::Http2WithHttp1Fallback => {
                // Default reqwest behavior - try HTTP/2, fallback to HTTP/1.1
                // No additional configuration needed
            }
        }

        Ok(builder.build()?)
    }
}

#[async_trait]
impl FetcherT for ReqwestFetcher {
    fn name(&self) -> &'static str {
        "reqwest-blocking"
    }

    fn fetch_blocking(&self, url: &str, cfg: &FetchConfig) -> Result<String> {
        let parsed = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let origin = format!("{}://{}/", parsed.scheme(), parsed.host_str().unwrap_or(""));

        // Build client based on policy configuration
        let client = self.build_client_for_policy(cfg)?;

        let uas: Vec<&str> = if cfg.user_agents.is_empty() {
            vec!["Mozilla/5.0"]
        } else {
            cfg.user_agents.iter().map(|s| s.as_str()).collect()
        };

        let base = to_headermap(&cfg.default_headers, None)?;

        // Determine evasion strategies to try
        let strategies = match &cfg.bot_evasion_strategy {
            BotEvadeStrategy::Adaptive => {
                // Progressive fallback: UltraMinimal -> Minimal -> Standard -> Advanced
                // Start with ultra-minimal approach for sophisticated detection
                vec![
                    BotEvadeStrategy::UltraMinimal,
                    BotEvadeStrategy::Minimal,
                    BotEvadeStrategy::Standard,
                    BotEvadeStrategy::Advanced,
                ]
            }
            other => vec![other.clone()],
        };

        for (strategy_idx, strategy) in strategies.iter().enumerate() {
            for (ua_idx, ua) in uas.iter().enumerate() {
                // Attempt 1: strategy with no referer
                if let Ok(text) = self.try_once(&client, url, base.clone(), ua, None, strategy) {
                    return Ok(text);
                }

                // Small jitter before the optional referrer retry (only for first UA of first strategy)
                if strategy_idx == 0 && ua_idx == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(80 + jitter_ms(120)));
                }

                // Attempt 2: same-site Referer
                match self.try_once(&client, url, base.clone(), ua, Some(&origin), strategy) {
                    Ok(text) => return Ok(text),
                    Err(e) => {
                        // If this was the last strategy's last UA's last attempt, propagate error
                        if strategy_idx == strategies.len() - 1 && ua_idx == uas.len() - 1 {
                            return Err(e);
                        }
                    }
                }

                // Between UAs within same strategy
                std::thread::sleep(std::time::Duration::from_millis(120 + jitter_ms(160)));
            }

            // Between strategies - longer pause
            if strategy_idx < strategies.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(300 + jitter_ms(200)));
            }
        }

        // Shouldn't reach here, but keep a fallback
        Err(QrawlError::Other(
            "request failed after all evasion strategies".into(),
        ))
    }

    async fn fetch_async(&self, url: &str, cfg: &FetchConfig) -> Result<String> {
        let parsed = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let origin = format!("{}://{}/", parsed.scheme(), parsed.host_str().unwrap_or(""));

        // Build async client based on policy configuration
        let client = self.build_async_client_for_policy(cfg)?;

        let uas: Vec<&str> = if cfg.user_agents.is_empty() {
            vec!["Mozilla/5.0"]
        } else {
            cfg.user_agents.iter().map(|s| s.as_str()).collect()
        };

        let base = to_headermap(&cfg.default_headers, None)?;

        // Determine evasion strategies to try
        let strategies = match &cfg.bot_evasion_strategy {
            BotEvadeStrategy::Adaptive => {
                // Progressive fallback: UltraMinimal -> Minimal -> Standard -> Advanced
                // Start with ultra-minimal approach for sophisticated detection
                vec![
                    BotEvadeStrategy::UltraMinimal,
                    BotEvadeStrategy::Minimal,
                    BotEvadeStrategy::Standard,
                    BotEvadeStrategy::Advanced,
                ]
            }
            other => vec![other.clone()],
        };

        for (strategy_idx, strategy) in strategies.iter().enumerate() {
            for (ua_idx, ua) in uas.iter().enumerate() {
                // Attempt 1: strategy with no referer
                if let Ok(text) = self
                    .try_once_async(&client, url, base.clone(), ua, None, strategy)
                    .await
                {
                    return Ok(text);
                }

                // Small jitter before the optional referrer retry (only for first UA of first strategy)
                if strategy_idx == 0 && ua_idx == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(80 + jitter_ms(120)))
                        .await;
                }

                // Attempt 2: same-site Referer
                match self
                    .try_once_async(&client, url, base.clone(), ua, Some(&origin), strategy)
                    .await
                {
                    Ok(text) => return Ok(text),
                    Err(e) => {
                        // If this was the last strategy's last UA's last attempt, propagate error
                        if strategy_idx == strategies.len() - 1 && ua_idx == uas.len() - 1 {
                            return Err(e);
                        }
                    }
                }

                // Between UAs within same strategy
                tokio::time::sleep(tokio::time::Duration::from_millis(120 + jitter_ms(160))).await;
            }

            // Between strategies - longer pause
            if strategy_idx < strategies.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(300 + jitter_ms(200))).await;
            }
        }

        // Shouldn't reach here, but keep a fallback
        Err(QrawlError::Other(
            "request failed after all evasion strategies".into(),
        ))
    }
}

impl ReqwestFetcher {
    fn try_once(
        &self,
        client: &Client,
        url: &str,
        mut headers: HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) -> Result<String> {
        self.apply_evasion_strategy(&mut headers, ua, referer, strategy);

        let resp = client.get(url).headers(headers).send()?;
        let status = resp.status();
        let text = resp.text()?;

        if status.is_success() && !looks_blocked(&text) {
            return Ok(text);
        }
        Err(QrawlError::Other(format!(
            "http status {} for {}",
            status, url
        )))
    }

    fn apply_evasion_strategy(
        &self,
        headers: &mut HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) {
        match strategy {
            BotEvadeStrategy::UltraMinimal => {
                // Ultra minimal: ONLY User-Agent header
                // No Accept, Accept-Language, Accept-Encoding - nothing that screams "browser"
            }
            BotEvadeStrategy::Minimal => {
                // Basic headers for sites that expect some browser-like behavior
                headers
                    .entry(ACCEPT)
                    .or_insert(HeaderValue::from_static("text/html;q=0.9,*/*;q=0.8"));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.8"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br"));
                // No DNT, no Upgrade-Insecure-Requests, no Connection header
            }
            BotEvadeStrategy::Standard => {
                // Current qrawl approach (full browser simulation)
                headers.entry(ACCEPT).or_insert(HeaderValue::from_static(
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
                ));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.5"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br"));
                headers
                    .entry(CONNECTION)
                    .or_insert(HeaderValue::from_static("keep-alive"));
                headers.insert(
                    HeaderName::from_static("upgrade-insecure-requests"),
                    HeaderValue::from_static("1"),
                );
                headers.insert(
                    HeaderName::from_static("dnt"),
                    HeaderValue::from_static("1"),
                );
            }
            BotEvadeStrategy::Advanced => {
                // Enhanced browser fingerprint with security headers
                headers.entry(ACCEPT).or_insert(HeaderValue::from_static(
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"
                ));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.9"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br, zstd"));
                headers
                    .entry(CONNECTION)
                    .or_insert(HeaderValue::from_static("keep-alive"));
                headers.insert(
                    HeaderName::from_static("upgrade-insecure-requests"),
                    HeaderValue::from_static("1"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-dest"),
                    HeaderValue::from_static("document"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-mode"),
                    HeaderValue::from_static("navigate"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-site"),
                    HeaderValue::from_static("none"),
                );
            }
            BotEvadeStrategy::Adaptive => {
                // This will be handled by the caller with fallback logic
                // Default to Standard for individual attempts - apply directly to avoid recursion
                headers.insert(
                    ACCEPT,
                    HeaderValue::from_static(
                        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    ),
                );
                headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
                headers.insert(
                    ACCEPT_ENCODING,
                    HeaderValue::from_static("gzip, deflate, br"),
                );
                headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
                headers.insert(UPGRADE_INSECURE_REQUESTS, HeaderValue::from_static("1"));
                headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=0"));
                if let Some(ref_url) = referer {
                    if let Ok(ref_value) = HeaderValue::from_str(ref_url) {
                        headers.insert(REFERER, ref_value);
                    }
                }
            }
        }

        // Add User-Agent - use different UA for UltraMinimal
        let user_agent = match strategy {
            BotEvadeStrategy::UltraMinimal => {
                // Linux User-Agent
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
            }
            _ => ua,
        };
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(user_agent).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
        );
        if let Some(r) = referer {
            headers.insert(REFERER, HeaderValue::from_str(r).unwrap());
        }
    }

    async fn try_once_async(
        &self,
        client: &AsyncClient,
        url: &str,
        mut headers: HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) -> Result<String> {
        self.apply_evasion_strategy(&mut headers, ua, referer, strategy);

        let resp = client.get(url).headers(headers).send().await?;
        let status = resp.status();
        let text = resp.text().await?;

        if status.is_success() && !looks_blocked(&text) {
            return Ok(text);
        }
        Err(QrawlError::Other(format!(
            "http status {} for {}",
            status, url
        )))
    }
}

// Convert policy headers into a HeaderMap
fn to_headermap(hs: &HeaderSet, ua: Option<&str>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (k, v) in &hs.0 {
        let kn = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| QrawlError::Other(format!("bad header name {}: {}", k, e)))?;
        let vv = HeaderValue::from_str(v)
            .map_err(|e| QrawlError::Other(format!("bad header value for {}: {}", k, e)))?;
        headers.insert(kn, vv);
    }
    if let Some(ua_str) = ua {
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(ua_str).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
        );
    }
    Ok(headers)
}

// Simple block-page detector
fn looks_blocked(body: &str) -> bool {
    let b = body.to_ascii_lowercase();

    // Check for specific blocking patterns that indicate actual bot blocking,
    // not just mentions of security technologies like reCAPTCHA
    b.contains("verify you are a human")
        || b.contains("please complete the captcha")
        || b.contains("solve this captcha")
        || b.contains("captcha challenge")
        || b.contains("cf-browser-verification")
        || b.contains("px-captcha")
        || b.contains("access denied")
        || b.contains("blocked by cloudflare")
        || b.contains("please enable javascript and cookies")
        || b.contains("suspicious activity")
        || b.contains("bot detection")
}

// Small, dependency-free jitter (ms)
fn jitter_ms(range: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_nanos(0));
    let nanos = now.subsec_nanos() as u64;
    let micros = (now.as_micros() & 0xFFFF) as u64;
    (nanos ^ (micros << 5)) % range
}

/* ===========================
SCRAPER (schema-first)
=========================== */

pub struct DefaultScraper;
impl ScraperT for DefaultScraper {
    fn name(&self) -> &'static str {
        "default-scraper"
    }

    fn scrape(&self, url: &str, html: &str, cfg: &ScrapeConfig) -> Result<PageExtraction> {
        let doc = Html::parse_document(html);

        // JSON-LD first
        let mut json_ld = Vec::<serde_json::Value>::new();
        if cfg.extract_json_ld {
            if let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) {
                for s in doc.select(&sel) {
                    if let Some(txt) = s.text().next() {
                        if let Some(mut vals) = parse_json_ld_block(txt) {
                            json_ld.append(&mut vals);
                        }
                    }
                }
            }
        }

        // Optional CSS areas (manual policies)
        let mut areas_out = Vec::<AreaContent>::new();
        for area in &cfg.areas {
            if area.roots.is_empty() {
                continue;
            }
            for rsel in &area.roots {
                if let Ok(sel) = Selector::parse(&rsel.0) {
                    for root_el in doc.select(&sel) {
                        if is_excluded(&root_el, &area.exclude_within) {
                            continue;
                        }

                        let mut out = AreaContent {
                            role: area.role,
                            root_selector_matched: rsel.0.clone(),
                            title: None,
                            content: Vec::new(),
                        };
                        collect_strings(&root_el, &area.fields.title, &mut out.title, true);
                        collect_content_blocks(&root_el, &area.fields, &mut out.content);

                        areas_out.push(out);
                    }
                }
            }
        }

        Ok(PageExtraction {
            url: url.to_string(),
            domain: Url::parse(url)
                .ok()
                .and_then(|u| u.domain().map(|d| d.to_string()))
                .unwrap_or_default(),
            areas: areas_out,
            json_ld,
            fetched_at: chrono::Utc::now(),
        })
    }
}

/* -------- JSON-LD helpers -------- */

fn parse_json_ld_block(txt: &str) -> Option<Vec<serde_json::Value>> {
    let txt = txt.trim();
    if txt.is_empty() {
        return None;
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(txt) {
        return Some(flatten_jsonld(v));
    }
    let bracketed = format!("[{}]", txt);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&bracketed) {
        return Some(flatten_jsonld(v));
    }
    None
}

fn flatten_jsonld(v: serde_json::Value) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    match v {
        serde_json::Value::Array(arr) => {
            for it in arr {
                out.extend(flatten_jsonld(it));
            }
        }
        serde_json::Value::Object(mut obj) => {
            if let Some(graph) = obj.remove("@graph") {
                out.extend(flatten_jsonld(graph));
                if !obj.is_empty() {
                    out.push(serde_json::Value::Object(obj));
                }
            } else {
                out.push(serde_json::Value::Object(obj));
            }
        }
        other => out.push(other),
    }
    out
}

/* -------- CSS helpers -------- */

fn is_excluded(root: &ElementRef<'_>, exclude: &[Sel]) -> bool {
    for s in exclude {
        if let Ok(sel) = Selector::parse(&s.0) {
            if root.select(&sel).next().is_some() {
                return true;
            }
        }
    }
    false
}

fn collect_strings(
    root: &ElementRef<'_>,
    sels: &[Sel],
    target: &mut Option<String>,
    first_only: bool,
) {
    for s in sels {
        if let Ok(sel) = Selector::parse(&s.0) {
            if let Some(el) = root.select(&sel).next() {
                let txt = el.text().collect::<String>().trim().to_string();
                if !txt.is_empty() {
                    *target = Some(txt);
                    if first_only {
                        return;
                    }
                }
            }
        }
    }
}

fn collect_content_blocks(
    root: &ElementRef<'_>,
    fields: &FieldSelectors,
    out: &mut Vec<ContentBlock>,
) {
    // Collect content by type in document order to preserve structure

    // Use a universal selector to get all elements in document order
    if let Ok(all_selector) = Selector::parse("*") {
        for el in root.select(&all_selector) {
            let tag_name = el.value().name();

            // Check if this element matches any of our configured selectors and process accordingly
            match tag_name {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    // Check if this heading matches any heading selector
                    let matches = fields.headings.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        let level = match tag_name {
                            "h1" => 1,
                            "h2" => 2,
                            "h3" => 3,
                            "h4" => 4,
                            "h5" => 5,
                            "h6" => 6,
                            _ => 2,
                        };
                        let text = el.text().collect::<String>().trim().to_string();
                        if !text.is_empty() {
                            out.push(ContentBlock::Heading { text, level });
                        }
                    }
                }
                "p" => {
                    // Check if this paragraph matches any paragraph selector
                    let matches = fields.paragraphs.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        let text = el.text().collect::<String>().trim().to_string();
                        if !text.is_empty() {
                            out.push(ContentBlock::Paragraph { text });
                        }
                    }
                }
                "img" => {
                    // Check if this image matches any image selector
                    let matches = fields.images.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        if let Some(src) = el.value().attr("src") {
                            let alt = el.value().attr("alt").map(|s| s.to_string());
                            out.push(ContentBlock::Image {
                                src: src.to_string(),
                                alt,
                            });
                        }
                    }
                }
                "a" => {
                    // Check if this link matches any link selector
                    let matches = fields.links.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        if let Some(href) = el.value().attr("href") {
                            let text = el.text().collect::<String>().trim().to_string();
                            out.push(ContentBlock::Link {
                                href: href.to_string(),
                                text,
                            });
                        }
                    }
                }
                "ul" | "ol" => {
                    // Check if this list matches any list selector
                    let matches = fields.lists.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        let mut items = Vec::new();
                        if let Ok(li_sel) = Selector::parse("li") {
                            for li in el.select(&li_sel) {
                                let text = li.text().collect::<String>().trim().to_string();
                                if !text.is_empty() {
                                    items.push(text);
                                }
                            }
                        }
                        if !items.is_empty() {
                            out.push(ContentBlock::List { items });
                        }
                    }
                }
                "table" => {
                    // Check if this table matches any table selector
                    let matches = fields.tables.iter().any(|sel| {
                        if let Ok(selector) = Selector::parse(&sel.0) {
                            root.select(&selector)
                                .any(|matching_el| matching_el.id() == el.id())
                        } else {
                            false
                        }
                    });

                    if matches {
                        let mut rows = Vec::new();
                        if let Ok(row_sel) = Selector::parse("tr") {
                            for tr in el.select(&row_sel) {
                                let mut cells = Vec::new();
                                if let Ok(cell_sel) = Selector::parse("td, th") {
                                    for cell in tr.select(&cell_sel) {
                                        let text =
                                            cell.text().collect::<String>().trim().to_string();
                                        cells.push(text);
                                    }
                                }
                                if !cells.is_empty() {
                                    rows.push(cells);
                                }
                            }
                        }
                        if !rows.is_empty() {
                            out.push(ContentBlock::Table { rows });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
