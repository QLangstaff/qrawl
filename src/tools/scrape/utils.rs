use crate::selectors::{
    BODY_SELECTOR, CLASS_SELECTOR, HTML_LANG_SELECTOR, JSONLD_SELECTOR, LI_SELECTOR, META_SELECTOR,
    MICRODATA_SELECTOR, P_SELECTOR, RDFA_SELECTOR, TITLE_SELECTOR,
};
use crate::types::{Jsonld, Metadata, Microformats};

pub(super) fn scrape_body_from_doc(document: &scraper::Html) -> String {
    document
        .select(&BODY_SELECTOR)
        .next()
        .map(|body| body.html())
        .unwrap_or_else(|| document.html())
}

/// The unified schema.org view from a parsed doc: JSON-LD `<script>` tags +
/// Microdata + RDFa + Microformats2, with cross-encoding entities merged (same
/// `@type` + `name`).
pub(super) fn scrape_jsonld_from_doc(document: &scraper::Html) -> Jsonld {
    let mut items: Jsonld = document
        .select(&JSONLD_SELECTOR)
        .filter_map(|el| serde_json::from_str(&el.text().collect::<String>()).ok())
        .flat_map(flatten_jsonld)
        .collect();
    merge_schema_entities(&mut items, scrape_microdata_from_doc(document));
    merge_schema_entities(&mut items, scrape_rdfa_from_doc(document));
    merge_schema_entities(
        &mut items,
        microformats_to_schema(&scrape_microformats_from_doc(document)),
    );
    items
}

/// Everything from one parse: body HTML, metadata, and the unified schema view.
pub(super) fn scrape_from_doc(document: &scraper::Html) -> (String, Metadata, Jsonld) {
    (
        scrape_body_from_doc(document),
        scrape_metadata_from_doc(document),
        scrape_jsonld_from_doc(document),
    )
}

fn flatten_jsonld(value: serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => arr.into_iter().flat_map(flatten_jsonld).collect(),
        serde_json::Value::Object(mut obj) => {
            if let Some(serde_json::Value::Array(arr)) = obj.remove("@graph") {
                return arr.into_iter().flat_map(flatten_jsonld).collect();
            }
            vec![serde_json::Value::Object(obj)]
        }
        _ => Vec::new(),
    }
}

