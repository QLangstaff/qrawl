use crate::{engine::Scraper as ScraperT, types::*};
use scraper::{ElementRef, Html, Selector};
use url::Url;

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
    // Use a universal selector to get all elements in document order
    if let Ok(all_selector) = Selector::parse("*") {
        for el in root.select(&all_selector) {
            let tag_name = el.value().name();

            match tag_name {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
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
