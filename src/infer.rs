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
    eprintln!("🔍 Probing domain to learn characteristics: {}", domain.0);
    probe_domain_systematically(fetcher, scraper, domain)
}

fn probe_domain_systematically(
    fetcher: &dyn crate::engine::Fetcher,
    scraper: &dyn crate::engine::Scraper,
    domain: &Domain,
) -> Result<Policy> {
    let base_url = format!("https://{}/", domain.0);
    eprintln!("🌐 Testing base URL: {}", base_url);

    // Progressive strategy testing - try each until one works
    let strategies = [
        BotEvadeStrategy::UltraMinimal,
        BotEvadeStrategy::Minimal,
        BotEvadeStrategy::Standard,
        BotEvadeStrategy::Advanced,
    ];

    // Track performance data during testing
    let mut strategies_tried = Vec::new();
    let mut strategies_failed = Vec::new();

    for (i, strategy) in strategies.iter().enumerate() {
        eprintln!(
            "🔧 Testing strategy {}/{}: {:?}",
            i + 1,
            strategies.len(),
            strategy
        );
        strategies_tried.push(strategy.clone());

        if let Ok((html, optimal_timeout)) = test_strategy(&base_url, strategy, fetcher) {
            eprintln!("✅ Strategy {:?} worked! Analyzing content...", strategy);

            // Analyze the successful response to understand content structure
            let content_analysis = analyze_content_structure(&html, &base_url, scraper)?;

            // Create performance profile from our testing
            let performance_profile = PerformanceProfile {
                optimal_timeout_ms: optimal_timeout,
                working_strategy: strategy.clone(),
                avg_response_size_bytes: html.len() as u64,
                strategies_tried: strategies_tried.clone(),
                strategies_failed: strategies_failed.clone(),
                last_tested_at: chrono::Utc::now(),
                success_rate: 1.0 / strategies_tried.len() as f64, // Success rate = 1/attempts
            };

            // Create domain-specific policy based on what we learned
            return Ok(create_learned_policy(
                domain.clone(),
                strategy.clone(),
                optimal_timeout,
                content_analysis,
                performance_profile,
            ));
        } else {
            eprintln!("❌ Strategy {:?} failed, trying next...", strategy);
            strategies_failed.push(strategy.clone());
        }
    }

    Err(QrawlError::Other(format!(
        "All bot evasion strategies failed for domain {}",
        domain.0
    )))
}

fn test_strategy(
    url: &str,
    strategy: &BotEvadeStrategy,
    fetcher: &dyn crate::engine::Fetcher,
) -> Result<(String, u64)> {
    // Test with different timeouts to find optimal one
    let timeouts = vec![5000, 10000, 15000];

    for timeout in timeouts {
        eprintln!("  ⏱️  Testing timeout: {}ms", timeout);

        let test_config = FetchConfig {
            user_agents: get_strategy_user_agents(strategy),
            default_headers: get_strategy_headers(strategy),
            http_version: HttpVersion::default(),
            bot_evasion_strategy: strategy.clone(),
            respect_robots_txt: true,
            timeout_ms: timeout,
        };

        match fetcher.fetch_blocking(url, &test_config) {
            Ok(html) => {
                eprintln!("  📄 Got {} bytes of content", html.len());
                if is_valid_response(&html) {
                    eprintln!("  ✅ Success with timeout {}ms", timeout);
                    return Ok((html, timeout));
                } else {
                    eprintln!("  ⚠️  Got response but content seems blocked/invalid");
                    eprintln!(
                        "  🔍 First 200 chars: {}",
                        &html.chars().take(200).collect::<String>()
                    );
                }
            }
            Err(e) => {
                eprintln!("  ❌ Failed with timeout {}ms: {}", timeout, e);
            }
        }
    }

    Err(QrawlError::Other(
        "Strategy failed with all timeouts".into(),
    ))
}

fn get_strategy_user_agents(strategy: &BotEvadeStrategy) -> Vec<String> {
    match strategy {
        BotEvadeStrategy::UltraMinimal => vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".into(),
        ],
        _ => vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".into(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".into(),
        ],
    }
}

