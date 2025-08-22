use crate::{error::*, types::*};
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use url::Url;

/* -----------------------------------------------------------------------
Inference: schema-first with www + language-root candidates and robots/sitemap
----------------------------------------------------------------------- */

pub fn infer_policy(
    fetcher: &dyn crate::engine::Fetcher,
    scraper: &dyn crate::engine::Scraper,
    domain: &Domain,
) -> Result<Policy> {
    infer_policy_with_seed(fetcher, scraper, domain, None)
}

pub fn infer_policy_with_seed(
    fetcher: &dyn crate::engine::Fetcher,
    scraper: &dyn crate::engine::Scraper,
    domain: &Domain,
    seed_url: Option<&str>,
) -> Result<Policy> {
    let schemes = ["https", "http"];
    let hosts: Vec<String> = if domain.0.starts_with("www.") {
        vec![domain.0.clone()]
    } else {
        vec![domain.0.clone(), format!("www.{}", domain.0)]
    };

    let probe_uas = vec![
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".to_string(),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:124.0) Gecko/20100101 Firefox/124.0".to_string(),
    ];

    // Sousy-like headers to seed persisted policy
    let base_headers = {
        let mut h = HeaderSet::empty();
        h = h.with(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        );
        h = h.with("Accept-Encoding", "gzip, deflate, br");
        h = h.with("Accept-Language", "en-US,en;q=0.5");
        h = h.with("Connection", "keep-alive");
        h = h.with("DNT", "1");
        h = h.with("Upgrade-Insecure-Requests", "1");
        h
    };

    let mut reasons: Vec<String> = Vec::new();
    let mut attempts: usize = 0;

    for scheme in schemes {
        for host in &hosts {
            // Build bases for this host
            let base = format!("{scheme}://{host}/");
            let base_url = match Url::parse(&base) {
                Ok(u) => u,
                Err(_) => {
                    reasons.push(format!("[{scheme}] invalid base {}", base));
                    continue;
                }
            };

            // Candidate list
            let mut candidates: Vec<String> = Vec::new();

            // 0) Seed URL (if provided and matches this host)
            if let Some(seed) = seed_url {
                if let Ok(u) = Url::parse(seed) {
                    if let Some(d) = u.domain() {
                        if d == *host {
                            candidates.push(u.to_string());
                        }
                    }
                }
            }

            // 1) Homepage + common language roots
            candidates.push(base.clone());
            for lang in ["en/", "en-us/", "us/en/", "gb/en/"] {
                candidates.push(format!("{base}{lang}"));
            }

            // 2) robots.txt -> discover sitemaps for this host
            let crawl_probe = CrawlConfig {
                user_agents: probe_uas.clone(),
                default_headers: base_headers.clone(),
                respect_robots_txt: true,
                timeout_ms: 20_000,
            };
            let robots_url = format!("{base}robots.txt");
            let mut sitemap_urls = Vec::<String>::new();
            if let Ok(txt) = fetcher.fetch_blocking(&robots_url, &crawl_probe) {
                for line in txt.lines() {
                    let line = line.trim();
                    if line.len() >= 8 && line[..8].eq_ignore_ascii_case("sitemap:") {
                        let u = line[8..].trim();
                        if let Some(abs) = absolutize(&base_url, u) {
                            sitemap_urls.push(abs);
                        }
                    }
                }
            } else {
                reasons.push(format!(
                    "[{scheme}] robots.txt fetch failed for {}",
                    robots_url
                ));
            }
            // common sitemap endpoints for this host
            sitemap_urls.push(format!("{base}sitemap.xml"));
            sitemap_urls.push(format!("{base}sitemap_index.xml"));

            // 3) Sample up to 5 content URLs from first responsive sitemap
            if !sitemap_urls.is_empty() {
                for sm in sitemap_urls {
                    if let Ok(body) = fetcher.fetch_blocking(&sm, &crawl_probe) {
                        let mut urls = extract_sitemap_urls(&body, &base_url);
                        urls.retain(|u| {
                            Url::parse(u)
                                .ok()
                                .and_then(|uu| uu.domain().map(|d| d == host.as_str()))
                                .unwrap_or(false)
                                && !u.ends_with(".xml")
                                && !u.ends_with(".gz")
                        });
                        for u in urls.into_iter().take(5) {
                            candidates.push(u);
                        }
                        break; // use the first working sitemap
                    } else {
                        reasons.push(format!("[{scheme}] sitemap fetch failed for {}", sm));
                    }
                }
            }

            // De-dup candidates
            {
                let mut seen = HashSet::new();
                candidates.retain(|u| seen.insert(u.clone()));
            }

            // Try each candidate
            for cand in candidates {
                attempts += 1;

                let crawl_attempt = CrawlConfig {
                    user_agents: probe_uas.clone(), // fetcher rotates UA + simple referrer retry
                    default_headers: base_headers.clone(),
                    respect_robots_txt: true,
                    timeout_ms: 20_000,
                };

                let html = match fetcher.fetch_blocking(&cand, &crawl_attempt) {
                    Ok(h) => h,
                    Err(e) => {
                        reasons.push(format!(
                            "[{scheme}] fetch failed ({}) at {}",
                            trim_status(&e.to_string()),
                            cand
                        ));
                        continue;
                    }
                };

                if !has_structured_data(&html) {
                    reasons.push(format!("[{scheme}] no structured data at {}", cand));
                    continue;
                }

                let mut areas = Vec::<AreaPolicy>::new();
                if has_itemlist_schema(&html) {
                    areas.push(AreaPolicy {
                        roots: vec![],
                        exclude_within: vec![],
                        role: AreaRole::Main,
                        fields: FieldSelectors::default(),
                        is_repeating: true,
                        follow_links: FollowLinks {
                            enabled: true,
                            scope: FollowScope::SameDomain,
                            allow_domains: vec![],
                            max: 10,
                            dedupe: true,
                        },
                    });
                }

                let scrape = ScrapeConfig {
                    extract_json_ld: true,
                    areas,
                };

                match scraper.scrape(&cand, &html, &scrape) {
                    Ok(page) => {
                        if page.json_ld.is_empty() {
                            reasons.push(format!(
                                "[{scheme}] structured data present but parsed JSON-LD empty at {}",
                                cand
                            ));
                            continue;
                        }
                        let final_crawl = CrawlConfig {
                            user_agents: probe_uas.clone(),
                            default_headers: base_headers.clone(),
                            respect_robots_txt: true,
                            timeout_ms: 20_000,
                        };
                        return Ok(Policy {
                            domain: Domain(host.clone()),
                            crawl: final_crawl,
                            scrape,
                        });
                    }
                    Err(e) => {
                        reasons.push(format!("[{scheme}] scrape failed at {}: {}", cand, e));
                        continue;
                    }
                }
            }
        }
    }

    let summary = summarize_reasons(&reasons, 8);
    Err(QrawlError::Other(format!(
        "unable to infer policy for {}. attempts={}. {}",
        domain.0, attempts, summary
    )))
}

