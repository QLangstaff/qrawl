use crate::tools::types::{Jsonld, Metadata};
use scraper::{Html, Selector};

pub fn scrape_body_content(html: &str) -> String {
    let document = Html::parse_document(html);

    if let Ok(selector) = Selector::parse("body") {
        if let Some(body) = document.select(&selector).next() {
            return body.html();
        }
    }

    html.to_string()
}

pub fn scrape_jsonld_scripts(html: &str) -> Jsonld {
    let document = Html::parse_document(html);
    let selector = match Selector::parse("script[type='application/ld+json']") {
        Ok(sel) => sel,
        Err(_) => return Vec::new(),
    };

    document
        .select(&selector)
        .filter_map(|el| {
            let raw = el.text().collect::<String>();
            serde_json::from_str(&raw).ok()
        })
        .flat_map(flatten_jsonld)
        .collect()
}

fn flatten_jsonld(value: serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => arr.into_iter().flat_map(flatten_jsonld).collect(),
        serde_json::Value::Object(_) => vec![value],
        _ => Vec::new(),
    }
}

pub fn scrape_metadata_tags(html: &str) -> Metadata {
    let document = Html::parse_document(html);
    let mut tags = Vec::new();

    if let Ok(selector) = Selector::parse("title") {
        if let Some(el) = document.select(&selector).next() {
            let text = el.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                tags.push(("title".to_string(), text));
            }
        }
    }

    if let Ok(selector) = Selector::parse("meta[name], meta[property]") {
        for el in document.select(&selector) {
            let key = el
                .value()
                .attr("name")
                .or_else(|| el.value().attr("property"))
                .map(|s| s.to_string());
            let value = el.value().attr("content").map(|s| s.to_string());

            if let (Some(k), Some(v)) = (key, value) {
                if !v.trim().is_empty() {
                    tags.push((k, v));
                }
            }
        }
    }

    if let Ok(selector) = Selector::parse("html[lang]") {
        if let Some(el) = document.select(&selector).next() {
            if let Some(lang) = el.value().attr("lang") {
                tags.push(("lang".to_string(), lang.to_string()));
            }
        }
    }

    tags
}