pub(super) fn scrape_metadata_from_doc(document: &scraper::Html) -> Metadata {
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

// ---------------------------------------------------------------------------
// Microdata
//
// Parses HTML Microdata (`itemscope`/`itemtype`/`itemprop`) into the same
// flattened, JSON-LD-shaped `serde_json::Value`s that `scrape_jsonld` emits, so the output
// flows through `extract_schema_types` and other consumers unchanged. `@type`
// is the short name from `itemtype` (e.g. `https://schema.org/Recipe` →
// `"Recipe"`). `itemref` is not supported. URLs are kept raw (no base-URL
// resolution — `scrape_*` don't receive the page URL).
// ---------------------------------------------------------------------------

pub(super) fn scrape_microdata_from_doc(document: &scraper::Html) -> Jsonld {
    document
        .select(&MICRODATA_SELECTOR)
        // Top-level items only: an `itemscope` element that also has `itemprop`
        // is a nested item (a property value of its enclosing item), reached by
        // recursion from that parent — not a top-level item.
        .filter(|el| el.value().attr("itemprop").is_none())
        .map(|el| microdata_item_to_value(&el))
        .collect()
}

/// Build a flattened, JSON-LD-shaped object from an `itemscope` element.
fn microdata_item_to_value(item: &scraper::ElementRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    // `@type` from `itemtype` (short names). Anonymous items (no `itemtype`)
    // simply get no `@type`, which `extract_schema_types` skips.
    if let Some(itemtype) = item.value().attr("itemtype") {
        let types: Vec<serde_json::Value> = itemtype
            .split_whitespace()
            .filter_map(short_type)
            .map(serde_json::Value::String)
            .collect();
        match types.len() {
            0 => {}
            1 => {
                obj.insert("@type".to_string(), types.into_iter().next().unwrap());
            }
            _ => {
                obj.insert("@type".to_string(), serde_json::Value::Array(types));
            }
        }
    }

    if let Some(itemid) = item.value().attr("itemid") {
        let itemid = itemid.trim();
        if !itemid.is_empty() {
            obj.insert(
                "@id".to_string(),
                serde_json::Value::String(itemid.to_string()),
            );
        }
    }

    let mut props = Vec::new();
    collect_properties(item, "itemscope", "itemprop", &mut props);
    for (el, names) in props {
        let value = if el.value().attr("itemscope").is_some() {
            microdata_item_to_value(&el) // nested item
        } else {
            microdata_prop_value(&el)
        };
        // `itemprop` may list several space-separated names; the value belongs
        // to each of them.
        for name in names.split_whitespace() {
            insert_prop(&mut obj, name, value.clone());
        }
    }

    serde_json::Value::Object(obj)
}

/// Collect `(element, property-name)` pairs belonging to `item`, walking
/// descendants but **never crossing into a nested item** (the `scope_attr`
/// boundary), whose properties belong to that nested item, not this one. Shared
/// by the Microdata (`itemscope`/`itemprop`) and RDFa (`typeof`/`property`) walks.
fn collect_properties<'a>(
    item: &scraper::ElementRef<'a>,
    scope_attr: &str,
    prop_attr: &str,
    out: &mut Vec<(scraper::ElementRef<'a>, String)>,
) {
    for child in item.children().filter_map(scraper::ElementRef::wrap) {
        let has_scope = child.value().attr(scope_attr).is_some();
        let prop = child.value().attr(prop_attr).map(str::to_string);

        if has_scope {
            // Nested item: record it as a property if named, but do NOT descend
            // — its inner properties belong to the nested item.
            if let Some(name) = prop {
                out.push((child, name));
            }
        } else {
            if let Some(name) = prop {
                out.push((child, name));
            }
            // A plain element can still contain more properties of THIS item.
            collect_properties(&child, scope_attr, prop_attr, out);
        }
    }
}

/// Insert a property, promoting to an array when the same name repeats.
fn insert_prop(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: serde_json::Value,
) {
    match obj.get_mut(name) {
        Some(serde_json::Value::Array(arr)) => arr.push(value),
        Some(existing) => {
            let prev = existing.take();
            *existing = serde_json::Value::Array(vec![prev, value]);
        }
        None => {
            obj.insert(name.to_string(), value);
        }
    }
}

/// The value of a non-`itemscope` `itemprop` element, per the WHATWG rules.
fn microdata_prop_value(el: &scraper::ElementRef) -> serde_json::Value {
    let element = el.value();
    let attr = |name: &str| element.attr(name).unwrap_or("").trim().to_string();
    let text = || el.text().collect::<String>().trim().to_string();

    let value = match element.name() {
        "meta" => attr("content"),
        "audio" | "embed" | "iframe" | "img" | "source" | "track" | "video" => attr("src"),
        "a" | "area" | "link" => attr("href"),
        "object" => attr("data"),
        "data" | "meter" => attr("value"),
        "time" => {
            let datetime = attr("datetime");
            if datetime.is_empty() {
                text()
            } else {
                datetime
            }
        }
        _ => text(),
    };
    serde_json::Value::String(value)
}

/// Short type name — last non-empty segment of an `itemtype`/`typeof` value,
/// splitting on `/`, `#`, and `:` so full IRIs (`https://schema.org/Recipe`),
/// CURIEs (`schema:Recipe`), and bare terms (`Recipe`) all yield `Recipe`.
fn short_type(itemtype: &str) -> Option<String> {
    itemtype
        .rsplit([':', '/', '#'])
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// RDFa (RDFa Lite)
//
// Parses `typeof`/`property`/`resource` into the same flattened, JSON-LD-shaped
// `serde_json::Value`s as Microdata, mirroring it structurally: `typeof` is the item and
// `@type` source, `property` is a property, `resource`/`about` is `@id`, and the
// nested-item boundary is `typeof`. Property values follow RDFa precedence:
// `@content` first (the `<meta property … content>` / Open Graph pattern), then
// `href`/`src`/`data`, then `<time datetime>`, then text.
//
// Page-level `property` elements with no enclosing `typeof` (e.g. Open Graph
// `<meta>` in `<head>`) are deliberately NOT emitted as items — they're handled
// by `scrape_metadata`.
//
// Deferred (documented limitations, not silently wrong answers):
//   - `rel`/`rev` link relations (RDFa Lite favors `property`).
//   - `resource`/`href` subject *chaining* on a `property` that lacks `typeof`:
//     such a sub-resource's properties are flattened onto the enclosing item
//     rather than split into a separate subject (rare; see the chaining test).
//   - Full `vocab`/`prefix` CURIE resolution — local names are surfaced as-is.
// ---------------------------------------------------------------------------

pub(super) fn scrape_rdfa_from_doc(document: &scraper::Html) -> Jsonld {
    document
        .select(&RDFA_SELECTOR)
        // Top-level typed resources: a `typeof` that also has `property` is a
        // nested resource (the object of its parent's property), reached by
        // recursion from that parent.
        .filter(|el| el.value().attr("property").is_none())
        .map(|el| rdfa_item_to_value(&el))
        .collect()
}

/// Build a flattened, JSON-LD-shaped object from a `typeof` element.
fn rdfa_item_to_value(item: &scraper::ElementRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    if let Some(types_attr) = item.value().attr("typeof") {
        let types: Vec<serde_json::Value> = types_attr
            .split_whitespace()
            .filter_map(short_type)
            .map(serde_json::Value::String)
            .collect();
        match types.len() {
            0 => {}
            1 => {
                obj.insert("@type".to_string(), types.into_iter().next().unwrap());
            }
            _ => {
                obj.insert("@type".to_string(), serde_json::Value::Array(types));
            }
        }
    }

    // Subject IRI: `resource`, else `about`.
    if let Some(id) = item
        .value()
        .attr("resource")
        .or_else(|| item.value().attr("about"))
    {
        let id = id.trim();
        if !id.is_empty() {
            obj.insert("@id".to_string(), serde_json::Value::String(id.to_string()));
        }
    }

    let mut props = Vec::new();
    collect_properties(item, "typeof", "property", &mut props);
    for (el, names) in props {
        let value = if el.value().attr("typeof").is_some() {
            rdfa_item_to_value(&el) // nested typed resource
        } else {
            rdfa_prop_value(&el)
        };
        for name in names.split_whitespace() {
            insert_prop(&mut obj, name, value.clone());
        }
    }

    serde_json::Value::Object(obj)
}

/// The value of a non-`typeof` `property` element, per RDFa precedence.
fn rdfa_prop_value(el: &scraper::ElementRef) -> serde_json::Value {
    let element = el.value();

    // `@content` is an explicit literal and overrides text/href (the Open Graph
    // / `<meta property … content>` pattern).
    if let Some(content) = element.attr("content") {
        return serde_json::Value::String(content.trim().to_string());
    }

    let attr = |name: &str| element.attr(name).unwrap_or("").trim().to_string();
    let text = || el.text().collect::<String>().trim().to_string();

    let value = match element.name() {
        "a" | "area" | "link" => attr("href"),
        "audio" | "embed" | "iframe" | "img" | "source" | "track" | "video" => attr("src"),
        "object" => attr("data"),
        "time" => {
            let datetime = attr("datetime");
            if datetime.is_empty() {
                text()
            } else {
                datetime
            }
        }
        _ => text(),
    };
    serde_json::Value::String(value)
}

// ---------------------------------------------------------------------------
// Microformats2
//
// Parses mf2 (class-based: `h-*` roots, `p-*`/`u-*`/`dt-*`/`e-*` properties)
// into canonical mf2 JSON: `{"type": ["h-card"], "properties": {"name": [...]},
// "children": [...]}`. mf2 is a DISTINCT vocabulary from schema.org, so this
// returns `Microformats` (not `Jsonld`); `microformats_to_schema` normalizes it,
// and `scrape_jsonld` folds the result into the unified schema.org view.
//
// Robustness against CSS utility classes that share these prefixes:
//   - roots are whitelisted to known mf2 vocabularies, so Tailwind/Bootstrap
//     `h-screen`/`h-full`/`h-4` are NOT mistaken for microformats;
//   - property classes whose name carries no letter (`p-4`, `p-2` padding) are
//     ignored.
//
// Deferred (documented limitations): mf1 backcompat (`vcard`/`hentry`/… → mf2),
// implied `name`/`url`/`photo` properties, `rel`-based properties (rel=me/tag),
// base-URL resolution, and the full p-/u- value-class patterns.
// ---------------------------------------------------------------------------

/// Known mf2 root vocabularies — whitelisted so CSS utility classes that share
/// the `h-` prefix (e.g. Tailwind `h-screen`) aren't parsed as microformats.
const MF_ROOTS: &[&str] = &[
    "h-adr",
    "h-card",
    "h-cite",
    "h-entry",
    "h-event",
    "h-feed",
    "h-geo",
    "h-item",
    "h-listing",
    "h-measure",
    "h-news",
    "h-price",
    "h-product",
    "h-recipe",
    "h-resume",
    "h-review",
    "h-review-aggregate",
    "h-app",
    "h-x-app",
];

#[derive(Clone, Copy)]
enum PropKind {
    P,
    U,
    Dt,
    E,
}

pub(super) fn scrape_microformats_from_doc(document: &scraper::Html) -> Microformats {
    document
        .select(&CLASS_SELECTOR)
        // Top-level roots only: a root nested inside another root is reached by
        // recursion (as a property value or a child).
        .filter(|el| is_mf_root(el) && !has_mf_root_ancestor(el))
        .map(|el| parse_mf_item(&el))
        // Drop empty top-level h-geo/h-adr — almost always a CSS class collision
        // on the bare words `geo`/`adr` rather than a real microformat.
        .filter(|item| !is_empty_geo_or_adr(item))
        .collect()
}

fn class_tokens(el: &scraper::ElementRef) -> Vec<String> {
    el.value()
        .attr("class")
        .map(|c| c.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn is_mf_root(el: &scraper::ElementRef) -> bool {
    el.value().attr("class").is_some_and(|c| {
        c.split_whitespace()
            .any(|t| MF_ROOTS.contains(&t) || mf1_root(t).is_some())
    })
}

/// A root's mf2 type list and, for an mf1 (backcompat) root, its vocabulary.
/// Prefers mf2: if the element carries an `h-*` class, that wins and `vocab` is
/// `None`, so mf2-prefixed properties aren't dropped on transitional markup.
fn root_types_and_vocab(el: &scraper::ElementRef) -> (Vec<String>, Option<Mf1Vocab>) {
    let tokens = class_tokens(el);
    let mf2: Vec<String> = tokens
        .iter()
        .filter(|t| MF_ROOTS.contains(&t.as_str()))
        .cloned()
        .collect();
    if !mf2.is_empty() {
        return (mf2, None);
    }
    for token in &tokens {
        if let Some((vocab, mf2_type)) = mf1_root(token) {
            return (vec![mf2_type.to_string()], Some(vocab));
        }
    }
    (Vec::new(), None)
}

fn has_mf_root_ancestor(el: &scraper::ElementRef) -> bool {
    let mut node = el.parent();
    while let Some(n) = node {
        if let Some(elem) = scraper::ElementRef::wrap(n) {
            if is_mf_root(&elem) {
                return true;
            }
        }
        node = n.parent();
    }
    false
}

/// Property `(kind, name)` pairs on an element. The no-letter guard drops CSS
/// utility classes like `p-4`/`p-2` that share a property prefix.
fn mf_property_classes(el: &scraper::ElementRef) -> Vec<(PropKind, String)> {
    class_tokens(el)
        .iter()
        .filter_map(|token| {
            let (kind, name) = if let Some(n) = token.strip_prefix("dt-") {
                (PropKind::Dt, n)
            } else if let Some(n) = token.strip_prefix("p-") {
                (PropKind::P, n)
            } else if let Some(n) = token.strip_prefix("u-") {
                (PropKind::U, n)
            } else if let Some(n) = token.strip_prefix("e-") {
                (PropKind::E, n)
            } else {
                return None;
            };
            if name.chars().any(|c| c.is_ascii_alphabetic()) {
                Some((kind, name.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn parse_mf_item(root: &scraper::ElementRef) -> serde_json::Value {
    let (types, vocab) = root_types_and_vocab(root);
    let mut properties = serde_json::Map::new();
    let mut children = Vec::new();
    collect_mf(root, vocab, &mut properties, &mut children);

    let mut obj = serde_json::Map::new();
    obj.insert(
        "type".to_string(),
        serde_json::Value::Array(types.into_iter().map(serde_json::Value::String).collect()),
    );
    obj.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !children.is_empty() {
        obj.insert("children".to_string(), serde_json::Value::Array(children));
    }
    serde_json::Value::Object(obj)
}

/// Walk descendants of a root, populating its `properties` and `children`.
/// Never descends into a nested root (its properties belong to it). `vocab` is
/// `Some` inside an mf1 root (use the per-vocab map) and `None` inside mf2.
fn collect_mf(
    el: &scraper::ElementRef,
    vocab: Option<Mf1Vocab>,
    properties: &mut serde_json::Map<String, serde_json::Value>,
    children: &mut Vec<serde_json::Value>,
) {
    for child in el.children().filter_map(scraper::ElementRef::wrap) {
        let prop_classes = match vocab {
            Some(v) => mf1_property_classes(&child, v),
            None => mf_property_classes(&child),
        };

        if is_mf_root(&child) {
            let nested = parse_mf_item(&child);
            if prop_classes.is_empty() {
                // Nested root with no property class → a child microformat.
                children.push(nested);
            } else {
                // Nested root that is also a property → its parsed object (with
                // an implied `value`) is the property value.
                let value = mf_nested_value(&child, &nested);
                let nested_value = with_value(nested, value);
                for (_, name) in &prop_classes {
                    push_mf_prop(properties, name, nested_value.clone());
                }
            }
            // Boundary: do not descend into the nested root.
        } else {
            for (kind, name) in &prop_classes {
                push_mf_prop(properties, name, mf_property_value(&child, *kind));
            }
            collect_mf(&child, vocab, properties, children);
        }
    }
}

fn push_mf_prop(
    properties: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: serde_json::Value,
) {
    match properties.get_mut(name) {
        Some(serde_json::Value::Array(arr)) => arr.push(value),
        _ => {
            properties.insert(name.to_string(), serde_json::Value::Array(vec![value]));
        }
    }
}

fn with_value(mut item: serde_json::Value, value: String) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut map) = item {
        map.insert("value".to_string(), serde_json::Value::String(value));
    }
    item
}

/// Implied `value` for a nested root used as a property: its first `name`, else
/// the element's normalized text.
fn mf_nested_value(el: &scraper::ElementRef, nested: &serde_json::Value) -> String {
    nested
        .get("properties")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_array())
        .and_then(|a| a.first())
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| mf_text(el))
}

fn mf_property_value(el: &scraper::ElementRef, kind: PropKind) -> serde_json::Value {
    match kind {
        PropKind::P => serde_json::Value::String(mf_p_value(el)),
        PropKind::U => serde_json::Value::String(mf_u_value(el)),
        PropKind::Dt => serde_json::Value::String(mf_dt_value(el)),
        PropKind::E => mf_e_value(el),
    }
}

fn mf_text(el: &scraper::ElementRef) -> String {
    el.text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn mf_p_value(el: &scraper::ElementRef) -> String {
    let element = el.value();
    let attr = |n: &str| element.attr(n).map(|s| s.trim().to_string());
    match element.name() {
        "abbr" | "link" => attr("title").unwrap_or_else(|| mf_text(el)),
        "data" | "input" => attr("value").unwrap_or_else(|| mf_text(el)),
        "img" | "area" => attr("alt").unwrap_or_default(),
        _ => mf_text(el),
    }
}

fn mf_u_value(el: &scraper::ElementRef) -> String {
    let element = el.value();
    let attr = |n: &str| element.attr(n).map(|s| s.trim().to_string());
    match element.name() {
        "a" | "area" | "link" => attr("href"),
        "img" | "audio" | "video" | "source" => attr("src"),
        "object" => attr("data"),
        _ => None,
    }
    .unwrap_or_else(|| mf_text(el))
}

fn mf_dt_value(el: &scraper::ElementRef) -> String {
    let element = el.value();
    let attr = |n: &str| element.attr(n).map(|s| s.trim().to_string());
    match element.name() {
        "time" | "ins" | "del" => attr("datetime"),
        "abbr" => attr("title"),
        "data" | "input" => attr("value"),
        _ => None,
    }
    .unwrap_or_else(|| mf_text(el))
}

fn mf_e_value(el: &scraper::ElementRef) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "html".to_string(),
        serde_json::Value::String(el.inner_html().trim().to_string()),
    );
    map.insert("value".to_string(), serde_json::Value::String(mf_text(el)));
    serde_json::Value::Object(map)
}

// ---------------------------------------------------------------------------
// Microformats1 backcompat
//
// mf1 is class-based like mf2 but UNPREFIXED (`vcard`/`fn`/`url`, not
// `h-card`/`p-name`), and the property→mf2 mapping is PER-VOCABULARY. We map mf1
// roots and properties to their mf2 equivalents and reuse the mf2 machinery
// (value rules, walk, boundary, children) verbatim.
//
// The per-vocab map doubles as a whitelist: only known property names for the
// current vocabulary are recognized. This BOUNDS but cannot eliminate (mf1 has
// no prefix) collisions with CSS classes that are English words — e.g. a CSS
// `class="title"` inside a vcard collides with hCard `title`→`p-job-title`.
//
// Scope: vcard/hCard, hentry+hfeed/hAtom, hrecipe, geo, adr. Deferred (spec maps
// not verifiable against real pages, with real quirk risk): vevent/hCalendar,
// hreview, hproduct, hresume, and rel-based properties.

#[derive(Clone, Copy)]
enum Mf1Vocab {
    Vcard,
    Hentry,
    Hfeed,
    Hrecipe,
    Geo,
    Adr,
}

/// An mf1 root class → (vocabulary, mf2 type).
fn mf1_root(class: &str) -> Option<(Mf1Vocab, &'static str)> {
    use Mf1Vocab::*;
    match class {
        "vcard" => Some((Vcard, "h-card")),
        "hentry" => Some((Hentry, "h-entry")),
        "hfeed" => Some((Hfeed, "h-feed")),
        "hrecipe" => Some((Hrecipe, "h-recipe")),
        "geo" => Some((Geo, "h-geo")),
        "adr" => Some((Adr, "h-adr")),
        _ => None,
    }
}

fn mf1_property_classes(el: &scraper::ElementRef, vocab: Mf1Vocab) -> Vec<(PropKind, String)> {
    class_tokens(el)
        .iter()
        .filter_map(|token| mf1_property(vocab, token).map(|(k, n)| (k, n.to_string())))
        .collect()
}

/// An mf1 `(vocabulary, class)` → mf2 `(kind, property-name)`. The match arms
/// ARE the per-vocab whitelist — an unlisted class is not a property.
fn mf1_property(vocab: Mf1Vocab, class: &str) -> Option<(PropKind, &'static str)> {
    use Mf1Vocab::*;
    use PropKind::*;
    match (vocab, class) {
        // hCard (vcard → h-card)
        (Vcard, "fn") => Some((P, "name")),
        (Vcard, "nickname") => Some((P, "nickname")),
        (Vcard, "org") => Some((P, "org")),
        (Vcard, "title") => Some((P, "job-title")),
        (Vcard, "role") => Some((P, "role")),
        (Vcard, "note") => Some((P, "note")),
        (Vcard, "category") => Some((P, "category")),
        (Vcard, "tel") => Some((P, "tel")),
        (Vcard, "label") => Some((P, "label")),
        (Vcard, "url") => Some((U, "url")),
        (Vcard, "email") => Some((U, "email")),
        (Vcard, "photo") => Some((U, "photo")),
        (Vcard, "logo") => Some((U, "logo")),
        (Vcard, "uid") => Some((U, "uid")),
        (Vcard, "bday") => Some((Dt, "bday")),
        (Vcard, "adr") => Some((P, "adr")),
        (Vcard, "geo") => Some((P, "geo")),
        (Vcard, "locality") => Some((P, "locality")),
        (Vcard, "region") => Some((P, "region")),
        (Vcard, "country-name") => Some((P, "country-name")),
        (Vcard, "postal-code") => Some((P, "postal-code")),
        (Vcard, "street-address") => Some((P, "street-address")),
        // hAtom entry (hentry → h-entry)
        (Hentry, "entry-title") => Some((P, "name")),
        (Hentry, "entry-summary") => Some((P, "summary")),
        (Hentry, "entry-content") => Some((E, "content")),
        (Hentry, "published") => Some((Dt, "published")),
        (Hentry, "updated") => Some((Dt, "updated")),
        (Hentry, "author") => Some((P, "author")),
        (Hentry, "category") => Some((P, "category")),
        (Hentry, "url") => Some((U, "url")),
        // hAtom feed (hfeed → h-feed): a container; entries become children.
        (Hfeed, "category") => Some((P, "category")),
        // hRecipe (hrecipe → h-recipe)
        (Hrecipe, "fn") => Some((P, "name")),
        (Hrecipe, "ingredient") => Some((P, "ingredient")),
        (Hrecipe, "yield") => Some((P, "yield")),
        (Hrecipe, "instructions") => Some((E, "instructions")),
        (Hrecipe, "duration") => Some((Dt, "duration")),
        (Hrecipe, "photo") => Some((U, "photo")),
        (Hrecipe, "summary") => Some((P, "summary")),
        (Hrecipe, "author") => Some((P, "author")),
        (Hrecipe, "published") => Some((Dt, "published")),
        (Hrecipe, "nutrition") => Some((P, "nutrition")),
        // geo → h-geo
        (Geo, "latitude") => Some((P, "latitude")),
        (Geo, "longitude") => Some((P, "longitude")),
        // adr → h-adr
        (Adr, "street-address") => Some((P, "street-address")),
        (Adr, "extended-address") => Some((P, "extended-address")),
        (Adr, "locality") => Some((P, "locality")),
        (Adr, "region") => Some((P, "region")),
        (Adr, "postal-code") => Some((P, "postal-code")),
        (Adr, "country-name") => Some((P, "country-name")),
        (Adr, "post-office-box") => Some((P, "post-office-box")),
        _ => None,
    }
}

/// True for a top-level `h-geo`/`h-adr` with no properties or children — almost
/// always a CSS `class="geo"`/`"adr"` collision rather than a real microformat
/// (those bare words double as common class names; the other mf1 roots are
/// microformat coinages that essentially never appear as CSS classes).
fn is_empty_geo_or_adr(item: &serde_json::Value) -> bool {
    let ty = item
        .get("type")
        .and_then(serde_json::Value::as_array)
        .and_then(|a| a.first())
        .and_then(serde_json::Value::as_str);
    matches!(ty, Some("h-geo") | Some("h-adr"))
        && item
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .map(serde_json::Map::is_empty)
            .unwrap_or(true)
        && item.get("children").is_none()
}

// ---------------------------------------------------------------------------
// Cross-encoding unify
// ---------------------------------------------------------------------------

/// Fold `extra` schema.org entities into `base`: an entity merges into an
/// existing one of the same `@type` + `name` (filling only missing/empty
/// properties), else is appended. Dedups the common case where a page carries
/// the same entity in two encodings — e.g. Microdata (name + ingredients, no
/// steps) and an h-recipe microformat (with steps) — into one complete entity
/// rather than a partial duplicate.
pub(super) fn merge_schema_entities(base: &mut Jsonld, extra: Jsonld) {
    for entity in extra {
        match base.iter_mut().find(|e| same_schema_entity(e, &entity)) {
            Some(existing) => fill_missing_props(existing, entity),
            None => base.push(entity),
        }
    }
}

/// Two entities are "the same" when they share a `@type` (short name) and a
/// non-empty `name`. Without a name there's no safe identity — don't merge.
fn same_schema_entity(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (schema_name(a), schema_name(b)) {
        (Some(na), Some(nb)) if na.eq_ignore_ascii_case(&nb) => {
            schema_type_short(a) == schema_type_short(b)
        }
        _ => false,
    }
}

fn schema_name(v: &serde_json::Value) -> Option<String> {
    match v.get("name")? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Array(arr) => arr
            .first()
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

/// Short `@type` name (last segment of an IRI/CURIE; first of an array).
fn schema_type_short(v: &serde_json::Value) -> Option<String> {
    let raw = match v.get("@type")? {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(arr) => arr.first()?.as_str()?,
        _ => return None,
    };
    Some(
        raw.rsplit(['/', '#', ':'])
            .next()
            .unwrap_or(raw)
            .to_string(),
    )
}

/// Fill only missing/empty properties of `existing` from `incoming` — a richer
/// existing value is never overwritten.
fn fill_missing_props(existing: &mut serde_json::Value, incoming: serde_json::Value) {
    let (Some(ex), serde_json::Value::Object(inc)) = (existing.as_object_mut(), incoming) else {
        return;
    };
    for (key, value) in inc {
        match ex.get(&key) {
            Some(current) if !schema_prop_is_empty(current) => {}
            _ => {
                ex.insert(key, value);
            }
        }
    }
}

fn schema_prop_is_empty(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Microformats2 → schema.org normalization
//
// `scrape_microformats_from_doc` yields canonical mf2 items
// (`{type:["h-recipe"], properties:{name:[...], ingredient:[...]}, …}`) — a
// DISTINCT vocabulary from schema.org. This maps the common `h-*` types and
// their prefix-stripped properties into the same schema.org-shaped `Jsonld` that
// JSON-LD / Microdata / RDFa produce, so a microformat-only page flows through
// the same `@type`-keyed consumers. Generic: a vocabulary table, no per-site or
// per-entity heuristics.
// ---------------------------------------------------------------------------

pub(super) fn microformats_to_schema(items: &Microformats) -> Jsonld {
    items.iter().filter_map(mf_item_to_schema).collect()
}

/// Convert one mf2 item to a schema.org-shaped object, or `None` if its type is
/// unrecognized or nothing mapped beyond `@type`.
fn mf_item_to_schema(item: &serde_json::Value) -> Option<serde_json::Value> {
    let h_type = item
        .get("type")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|t| t.starts_with("h-"))?;
    let schema_type = mf_type_to_schema(h_type)?;
    let props = item.get("properties")?.as_object()?;

    let mut obj = serde_json::Map::new();
    obj.insert("@type".to_string(), serde_json::Value::String(schema_type.to_string()));
    for (mf_key, values) in props {
        let Some(schema_key) = mf_prop_to_schema(h_type, mf_key) else {
            continue;
        };
        let Some(arr) = values.as_array() else {
            continue;
        };
        let value = convert_mf_values(arr, is_steplist_prop(schema_key));
        if !mf_value_is_empty(&value) {
            // First mapping wins (two mf props can map to one schema key, e.g.
            // `photo`/`featured` → `image`).
            obj.entry(schema_key).or_insert(value);
        }
    }

    // An entity that mapped nothing beyond `@type` carries no usable data.
    if obj.len() <= 1 {
        return None;
    }
    Some(serde_json::Value::Object(obj))
}

/// Common `h-*` microformat types → schema.org `@type`. Unrecognized types are
/// dropped (return `None`) rather than emitted as opaque entities.
fn mf_type_to_schema(h_type: &str) -> Option<&'static str> {
    Some(match h_type {
        "h-recipe" => "Recipe",
        "h-entry" => "Article",
        "h-card" => "Person",
        "h-event" => "Event",
        "h-product" => "Product",
        "h-review" => "Review",
        "h-adr" => "PostalAddress",
        "h-geo" => "GeoCoordinates",
        _ => return None,
    })
}

/// Map an mf2 property (prefix already stripped: `name`, `ingredient`, …) to its
/// schema.org name for the owning `h-*` type. Per-type rules first, then the
/// cross-type common set; unmapped properties are dropped.
fn mf_prop_to_schema(h_type: &str, prop: &str) -> Option<&'static str> {
    let mapped = match (h_type, prop) {
        ("h-recipe", "ingredient") => "recipeIngredient",
        ("h-recipe", "instructions") => "recipeInstructions",
        ("h-recipe", "yield") => "recipeYield",
        ("h-recipe", "duration") => "totalTime",
        ("h-recipe", "category") => "recipeCategory",
        ("h-recipe", "cuisine") => "recipeCuisine",
        ("h-recipe", "nutrition") => "nutrition",
        ("h-event", "start") => "startDate",
        ("h-event", "end") => "endDate",
        ("h-event", "location") => "location",
        ("h-product", "price") => "price",
        ("h-product", "brand") => "brand",
        ("h-product", "identifier") => "productID",
        ("h-product", "review") => "review",
        ("h-review", "rating") => "reviewRating",
        ("h-review", "item") => "itemReviewed",
        ("h-card", "org") => "worksFor",
        ("h-card", "tel") => "telephone",
        ("h-card", "email") => "email",
        ("h-card", "adr") => "address",
        ("h-card", "job-title") => "jobTitle",
        ("h-entry", "content") => "articleBody",
        ("h-entry", "category") => "keywords",
        _ => return mf_common_prop(prop),
    };
    Some(mapped)
}

fn mf_common_prop(prop: &str) -> Option<&'static str> {
    Some(match prop {
        "name" => "name",
        "summary" => "description",
        "photo" | "featured" => "image",
        "url" => "url",
        "author" => "author",
        "published" => "datePublished",
        "updated" => "dateModified",
        _ => return None,
    })
}

/// schema.org properties whose value type is a list (`ItemList`) — where an
/// `e-*` block with multiple children should expand to an array of texts (e.g.
/// recipe steps). Everything else is `Text` per schema.org and keeps the flat
/// string; notably `articleBody` is `Text`, not a list, so it is NOT split.
fn is_steplist_prop(schema_key: &str) -> bool {
    matches!(schema_key, "recipeInstructions")
}

/// Flatten mf2 property values (always an array) into a schema.org value: a lone
/// value collapses to a scalar, multiple to an array. For a `split_blocks`
/// property — one schema.org models as a list, see [`is_steplist_prop`] — an
/// `e-*` block whose markup has multiple `<li>`/`<p>` children expands to an
/// array of those texts (recipe steps). Otherwise the block stays a single flat
/// string, matching schema.org's `Text` typing (`articleBody`, `description`).
fn convert_mf_values(values: &[serde_json::Value], split_blocks: bool) -> serde_json::Value {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for v in values {
        match v {
            serde_json::Value::String(s) => push_nonempty(&mut out, s),
            serde_json::Value::Object(o) => {
                if let Some(html) = o.get("html").and_then(serde_json::Value::as_str) {
                    // `e-*` parsed-element property.
                    let blocks = if split_blocks {
                        split_e_blocks(html)
                    } else {
                        Vec::new()
                    };
                    if blocks.len() > 1 {
                        out.extend(blocks.into_iter().map(serde_json::Value::String));
                    } else if let Some(value) = o.get("value").and_then(serde_json::Value::as_str) {
                        push_nonempty(&mut out, value);
                    } else if let Some(block) = blocks.into_iter().next() {
                        out.push(serde_json::Value::String(block));
                    }
                } else if let Some(value) = o.get("value").and_then(serde_json::Value::as_str) {
                    // Nested-root property (e.g. an h-card used as `author`): use
                    // its implied `value`.
                    push_nonempty(&mut out, value);
                }
            }
            _ => {}
        }
    }
    match out.len() {
        1 => out.into_iter().next().unwrap(),
        _ => serde_json::Value::Array(out),
    }
}

fn push_nonempty(out: &mut Vec<serde_json::Value>, s: &str) {
    let trimmed = s.trim();
    if !trimmed.is_empty() {
        out.push(serde_json::Value::String(trimmed.to_string()));
    }
}

/// Split an `e-*` HTML fragment into step texts: prefer `<li>`, else `<p>`.
/// Empty when there are no block children (the caller falls back to the flat
/// `value`).
fn split_e_blocks(html: &str) -> Vec<String> {
    let frag = scraper::Html::parse_fragment(html);
    for selector in [&*LI_SELECTOR, &*P_SELECTOR] {
        let steps: Vec<String> = frag
            .select(selector)
            .map(|e| {
                e.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .collect();
        if !steps.is_empty() {
            return steps;
        }
    }
    Vec::new()
}

fn mf_value_is_empty(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::String(s) => s.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    }
}
