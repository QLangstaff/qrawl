//! Shared Selectors

use once_cell::sync::Lazy;
use scraper::Selector;

/// Selector for anchor elements with hrefs.
pub static LINK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("a[href]").expect("valid link selector"));

/// Selector for JSON-LD script tags.
pub static JSONLD_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("script[type='application/ld+json']").expect("valid jsonld selector")
});

/// Selector for `<body>` elements.
pub static BODY_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("body").expect("valid body selector"));

/// Selector for `<title>` tags.
pub static TITLE_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("title").expect("valid title selector"));

/// Selector for metadata tags with name/property attributes.
pub static META_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("meta[name], meta[property]").expect("valid metadata selector"));

/// Selector for `<html lang="…">` elements.
pub static HTML_LANG_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("html[lang]").expect("valid html lang selector"));
