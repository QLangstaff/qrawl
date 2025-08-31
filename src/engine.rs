use crate::{services::PolicyStore, types::*};
use async_trait::async_trait;
use std::collections::HashSet;
use url::Url;

/* ---------- Traits that impls.rs provides ---------- */

#[async_trait]
pub trait Fetcher: Send + Sync {
    fn fetch_blocking(&self, url: &str, cfg: &FetchConfig) -> crate::Result<String>;

    /// Async variant of fetch_blocking. Must be implemented by concrete types.
    async fn fetch_async(&self, url: &str, cfg: &FetchConfig) -> crate::Result<String>;

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

#[derive(Clone, Copy, Default)]
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

    pub fn extract(&self, url: &str) -> Result<ExtractionBundle> {
        let domain = Domain::from_url(url)?;
        let pol = self.store.get(&domain)?.ok_or_else(|| {
            QrawlError::inference_error(&domain.0, "policy_lookup", "no policy found for domain")
        })?;

        let html = self.fetcher.fetch_blocking(url, &pol.fetch)?;
        let parent = self.scraper.scrape(url, &html, &pol.scrape)?;
        let children = self.follow_itemlist_children(url, &parent, &pol)?;

        Ok(ExtractionBundle { parent, children })
    }

    pub async fn extract_async(&self, url: &str) -> Result<ExtractionBundle> {
        let domain = Domain::from_url(url)?;
        let pol = self.store.get(&domain)?.ok_or_else(|| {
            QrawlError::inference_error(&domain.0, "policy_lookup", "no policy found for domain")
        })?;

        let html = self.fetcher.fetch_async(url, &pol.fetch).await?;
        let parent = self.scraper.scrape(url, &html, &pol.scrape)?;
        let children = self
            .follow_itemlist_children_async(url, &parent, &pol)
            .await?;

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

        let base = Url::parse(base_url).map_err(|_| {
            QrawlError::validation_error("url", &format!("invalid URL: {}", base_url))
        })?;
        let parent_domain = base.domain().unwrap_or("");

        // Extract candidate URLs from JSON-LD ItemList and content areas
        let mut links = extract_itemlist_urls(&parent.json_ld, &base);

        // Also extract links from content areas
        for area in &parent.areas {
            for block in &area.content {
                if let ContentBlock::Link { href, .. } = block {
                    // Resolve relative URLs to absolute
                    if let Ok(absolute_url) = base.join(href) {
                        links.push(absolute_url.to_string());
                    }
                }
            }
        }

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
            if let Ok(html) = self.fetcher.fetch_blocking(&child, &pol.fetch) {
                if let Ok(page) = self.scraper.scrape(&child, &html, &pol.scrape) {
                    out.push(page);
                }
            }
        }
        Ok(out)
    }

    async fn follow_itemlist_children_async(
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

        let base = Url::parse(base_url).map_err(|_| {
            QrawlError::validation_error("url", &format!("invalid URL: {}", base_url))
        })?;
        let parent_domain = base.domain().unwrap_or("");

        // Extract candidate URLs from JSON-LD ItemList and content areas
        let mut links = extract_itemlist_urls(&parent.json_ld, &base);

        // Also extract links from content areas
        for area in &parent.areas {
            for block in &area.content {
                if let ContentBlock::Link { href, .. } = block {
                    // Resolve relative URLs to absolute
                    if let Ok(absolute_url) = base.join(href) {
                        links.push(absolute_url.to_string());
                    }
                }
            }
        }

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

        // Fetch + scrape children asynchronously
        let mut out = Vec::new();
        for child in links {
            if let Ok(html) = self.fetcher.fetch_async(&child, &pol.fetch).await {
                if let Ok(page) = self.scraper.scrape(&child, &html, &pol.scrape) {
                    out.push(page);
                }
            }
        }
        Ok(out)
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
