use crate::{error::*, store::PolicyStore, types::*};
use std::collections::HashSet;
use url::Url;

/* ---------- Traits that impls.rs provides ---------- */

pub trait Fetcher: Send + Sync {
    fn fetch_blocking(&self, url: &str, cfg: &CrawlConfig) -> crate::Result<String>;
    /// Optional; concrete impls (like reqwest) can override.
    fn name(&self) -> &'static str {
        "fetcher"
    }
}

pub trait Scraper: Send + Sync {
    fn scrape(&self, url: &str, html: &str, cfg: &ScrapeConfig) -> crate::Result<PageExtraction>;
    /// Optional; concrete impls can override.
    fn name(&self) -> &'static str {
        "scraper"
    }
}

/* ---------- Engine options ---------- */

#[derive(Default, Clone, Copy)]
pub struct EngineOptions {
    pub max_children: usize,
}

/* ---------- Engine ---------- */

pub struct Engine<'a, PS: PolicyStore> {
    pub store: &'a PS,
    pub fetcher: &'a dyn Fetcher,
    pub scraper: &'a dyn Scraper,
    pub opts: EngineOptions,
}

impl<'a, PS: PolicyStore> Engine<'a, PS> {
    pub fn new(
        store: &'a PS,
        fetcher: &'a dyn Fetcher,
        scraper: &'a dyn Scraper,
        opts: EngineOptions,
    ) -> Self {
        Self {
            store,
            fetcher,
            scraper,
            opts,
        }
    }

    pub fn extract_known(&self, url: &str) -> Result<ExtractionBundle> {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let domain = Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?;
        let pol = self
            .store
            .get(&domain)?
            .ok_or_else(|| QrawlError::Other(format!("no policy for domain {}", domain.0)))?;

        let html = self.fetcher.fetch_blocking(url, &pol.crawl)?;
        let parent = self.scraper.scrape(url, &html, &pol.scrape)?;
        let children = self.follow_itemlist_children(url, &parent, &pol)?;

        Ok(ExtractionBundle { parent, children })
    }

    pub fn extract_unknown(&self, url: &str) -> Result<ExtractionBundle> {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let domain = Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?;

        let pol = transient_unknown_policy(domain);

        let html = self.fetcher.fetch_blocking(url, &pol.crawl)?;
        let parent = self.scraper.scrape(url, &html, &pol.scrape)?;
        let children = self.follow_itemlist_children(url, &parent, &pol)?;

        Ok(ExtractionBundle { parent, children })
    }

    /// Follow children when:
    ///  - policy has follow_links.enabled, OR
    ///  - parent JSON-LD contains ItemList (auto-enable with sane defaults)
    fn follow_itemlist_children(
        &self,
        base_url: &str,
        parent: &PageExtraction,
        pol: &Policy,
    ) -> Result<Vec<PageExtraction>> {
        // Determine effective follow config:
        // 1) find first area with follow_links.enabled
        let maybe_cfg = pol
            .scrape
            .areas
            .iter()
            .find(|a| a.follow_links.enabled)
            .map(|a| a.follow_links.clone());

        // 2) If not explicitly enabled, auto-enable if parent has ItemList
        let (enabled, scope, allow_domains, max, dedupe) = if let Some(cfg) = maybe_cfg {
            (
                true,
                cfg.scope,
                cfg.allow_domains.clone(),
                cfg.max,
                cfg.dedupe,
            )
        } else if parent_has_itemlist(&parent.json_ld) {
            // Auto defaults when ItemList is present
            (true, FollowScope::SameDomain, Vec::new(), 10, true)
        } else {
            (false, FollowScope::SameDomain, Vec::new(), 0, true)
        };

        if !enabled || max == 0 {
            return Ok(vec![]);
        }

        let base = Url::parse(base_url).map_err(|_| QrawlError::InvalidUrl(base_url.into()))?;
        let parent_domain = base.domain().unwrap_or("");

        // Extract candidate URLs from JSON-LD ItemList
        let mut links = extract_itemlist_urls(&parent.json_ld, &base);

        // Scope filtering
        links.retain(|u| match scope {
            FollowScope::SameDomain => Url::parse(u)
                .ok()
                .and_then(|uu| uu.domain().map(|d| d == parent_domain))
                .unwrap_or(false),
            FollowScope::AnyDomain => true,
            FollowScope::AllowList => {
                let dom = Url::parse(u)
                    .ok()
                    .and_then(|uu| uu.domain().map(|d| d.to_string()));
                match dom {
                    Some(d) => allow_domains.iter().any(|ad| ad.eq_ignore_ascii_case(&d)),
                    None => false,
                }
            }
        });

        // Dedupe + limit
        if dedupe {
            let mut seen = HashSet::<String>::new();
            links.retain(|u| seen.insert(u.clone()));
        }
        if links.len() as u32 > max {
            links.truncate(max as usize);
        }

        // Fetch + scrape children
        let mut out = Vec::new();
        for child in links {
            if let Ok(html) = self.fetcher.fetch_blocking(&child, &pol.crawl) {
                if let Ok(page) = self.scraper.scrape(&child, &html, &pol.scrape) {
                    out.push(page);
                }
            }
        }
        Ok(out)
    }
}