/* ---------------- helpers: diagnostics ---------------- */

fn summarize_reasons(reasons: &[String], top_n: usize) -> String {
    if reasons.is_empty() {
        return "no further details".into();
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for r in reasons {
        *counts.entry(r.as_str()).or_default() += 1;
    }
    let mut items: Vec<(&str, usize)> = counts.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let top = items
        .into_iter()
        .take(top_n)
        .map(|(msg, n)| format!("{n}× {msg}"))
        .collect::<Vec<_>>()
        .join(" | ");
    format!("Top reasons: {top}")
}

fn trim_status(s: &str) -> String {
    if let Some(pos) = s.find("status ") {
        return s[pos..]
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
    }
    if let Some(pos) = s.find(" for http") {
        return s[..pos].to_string();
    }
    s.to_string()
}

/* ---------------- helpers: detection ---------------- */

fn has_structured_data(html: &str) -> bool {
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) {
        if doc.select(&sel).next().is_some() {
            return true;
        }
    }
    if let Ok(sel) = Selector::parse(r#"[itemscope]"#) {
        if doc.select(&sel).next().is_some() {
            return true;
        }
    }
    if let Ok(sel) = Selector::parse(r#"[typeof],[property],[about],[rel],[vocab]"#) {
        if doc.select(&sel).next().is_some() {
            return true;
        }
    }
    false
}

fn has_itemlist_schema(html: &str) -> bool {
    let doc = Html::parse_document(html);
    let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) else {
        return false;
    };
    for s in doc.select(&sel) {
        if let Some(txt) = s.text().next() {
            if itemlist_in_jsonld_text(txt) {
                return true;
            }
        }
    }
    false
}

fn itemlist_in_jsonld_text(txt: &str) -> bool {
    let txt = txt.trim();
    if txt.is_empty() {
        return false;
    }
    if let Ok(v) = serde_json::from_str::<Value>(txt) {
        if contains_itemlist(&v) {
            return true;
        }
    }
    let bracketed = format!("[{}]", txt);
    if let Ok(v) = serde_json::from_str::<Value>(&bracketed) {
        if contains_itemlist(&v) {
            return true;
        }
    }
    false
}

fn contains_itemlist(v: &Value) -> bool {
    match v {
        Value::Array(arr) => arr.iter().any(contains_itemlist),
        Value::Object(map) => {
            if let Some(t) = map.get("@type") {
                if type_is_itemlist(t) {
                    return true;
                }
            }
            if let Some(graph) = map.get("@graph") {
                if contains_itemlist(graph) {
                    return true;
                }
            }
            if map.contains_key("itemListElement") {
                return true;
            }
            map.values().any(contains_itemlist)
        }
        _ => false,
    }
}

fn type_is_itemlist(t: &Value) -> bool {
    match t {
        Value::String(s) => s.eq_ignore_ascii_case("ItemList"),
        Value::Array(arr) => arr.iter().any(|v| {
            v.as_str()
                .map(|s| s.eq_ignore_ascii_case("ItemList"))
                .unwrap_or(false)
        }),
        _ => false,
    }
}

/* ---------------- helpers: sitemap + urls ---------------- */

fn extract_sitemap_urls(xml: &str, base: &Url) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut i = 0usize;
    let bytes = xml.as_bytes();
    while let Some(s) = find_tag(bytes, i, b"<loc>") {
        if let Some(e) = find_tag(bytes, s, b"</loc>") {
            if e > s + 5 {
                let inner = &xml[s + 5..e];
                if let Some(abs) = absolutize(base, inner.trim()) {
                    out.push(abs);
                }
            }
            i = e + 6;
        } else {
            break;
        }
    }
    let mut seen = HashSet::new();
    out.retain(|u| seen.insert(u.clone()));
    out
}

fn find_tag(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn absolutize(base: &Url, link: &str) -> Option<String> {
    if let Ok(u) = Url::parse(link) {
        return Some(u.to_string());
    }
    base.join(link).ok().map(|u| u.to_string())
}
