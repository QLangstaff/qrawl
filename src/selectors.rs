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

/// Selector for Microdata items (`itemscope` elements). Top-level items are
/// filtered in code (an `itemscope` that also has `itemprop` is a nested item).
pub static MICRODATA_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("[itemscope]").expect("valid microdata selector"));

/// Selector for RDFa typed resources (`typeof` elements). Top-level items are
/// filtered in code (a `typeof` that also has `property` is a nested resource).
pub static RDFA_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("[typeof]").expect("valid rdfa selector"));

/// Selector for any element with a `class` attribute. Microformats are
/// class-based (`h-*` roots, `p-*`/`u-*`/`dt-*`/`e-*` properties), so candidates
/// are matched broadly here and filtered by class token in code.
pub static CLASS_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("[class]").expect("valid class selector"));

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

/// Selector for `<li>` elements (mf2 `e-*` step splitting).
pub static LI_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("li").expect("valid li selector"));

/// Selector for `<p>` elements (mf2 `e-*` step splitting).
pub static P_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse("p").expect("valid p selector"));
