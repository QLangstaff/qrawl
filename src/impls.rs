use crate::{
    engine::{Fetcher as FetcherT, Scraper as ScraperT},
    error::*,
    types::*,
};
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CONNECTION,
    REFERER, USER_AGENT,
};
use scraper::{ElementRef, Html, Selector};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

/* ===========================
FETCHER (sousy-style first, HTTP/1.1)
=========================== */

pub struct ReqwestFetcher {
    client: Client,
}

impl ReqwestFetcher {
    pub fn new() -> Result<Self> {
        // Force HTTP/1.1; some WAFs expect 1.1 + Connection: keep-alive
        let client = Client::builder()
            .http1_only()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { client })
    }
}

impl FetcherT for ReqwestFetcher {
    fn name(&self) -> &'static str {
        "reqwest-blocking"
    }

    fn fetch_blocking(&self, url: &str, cfg: &CrawlConfig) -> Result<String> {
        let parsed = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let origin = format!("{}://{}/", parsed.scheme(), parsed.host_str().unwrap_or(""));

        let uas: Vec<&str> = if cfg.user_agents.is_empty() {
            vec!["Mozilla/5.0"]
        } else {
            cfg.user_agents.iter().map(|s| s.as_str()).collect()
        };

        let base = to_headermap(&cfg.default_headers, None)?;

        for (ua_idx, ua) in uas.iter().enumerate() {
            // Attempt 1: simple browser-like profile
            if let Ok(text) = self.try_once(url, base.clone(), ua, None) {
                return Ok(text);
            }

            // Small jitter before the optional referrer retry (only for first UA)
            if ua_idx == 0 {
                std::thread::sleep(std::time::Duration::from_millis(80 + jitter_ms(120)));
            }

            // Attempt 2: same-site Referer
            match self.try_once(url, base.clone(), ua, Some(&origin)) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    // If this was the last UA's last attempt, propagate the concrete error
                    if ua_idx == uas.len() - 1 {
                        return Err(e);
                    }
                }
            }

            // Between UAs
            std::thread::sleep(std::time::Duration::from_millis(120 + jitter_ms(160)));
        }

        // Shouldn't reach here, but keep a fallback
        Err(QrawlError::Other(
            "request failed after simple attempts".into(),
        ))
    }
}

impl ReqwestFetcher {
    fn try_once(
        &self,
        url: &str,
        mut headers: HeaderMap,
        ua: &str,
        referer: Option<&str>,
    ) -> Result<String> {
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
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(ua).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
        );
        if let Some(r) = referer {
            headers.insert(REFERER, HeaderValue::from_str(r).unwrap());
        }

        let resp = self.client.get(url).headers(headers).send()?;
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
    b.contains("verify you are a human")
        || b.contains("captcha")
        || b.contains("cf-browser-verification")
        || b.contains("px-captcha")
        || b.contains("access denied")
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
                            ..Default::default()
                        };
                        collect_strings(&root_el, &area.fields.title, &mut out.title, true);
                        collect_many(&root_el, &area.fields.headings, &mut out.headings);
                        collect_many(&root_el, &area.fields.paragraphs, &mut out.paragraphs);
                        collect_images(&root_el, &area.fields.images, &mut out.images);
                        collect_links(&root_el, &area.fields.links, &mut out.links);

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

fn collect_many(root: &ElementRef<'_>, sels: &[Sel], out: &mut Vec<String>) {
    for s in sels {
        if let Ok(sel) = Selector::parse(&s.0) {
            for el in root.select(&sel) {
                let txt = el.text().collect::<String>().trim().to_string();
                if !txt.is_empty() {
                    out.push(txt);
                }
            }
        }
    }
}

fn collect_images(root: &ElementRef<'_>, sels: &[Sel], out: &mut Vec<ImageOut>) {
    for s in sels {
        if let Ok(sel) = Selector::parse(&s.0) {
            for el in root.select(&sel) {
                if let Some(src) = el.value().attr("src") {
                    let alt = el.value().attr("alt").map(|s| s.to_string());
                    out.push(ImageOut {
                        src: src.to_string(),
                        alt,
                    });
                }
            }
        }
    }
}

fn collect_links(root: &ElementRef<'_>, sels: &[Sel], out: &mut Vec<LinkOut>) {
    for s in sels {
        if let Ok(sel) = Selector::parse(&s.0) {
            for el in root.select(&sel) {
                if let Some(href) = el.value().attr("href") {
                    let text = el.text().collect::<String>().trim().to_string();
                    out.push(LinkOut {
                        href: href.to_string(),
                        text,
                    });
                }
            }
        }
    }
}
