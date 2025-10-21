use scraper::{Html, Selector};
use serde_json::Value;

use crate::tools::types::{Jsonld, Metadata};

pub(super) fn scrape_body_content(html: &str) -> String {
    let document = Html::parse_document(html);

    if let Ok(selector) = Selector::parse("body") {
        if let Some(body) = document.select(&selector).next() {
            return body.html();
        }
    }

    html.to_string()
}

pub(super) fn scrape_jsonld_scripts(html: &str) -> Jsonld {
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

fn flatten_jsonld(value: Value) -> Vec<Value> {
    match value {
        Value::Array(arr) => arr.into_iter().flat_map(flatten_jsonld).collect(),
        Value::Object(mut obj) => {
            if let Some(Value::Array(arr)) = obj.remove("@graph") {
                return arr.into_iter().flat_map(flatten_jsonld).collect();
            }
            vec![Value::Object(obj)]
        }
        _ => Vec::new(),
    }
}

pub(super) fn scrape_metadata_tags(html: &str) -> Metadata {
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
