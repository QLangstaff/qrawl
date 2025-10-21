use scraper::Html;
use serde_json::Value;

use crate::selectors::{
    BODY_SELECTOR, HTML_LANG_SELECTOR, JSONLD_SELECTOR, META_SELECTOR, TITLE_SELECTOR,
};
use crate::types::{Jsonld, Metadata};

pub(super) fn scrape_body_content(html: &str) -> String {
    let document = Html::parse_document(html);

    if let Some(body) = document.select(&BODY_SELECTOR).next() {
        return body.html();
    }

    html.to_string()
}

pub(super) fn scrape_jsonld_scripts(html: &str) -> Jsonld {
    let document = Html::parse_document(html);

    document
        .select(&JSONLD_SELECTOR)
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

    if let Some(el) = document.select(&TITLE_SELECTOR).next() {
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            tags.push(("title".to_string(), text));
        }
    }

    for el in document.select(&META_SELECTOR) {
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

    if let Some(el) = document.select(&HTML_LANG_SELECTOR).next() {
        if let Some(lang) = el.value().attr("lang") {
            tags.push(("lang".to_string(), lang.to_string()));
        }
    }

    tags
}
