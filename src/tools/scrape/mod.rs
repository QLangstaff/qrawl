//! Scrape Tools

use crate::types::{Embed, Html, Jsonld, Metadata};

mod embed;
mod tests;
mod utils;

/// Scrape the page body as [`Html`] — the `<body>` subtree. A scrape leaf: it
/// yields HTML, so it composes into [`transform_markdown`](crate::tools::transform::transform_markdown)
/// for LLM-ready Markdown.
pub async fn scrape_body(html: &Html) -> Html {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        utils::scrape_body_from_doc(&scraper::Html::parse_document(&html))
    })
    .await
    .expect("scrape_body: spawn_blocking failed")
}

/// Scrape all of a page's schema.org structured data as one [`Jsonld`]: native
/// `<script type="application/ld+json">`, Microdata (`itemscope`/`itemtype`/
/// `itemprop`), RDFa (RDFa Lite: `typeof`/`property`/`resource`), and
/// Microformats2 (`h-*`) normalized to schema.org — all flattened to the same
/// shape (`@type` is the short name), in a single parse.
///
/// The encodings are **unified**: an entity encoded twice (same `@type` +
/// `name`) folds into one, filling only missing props, rather than emitting a
/// partial duplicate — so a page that carries a Recipe as both Microdata (name +
/// ingredients) and an h-recipe microformat (with steps) surfaces one complete
/// Recipe. `itemref` is not supported; RDFa emits only `typeof`-rooted resources
/// (page-level RDFa `<meta>` is left to [`scrape_metadata`]).
pub async fn scrape_jsonld(html: &Html) -> Jsonld {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        utils::scrape_jsonld_from_doc(&scraper::Html::parse_document(&html))
    })
    .await
    .expect("scrape_jsonld: spawn_blocking failed")
}

/// Scrape metadata tags from HTML.
pub async fn scrape_metadata(html: &Html) -> Metadata {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        utils::scrape_metadata_from_doc(&scraper::Html::parse_document(&html))
    })
    .await
    .expect("scrape_metadata: spawn_blocking failed")
}

/// Scrape everything from a single parse: body [`Html`], `<meta>` tags,
/// and the unified schema.org view ([`scrape_jsonld`]) — returned as
/// `(body, metadata, schema)`. Prefer this over calling `scrape_body` +
/// `scrape_metadata` + `scrape_jsonld` separately (which would parse three
/// times).
pub async fn scrape_all(html: &Html) -> (Html, Metadata, Jsonld) {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        utils::scrape_from_doc(&scraper::Html::parse_document(&html))
    })
    .await
    .expect("scrape_all: spawn_blocking failed")
}

/// Scrape a fetched [`Embed`] (`fetch_embed`) into metadata pairs, keyed
/// `oembed:title` / `oembed:author_name` / `oembed:thumbnail_url` and
/// `reddit:title` / `reddit:text` / `reddit:author` / `reddit:image` /
/// `reddit:video` / `reddit:url`. `url` is the item the embed was fetched
/// for: a Reddit comment URL selects that comment. Empty when the embed
/// doesn't parse.
pub fn scrape_embed(embed: &Embed, url: &str) -> Metadata {
    embed::scrape_embed(embed, url)
}