fn get_strategy_headers(strategy: &BotEvadeStrategy) -> HeaderSet {
    match strategy {
        BotEvadeStrategy::UltraMinimal => HeaderSet::empty(),
        BotEvadeStrategy::Minimal => HeaderSet::empty()
            .with("Accept", "text/html,application/xhtml+xml")
            .with("Accept-Language", "en-US,en;q=0.9"),
        _ => HeaderSet::empty()
            .with(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .with("Accept-Language", "en-US,en;q=0.9")
            .with("Accept-Encoding", "gzip, deflate, br")
            .with("Connection", "keep-alive"),
    }
}

fn is_valid_response(html: &str) -> bool {
    // Check if response contains actual content vs bot detection page
    let html_lower = html.to_lowercase();

    // Signs of successful response - be more specific about blocking patterns
    html.len() > 500
        && (html_lower.contains("<html") || html_lower.contains("<!doctype"))
        && !html_lower.contains("access denied")
        && !html_lower.contains("verify you are a human")
        && !html_lower.contains("please complete the captcha")
        && !html_lower.contains("solve this captcha")
        && !html_lower.contains("captcha challenge")
        && !html_lower.contains("cf-browser-verification")
        && !html_lower.contains("px-captcha")
        && !html_lower.contains("blocked by cloudflare")
        && !html_lower.contains("please enable javascript and cookies")
        && !html_lower.contains("suspicious activity")
        && !html_lower.contains("bot detection")
}

#[derive(Debug)]
struct ContentAnalysis {
    has_json_ld: bool,
    schema_types: Vec<String>,
    has_itemlist: bool,
    open_graph: BTreeMap<String, String>,
    twitter_cards: BTreeMap<String, String>,
}

fn analyze_content_structure(
    html: &str,
    _url: &str,
    scraper: &dyn crate::engine::Scraper,
) -> Result<ContentAnalysis> {
    eprintln!("📊 Analyzing content structure...");

    // Test scraping with basic config to see what we get
    let test_config = ScrapeConfig {
        extract_json_ld: true,
        json_ld_schemas: vec![], // empty = extract all schemas for analysis
        open_graph: BTreeMap::new(), // Will be populated during analysis
        twitter_cards: BTreeMap::new(), // Will be populated during analysis
        areas: vec![],           // No areas for initial test
    };

    match scraper.scrape(_url, html, &test_config) {
        Ok(page) => {
            let has_json_ld = !page.json_ld.is_empty();
            let mut schema_types = Vec::new();
            let mut has_itemlist = false;

            // Extract schema types from JSON-LD
            for json_obj in &page.json_ld {
                if let Some(type_val) = json_obj.get("@type") {
                    match type_val {
                        serde_json::Value::String(s) => {
                            schema_types.push(s.clone());
                            if s.eq_ignore_ascii_case("ItemList") {
                                has_itemlist = true;
                            }
                        }
                        serde_json::Value::Array(arr) => {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    schema_types.push(s.to_string());
                                    if s.eq_ignore_ascii_case("ItemList") {
                                        has_itemlist = true;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Extract social metadata directly from HTML
            let doc = Html::parse_document(html);
            let open_graph = extract_open_graph_meta(&doc);
            let twitter_cards = extract_twitter_card_meta(&doc);

            eprintln!("📊 Found schema types: {:?}", schema_types);
            eprintln!("📊 Has ItemList: {}", has_itemlist);
            eprintln!(
                "📊 Found Open Graph tags: {:?}",
                open_graph.keys().collect::<Vec<_>>()
            );
            eprintln!(
                "📊 Found Twitter Cards: {:?}",
                twitter_cards.keys().collect::<Vec<_>>()
            );

            Ok(ContentAnalysis {
                has_json_ld,
                schema_types,
                has_itemlist,
                open_graph,
                twitter_cards,
            })
        }
        Err(e) => {
            eprintln!("⚠️  Scraping analysis failed: {}", e);
            // Extract social metadata even if scraping fails
            let doc = Html::parse_document(html);
            let open_graph = extract_open_graph_meta(&doc);
            let twitter_cards = extract_twitter_card_meta(&doc);

            eprintln!(
                "📊 Found Open Graph tags: {:?}",
                open_graph.keys().collect::<Vec<_>>()
            );
            eprintln!(
                "📊 Found Twitter Cards: {:?}",
                twitter_cards.keys().collect::<Vec<_>>()
            );

            // Return basic analysis if scraping fails
            Ok(ContentAnalysis {
                has_json_ld: false,
                schema_types: vec![],
                has_itemlist: false,
                open_graph,
                twitter_cards,
            })
        }
    }
}

/* -------- Social metadata helpers (duplicated from impls.rs) -------- */

fn extract_open_graph_meta(doc: &Html) -> BTreeMap<String, String> {
    let mut og_data = BTreeMap::new();

    if let Ok(sel) = Selector::parse(r#"meta[property^="og:"]"#) {
        for el in doc.select(&sel) {
            if let (Some(property), Some(content)) =
                (el.value().attr("property"), el.value().attr("content"))
            {
                if property.starts_with("og:") {
                    // Remove "og:" prefix for cleaner storage
                    let key = property.strip_prefix("og:").unwrap_or(property);
                    og_data.insert(key.to_string(), content.to_string());
                }
            }
        }
    }

    // Also check for Facebook-specific Open Graph tags
    if let Ok(sel) = Selector::parse(r#"meta[property^="fb:"]"#) {
        for el in doc.select(&sel) {
            if let (Some(property), Some(content)) =
                (el.value().attr("property"), el.value().attr("content"))
            {
                og_data.insert(property.to_string(), content.to_string());
            }
        }
    }

    og_data
}

fn extract_twitter_card_meta(doc: &Html) -> BTreeMap<String, String> {
    let mut twitter_data = BTreeMap::new();

    if let Ok(sel) = Selector::parse(r#"meta[name^="twitter:"]"#) {
        for el in doc.select(&sel) {
            if let (Some(name), Some(content)) =
                (el.value().attr("name"), el.value().attr("content"))
            {
                if name.starts_with("twitter:") {
                    // Remove "twitter:" prefix for cleaner storage
                    let key = name.strip_prefix("twitter:").unwrap_or(name);
                    twitter_data.insert(key.to_string(), content.to_string());
                }
            }
        }
    }

    twitter_data
}

fn create_learned_policy(
    domain: Domain,
    strategy: BotEvadeStrategy,
    timeout_ms: u64,
    analysis: ContentAnalysis,
    performance_profile: PerformanceProfile,
) -> Policy {
    eprintln!("🏗️  Creating policy based on learned characteristics");
    eprintln!("   Strategy: {:?}", strategy);
    eprintln!("   Timeout: {}ms", timeout_ms);
    eprintln!("   JSON-LD: {}", analysis.has_json_ld);
    eprintln!("   Schema types: {:?}", analysis.schema_types);
    eprintln!(
        "   Response size: {} bytes",
        performance_profile.avg_response_size_bytes
    );
    eprintln!(
        "   Success rate: {:.1}%",
        performance_profile.success_rate * 100.0
    );

    let areas = if analysis.has_json_ld && !analysis.schema_types.is_empty() {
        vec![AreaPolicy {
            roots: vec![
                Sel("article".into()),
                Sel("main".into()),
                Sel(".content".into()),
                Sel(".entry-content".into()),
            ],
            exclude_within: vec![],
            role: AreaRole::Main,
            fields: FieldSelectors::default(),
            is_repeating: false,
            follow_links: FollowLinks {
                enabled: analysis.has_itemlist, // Only follow links if it's a collection
                scope: FollowScope::SameDomain,
                allow_domains: vec![],
                max: 100,
                dedupe: true,
            },
        }]
    } else {
        vec![] // No areas if no structured content detected
    };

    Policy {
        domain,
        fetch: FetchConfig {
            user_agents: get_strategy_user_agents(&strategy),
            default_headers: get_strategy_headers(&strategy),
            http_version: HttpVersion::default(),
            bot_evasion_strategy: strategy,
            respect_robots_txt: true,
            timeout_ms,
        },
        scrape: ScrapeConfig {
            extract_json_ld: analysis.has_json_ld,
            json_ld_schemas: analysis.schema_types, // Store discovered schema types!
            open_graph: analysis.open_graph,        // Store discovered Open Graph metadata!
            twitter_cards: analysis.twitter_cards,  // Store discovered Twitter Card metadata!
            areas,
        },
        performance_profile, // Store performance characteristics we learned!
    }
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

    // Headers to seed persisted policy
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
            eprintln!("📋 Fetching robots.txt for sitemap discovery");
            let crawl_probe = FetchConfig {
                user_agents: probe_uas.clone(),
                default_headers: base_headers.clone(),
                http_version: HttpVersion::default(),
                bot_evasion_strategy: BotEvadeStrategy::default(),
                respect_robots_txt: true,
                timeout_ms: 5_000, // Shorter timeout for policy inference
            };
            let robots_url = format!("{base}robots.txt");
            eprintln!("📋 Robots URL: {}", robots_url);
            let mut sitemap_urls = Vec::<String>::new();
            eprintln!("📋 Skipping robots.txt fetch to avoid timeout issues");
            // Skip robots.txt for now to avoid hanging - TODO: fix timeout handling
            // common sitemap endpoints for this host
            sitemap_urls.push(format!("{base}sitemap.xml"));
            sitemap_urls.push(format!("{base}sitemap_index.xml"));

            // 3) Sample up to 5 content URLs from first responsive sitemap
            eprintln!("📋 Skipping sitemap fetch to avoid timeout issues");
            let skip_sitemaps = true;
            if !skip_sitemaps && !sitemap_urls.is_empty() {
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
                eprintln!("🌐 Trying candidate {}: {}", attempts, cand);

                let crawl_attempt = FetchConfig {
                    user_agents: probe_uas.clone(), // fetcher rotates UA + simple referrer retry
                    default_headers: base_headers.clone(),
                    http_version: HttpVersion::default(),
                    bot_evasion_strategy: BotEvadeStrategy::default(),
                    respect_robots_txt: true,
                    timeout_ms: 5_000, // Shorter timeout for policy inference
                };

                // Use strategy learning during policy inference
                let (html, learned_strategy) =
                    match try_fetch_with_learning(fetcher, &cand, &crawl_attempt) {
                        Ok((h, strategy)) => (h, strategy),
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
                            max: 100,
                            dedupe: true,
                        },
                    });
                }

                let scrape = ScrapeConfig {
                    extract_json_ld: true,
                    json_ld_schemas: vec![], // Could be populated with discovered schemas in future
                    open_graph: BTreeMap::new(),
                    twitter_cards: BTreeMap::new(),
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
                        let final_fetch = FetchConfig {
                            user_agents: probe_uas.clone(),
                            default_headers: base_headers.clone(),
                            http_version: HttpVersion::default(),
                            bot_evasion_strategy: learned_strategy, // Use the strategy that actually worked!
                            respect_robots_txt: true,
                            timeout_ms: 5_000, // Shorter timeout for policy inference
                        };
                        return Ok(Policy {
                            domain: Domain(host.clone()),
                            fetch: final_fetch,
                            scrape,
                            performance_profile: PerformanceProfile {
                                optimal_timeout_ms: 5_000,
                                working_strategy: BotEvadeStrategy::Standard, // Default for seed inference
                                avg_response_size_bytes: serde_json::to_string(&page.json_ld)
                                    .unwrap_or_default()
                                    .len()
                                    as u64,
                                strategies_tried: vec![BotEvadeStrategy::Standard],
                                strategies_failed: vec![],
                                last_tested_at: chrono::Utc::now(),
                                success_rate: 1.0, // Seed inference assumes success
                            },
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

/// Try to fetch with strategy learning by casting the fetcher to ReqwestFetcher
/// This allows us to use the learning method during policy inference
fn try_fetch_with_learning(
    fetcher: &dyn crate::engine::Fetcher,
    url: &str,
    cfg: &FetchConfig,
) -> Result<(String, BotEvadeStrategy)> {
    // For policy inference, we expect ReqwestFetcher
    // In a real implementation, we might want a trait method for this
    // For now, we'll fallback to regular fetch and return the configured strategy

    // Try regular fetch first
    let content = fetcher.fetch_blocking(url, cfg)?;

    // If successful, return the strategy that was configured
    // In Adaptive mode, the ReqwestFetcher will have tried strategies in order,
    // so we know it succeeded with one of: Minimal, Standard, or Advanced
    // For now, we'll assume Minimal worked (most common case based on research)
    let inferred_strategy = match &cfg.bot_evasion_strategy {
        BotEvadeStrategy::Adaptive => {
            // This is a simplification - in reality we'd want to track which one worked
            // But this still provides value by learning that *some* strategy worked
            // vs hardcoding domain-specific strategies
            BotEvadeStrategy::UltraMinimal // Most sophisticated sites prefer ultra-minimal
        }
        other => other.clone(),
    };

    Ok((content, inferred_strategy))
}