/* ---------- Transient unknown-policy (NOT saved) ---------- */

fn transient_unknown_policy(domain: Domain) -> Policy {
    // Schema-first unknown policy: JSON-LD only, no CSS selectors, no follow by default
    Policy {
        domain,
        crawl: CrawlConfig {
            user_agents: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".into(),
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".into(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:124.0) Gecko/20100101 Firefox/124.0".into(),
            ],
            default_headers: HeaderSet(
                [
                    ("Accept".into(), "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into()),
                    ("Accept-Encoding".into(), "gzip, deflate, br".into()),
                    ("Accept-Language".into(), "en-US,en;q=0.9".into()),
                    ("Connection".into(), "keep-alive".into()),
                ].into_iter().collect()
            ),
            respect_robots_txt: true,
            timeout_ms: 20_000,
        },
        scrape: ScrapeConfig {
            extract_json_ld: true,
            areas: vec![], // kept empty on unknown to stay conservative
        },
    }
}

/* ---------- helpers ---------- */

fn parent_has_itemlist(json_ld: &Vec<serde_json::Value>) -> bool {
    for v in json_ld {
        if contains_itemlist(v) {
            return true;
        }
    }
    false
}

fn extract_itemlist_urls(json_ld: &Vec<serde_json::Value>, base: &Url) -> Vec<String> {
    let mut out = Vec::<String>::new();
    for v in json_ld {
        collect_itemlist_urls(v, base, &mut out);
    }
    out
}

fn collect_itemlist_urls(v: &serde_json::Value, base: &Url, out: &mut Vec<String>) {
    use serde_json::Value::*;
    match v {
        Array(arr) => {
            for it in arr {
                collect_itemlist_urls(it, base, out);
            }
        }
        Object(map) => {
            // Check @type quickly
            if let Some(t) = map.get("@type") {
                if type_is_itemlist(t) {
                    // read itemListElement
                    if let Some(ele) = map.get("itemListElement") {
                        collect_elements(ele, base, out);
                    }
                }
            }
            // Also accept objects that have itemListElement without @type
            if let Some(ele) = map.get("itemListElement") {
                collect_elements(ele, base, out);
            }
            // Dive into @graph and other nested fields
            if let Some(graph) = map.get("@graph") {
                collect_itemlist_urls(graph, base, out);
            }
            for (_k, vv) in map {
                collect_itemlist_urls(vv, base, out);
            }
        }
        _ => {}
    }
}

fn contains_itemlist(v: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    match v {
        Array(arr) => arr.iter().any(contains_itemlist),
        Object(map) => {
            if let Some(t) = map.get("@type") {
                if type_is_itemlist(t) {
                    return true;
                }
            }
            if map.contains_key("itemListElement") {
                return true;
            }
            if let Some(graph) = map.get("@graph") {
                if contains_itemlist(graph) {
                    return true;
                }
            }
            // conservative nested search
            map.values().any(contains_itemlist)
        }
        _ => false,
    }
}

fn type_is_itemlist(t: &serde_json::Value) -> bool {
    match t {
        serde_json::Value::String(s) => s.eq_ignore_ascii_case("ItemList"),
        serde_json::Value::Array(arr) => arr.iter().any(|v| {
            v.as_str()
                .map(|s| s.eq_ignore_ascii_case("ItemList"))
                .unwrap_or(false)
        }),
        _ => false,
    }
}

fn collect_elements(ele: &serde_json::Value, base: &Url, out: &mut Vec<String>) {
    if let Some(arr) = ele.as_array() {
        for item in arr {
            collect_one_element(item, base, out);
        }
    } else {
        collect_one_element(ele, base, out);
    }
}

fn collect_one_element(item: &serde_json::Value, base: &Url, out: &mut Vec<String>) {
    // ItemList can contain either Things or ListItems
    // - When ListItem: { "url": "..."} OR { "item": {"@id"|"url": "..."} }
    // - When Thing: {"@id"|"url": "..."}
    if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
        if let Some(abs) = absolutize(base, u) {
            out.push(abs);
        }
        return;
    }
    if let Some(it) = item.get("item") {
        if let Some(u) = it
            .get("url")
            .and_then(|v| v.as_str())
            .or_else(|| it.get("@id").and_then(|v| v.as_str()))
        {
            if let Some(abs) = absolutize(base, u) {
                out.push(abs);
            }
            return;
        }
    }
    if let Some(u) = item.get("@id").and_then(|v| v.as_str()) {
        if let Some(abs) = absolutize(base, u) {
            out.push(abs);
        }
    }
}

fn absolutize(base: &Url, link: &str) -> Option<String> {
    if let Ok(u) = Url::parse(link) {
        return Some(u.to_string());
    }
    base.join(link).ok().map(|u| u.to_string())
}
