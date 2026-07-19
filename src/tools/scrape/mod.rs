//! Scrape Tools

use crate::types::{Html, Jsonld, Metadata, Microformats};

mod tests;
mod utils;

/// Scrape body content from HTML.
pub async fn scrape_body(html: &Html) -> String {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_body_content(&html))
        .await
        .expect("scrape_body: spawn_blocking failed")
}

/// Scrape JSON-LD scripts from HTML.
pub async fn scrape_jsonld(html: &Html) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_jsonld_scripts(&html))
        .await
        .expect("scrape_jsonld: spawn_blocking failed")
}

/// Scrape metadata tags from HTML.
pub async fn scrape_metadata(html: &Html) -> Metadata {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_metadata_tags(&html))
        .await
        .expect("scrape_metadata: spawn_blocking failed")
}

/// Scrape Microdata (`itemscope`/`itemtype`/`itemprop`) from HTML.
///
/// Returns the same flattened, JSON-LD-shaped [`Jsonld`] that [`scrape_jsonld`]
/// produces — `@type` is the short schema.org name — so the result flows
/// through [`extract_schema_types`](crate::tools::extract::extract_schema_types)
/// and other JSON-LD consumers unchanged. `itemref` is not supported.
pub async fn scrape_microdata(html: &Html) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_microdata_items(&html))
        .await
        .expect("scrape_microdata: spawn_blocking failed")
}

/// Scrape Microformats2 (`h-*` roots; `p-*`/`u-*`/`dt-*`/`e-*` properties).
///
/// Returns canonical mf2 items (`{type, properties, children}`) as
/// [`Microformats`] — the raw mf2 vocabulary, keyed on `type` (not `@type`). To
/// consume it as schema.org, normalize via
/// [`extract_microformats_schema`](crate::tools::extract::extract_microformats_schema),
/// or [`extract_schema`](crate::tools::extract::extract_schema) to get it merged
/// with the native encodings. mf1 backcompat and implied properties are not yet
/// supported.
pub async fn scrape_microformats(html: &Html) -> Microformats {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_microformats_items(&html))
        .await
        .expect("scrape_microformats: spawn_blocking failed")
}

/// Scrape RDFa (RDFa Lite: `typeof`/`property`/`resource`) from HTML.
///
/// Returns the same flattened, JSON-LD-shaped [`Jsonld`] as [`scrape_jsonld`] /
/// [`scrape_microdata`], so it flows through
/// [`extract_schema_types`](crate::tools::extract::extract_schema_types)
/// unchanged. Only `typeof`-rooted resources are emitted — page-level RDFa
/// properties (Open Graph `<meta>`) are left to [`scrape_metadata`]. `rel`/`rev`
/// and `resource`/`href` subject chaining are not interpreted.
pub async fn scrape_rdfa(html: &Html) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || utils::scrape_rdfa_items(&html))
        .await
        .expect("scrape_rdfa: spawn_blocking failed")
}

/// Scrape the *native* schema.org structured data — JSON-LD, Microdata, and
/// RDFa — in a single parse, returned as one [`Jsonld`].
///
/// These three script/attribute encodings carry schema.org natively (`@type` is
/// the short name), so the union flows through
/// [`extract_schema_types`](crate::tools::extract::extract_schema_types)
/// unchanged. Items are ordered JSON-LD, Microdata, then RDFa.
///
/// Microformats2 is a *distinct* vocabulary and is deliberately **not** folded
/// in here — unifying it (normalized to schema.org, with cross-encoding dedup)
/// is the job of [`extract_schema`](crate::tools::extract::extract_schema), the
/// merged view every schema consumer shares. Keeping this primitive isolated is
/// what lets it stay a single-parse building block (`extract` → `scrape`, never
/// the reverse).
pub async fn scrape_structured(html: &Html) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        let document = scraper::Html::parse_document(&html);
        let mut items = utils::scrape_jsonld_from_doc(&document);
        items.extend(utils::scrape_microdata_from_doc(&document));
        items.extend(utils::scrape_rdfa_from_doc(&document));
        items
    })
    .await
    .expect("scrape_structured: spawn_blocking failed")
}

/// Scrape both JSON-LD and metadata in a single parse.
///
/// Equivalent to `tokio::join!(scrape_jsonld(html), scrape_metadata(html))` but
/// parses the HTML once instead of twice. Prefer this when you need both.
pub async fn scrape_all(html: &Html) -> (Jsonld, Metadata) {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        let document = scraper::Html::parse_document(&html);
        (
            utils::scrape_jsonld_from_doc(&document),
            utils::scrape_metadata_from_doc(&document),
        )
    })
    .await
    .expect("scrape_all: spawn_blocking failed")
}
