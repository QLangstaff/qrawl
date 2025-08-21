use crate::{engine::{Fetcher, Scraper}, types::*, error::*};
use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use scraper::{ElementRef, Html, Selector};
use url::Url;

/* ------------------------- Fetcher (blocking) ------------------------- */

pub struct ReqwestFetcher {
    client: Client,
}
impl ReqwestFetcher {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .cookie_store(true)
            .build()
            .map_err(|e| QrawlError::Other(e.to_string()))?;
        Ok(Self { client })
    }
    fn build_headers(&self, cfg: &CrawlConfig) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (k, v) in cfg.default_headers.0.iter() {
            if let (Ok(hk), Ok(hv)) = (HeaderName::try_from(k.as_str()), HeaderValue::try_from(v.as_str())) {
                let _ = headers.insert(hk, hv);
            }
        }
        if let Some(ua) = cfg.user_agents.get(0) {
            if let Ok(hv) = HeaderValue::try_from(ua.as_str()) {
                let _ = headers.insert("User-Agent", hv);
            }
        }
        headers
    }
}
impl Fetcher for ReqwestFetcher {
    fn name(&self) -> &'static str { "reqwest-blocking" }
    fn fetch_blocking(&self, url: &str, cfg: &CrawlConfig) -> Result<String> {
        let headers = self.build_headers(cfg);
        let resp = self.client
            .get(url)
            .headers(headers)
            .send()
            .map_err(|e| QrawlError::Other(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(QrawlError::Other(format!("http status {} for {}", resp.status(), url)));
        }
        resp.text().map_err(|e| QrawlError::Other(e.to_string()))
    }
}

/* ------------------------- Scraper (areas) ------------------------- */

fn compile(list: &[Sel]) -> Vec<Selector> {
    list.iter().filter_map(|s| Selector::parse(&s.0).ok()).collect()
}

/// Return true if `el` is inside any subtree selected by any `exclude` selector under `root`.
fn is_excluded(el: &ElementRef, root: &ElementRef, exclude: &[Selector]) -> bool {
    // For each exclude selector, select matching roots under `root` and
    // check if any ancestor of `el` matches one of those roots.
    for sel in exclude {
        for ex_root in root.select(sel) {
            let ex_id = ex_root.id(); // private NodeId type is fine as a local
            for anc in el.ancestors() {
                if let Some(ael) = ElementRef::wrap(anc) {
                    if ael.id() == ex_id { return true; }
                }
            }
        }
    }
    false
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

fn resolve(base: &Url, href: &str) -> Option<String> {
    Url::options().base_url(Some(base)).parse(href).ok().map(|u| u.to_string())
}

pub struct DefaultScraper;
impl Scraper for DefaultScraper {
    fn name(&self) -> &'static str { "default-scraper" }

    fn scrape(&self, url: &str, html: &str, cfg: &ScrapeConfig) -> Result<PageExtraction> {
        let doc = Html::parse_document(html);
        let base = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;

        let mut areas_out: Vec<AreaContent> = Vec::new();

        for area in &cfg.areas {
            let roots = compile(&area.roots);
            let exclude = compile(&area.exclude_within);
            let s_title = compile(&area.fields.title);
            let s_head = compile(&area.fields.headings);
            let s_p = compile(&area.fields.paragraphs);
            let s_img = compile(&area.fields.images);
            let s_a = compile(&area.fields.links);
            let s_lists = compile(&area.fields.lists);
            let s_tables = compile(&area.fields.tables);

            let mut matched_any = false;
            for rsel in &roots {
                for root in doc.select(rsel) {
                    matched_any = true;

                    let mut out = AreaContent {
                        role: area.role,
                        root_selector_matched: format!("{:?}", rsel),
                        ..Default::default()
                    };
                    // title
                    for s in &s_title {
                        if let Some(el) = root.select(s).find(|el| !is_excluded(el, &root, &exclude)) {
                            let t = text_of(&el);
                            if !t.is_empty() { out.title = Some(t); break; }
                        }
                    }
                    // headings
                    for s in &s_head {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            let t = text_of(&el);
                            if !t.is_empty() { out.headings.push(t); }
                        }
                    }
                    // paragraphs
                    for s in &s_p {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            let t = text_of(&el);
                            if !t.is_empty() { out.paragraphs.push(t); }
                        }
                    }
                    // images
                    for s in &s_img {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            if let Some(src) = el.value().attr("src") {
                                let src_abs = resolve(&base, src).unwrap_or_else(|| src.to_string());
                                let alt = el.value().attr("alt").map(|s| s.to_string());
                                out.images.push(ImageOut { src: src_abs, alt });
                            }
                        }
                    }
                    // links
                    for s in &s_a {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            if let Some(href) = el.value().attr("href") {
                                let href_abs = resolve(&base, href).unwrap_or_else(|| href.to_string());
                                let text = text_of(&el);
                                out.links.push(LinkOut { href: href_abs, text });
                            }
                        }
                    }
                    // lists
                    for s in &s_lists {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            // collect li text
                            if let Ok(li_sel) = Selector::parse("li") {
                                let mut list_items = vec![];
                                for li in el.select(&li_sel) {
                                    let t = text_of(&li);
                                    if !t.is_empty() { list_items.push(t); }
                                }
                                if !list_items.is_empty() { out.lists.push(list_items); }
                            }
                        }
                    }
                    // tables
                    for s in &s_tables {
                        for el in root.select(s) {
                            if is_excluded(&el, &root, &exclude) { continue; }
                            let tr_sel = Selector::parse("tr").unwrap();
                            let td_sel = Selector::parse("th, td").unwrap();
                            let mut rows = vec![];
                            for tr in el.select(&tr_sel) {
                                let mut row = vec![];
                                for td in tr.select(&td_sel) {
                                    let t = text_of(&td);
                                    if !t.is_empty() { row.push(t); }
                                }
                                if !row.is_empty() { rows.push(row); }
                            }
                            if !rows.is_empty() { out.tables.push(rows); }
                        }
                    }

                    areas_out.push(out);
                    if !area.is_repeating { break; }
                }
                if matched_any && !area.is_repeating { break; }
            }
        }

        // JSON-LD
        let mut json_ld = vec![];
        if cfg.extract_json_ld {
            if let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) {
                for el in doc.select(&sel) {
                    let txt = el.text().collect::<String>();
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                        if val.is_object() { json_ld.push(val) }
                        else if let Some(arr) = val.as_array() { json_ld.extend(arr.clone()); }
                    }
                }
            }
        }

        let domain = base.domain().unwrap_or_default().to_string();
        Ok(PageExtraction {
            url: url.to_string(),
            domain,
            areas: areas_out,
            json_ld,
            fetched_at: Utc::now(),
        })
    }
}
