use scraper::Selector;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::types::{Html, Jsonld, Metadata};
use crate::utils::{embed_keys, schema_types, short_schema_type, JSONLD_SELECTOR};

/// Microformats2 items, each in canonical Microformats2 shape:
/// `{"type": ["h-card"], "properties": {"name": ["…"], …}, "children": [...]}`.
///
/// Not [`Jsonld`]: Microformats2 is a distinct vocabulary (`h-card`/`h-entry`, not
/// schema.org), keyed on `type` (not `@type`) with array-valued properties.
type Microformats = Vec<serde_json::Value>;

/// Selector for Microdata items (`itemscope` elements). Top-level items are
/// filtered in code (an `itemscope` that also has `itemprop` is a nested item).
static MICRODATA_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("[itemscope]").expect("valid microdata selector"));

/// Selector for RDFa typed resources (`typeof` elements). Top-level items are
/// filtered in code (a `typeof` that also has `property` is a nested resource).
static RDFA_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("[typeof]").expect("valid rdfa selector"));

/// Selectors for property elements, to tell when one encloses another.
static MICRODATA_PROPERTY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("[itemprop]").expect("valid microdata selector"));
static RDFA_PROPERTY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("[property]").expect("valid rdfa selector"));

/// Selector for any element with a `class` attribute. Microformats are
/// class-based (`h-*` roots, `p-*`/`u-*`/`dt-*`/`e-*` properties), so candidates
/// are matched broadly here and filtered by class token in code.
static CLASS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("[class]").expect("valid class selector"));

/// Selector for `<body>` elements.
static BODY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("body").expect("valid body selector"));

/// Selector for `<title>` tags.
static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("valid title selector"));

/// Selector for metadata tags with name/property attributes.
static META_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name], meta[property]").expect("valid metadata selector")
});

/// Selector for `<html lang="…">` elements.
static HTML_LANG_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("html[lang]").expect("valid html lang selector"));

/// Selector for `<li>` elements (Microformats2 `e-*` step splitting).
static LI_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("li").expect("valid li selector"));

/// Selector for `<p>` elements (Microformats2 `e-*` step splitting).
static P_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("p").expect("valid p selector"));

pub(super) fn scrape_body_from_document(document: &scraper::Html) -> Html {
    let markup = match document.select(&BODY_SELECTOR).next() {
        // A body with nothing in it is no body: a page read as empty has none.
        Some(body) if !body.has_children() => String::new(),
        Some(body) => body.html(),
        None => document.html(),
    };
    Html::new(markup)
}

/// The unified schema.org view from a parsed document: JSON-LD `<script>` tags +
/// Microdata + RDFa + Microformats2, with cross-encoding entities merged (same
/// `@type` + `name`).
pub(super) fn scrape_jsonld_from_document(document: &scraper::Html) -> Jsonld {
    let mut items = jsonld_entities(document);
    // One copy budget for the three encodings.
    reset_copy_budget(document);
    merge_schema_entities(&mut items, microdata_items(document));
    merge_schema_entities(&mut items, rdfa_items(document));
    merge_schema_entities(
        &mut items,
        microformats_to_schema(&microformat_items(document)),
    );
    items
}

/// The entities a document's JSON-LD scripts declare, each `@graph` opened.
pub(crate) fn jsonld_entities(document: &scraper::Html) -> Jsonld {
    document
        .select(&JSONLD_SELECTOR)
        .filter_map(|element| serde_json::from_str(&element.text().collect::<String>()).ok())
        .flat_map(flatten_jsonld)
        .collect()
}

/// Everything from one parse: body HTML, metadata, and the unified schema view.
pub(crate) fn scrape_from_document(document: &scraper::Html) -> (Html, Metadata, Jsonld) {
    (
        scrape_body_from_document(document),
        scrape_metadata_from_document(document),
        scrape_jsonld_from_document(document),
    )
}

fn flatten_jsonld(value: serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(array) => array.into_iter().flat_map(flatten_jsonld).collect(),
        serde_json::Value::Object(mut object) => {
            let graph = object
                .remove("@graph")
                .map(flatten_jsonld)
                .unwrap_or_default();
            // An object that only wraps its `@graph` is not an entity itself.
            let is_entity = object
                .keys()
                .any(|key| !matches!(key.as_str(), "@context" | "@id"));
            let mut entity = serde_json::Value::Object(object);
            shorten_types(&mut entity);
            is_entity
                .then_some(entity)
                .into_iter()
                .chain(graph)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Rewrite every `@type`, nested ones too, to short names, like the other
/// encodings yield. `@context` defines terms and a `@value` object is a typed
/// literal (`xsd:date`), so neither holds an entity type.
fn shorten_types(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => items.iter_mut().for_each(shorten_types),
        serde_json::Value::Object(fields) if !fields.contains_key("@value") => {
            if let Some(declared) = fields.get("@type") {
                let types = schema_types(&serde_json::json!({ "@type": declared }));
                match types.as_slice() {
                    [] => {}
                    [one] => {
                        fields.insert("@type".to_string(), one.clone().into());
                    }
                    many => {
                        fields.insert("@type".to_string(), many.to_vec().into());
                    }
                }
            }
            fields
                .iter_mut()
                .filter(|(key, _)| !matches!(key.as_str(), "@type" | "@context"))
                .for_each(|(_, nested)| shorten_types(nested));
        }
        _ => {}
    }
}

pub(super) fn scrape_metadata_from_document(document: &scraper::Html) -> Metadata {
    let mut tags = Vec::new();

    if let Some(element) = document.select(&TITLE_SELECTOR).next() {
        let text = element.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            tags.push(("title".to_string(), text));
        }
    }

    for element in document.select(&META_SELECTOR) {
        let key = element
            .value()
            .attr("name")
            .or_else(|| element.value().attr("property"));
        let value = element.value().attr("content");
        if let (Some(key), Some(value)) = (key, value) {
            // The embed's key prefixes are reserved for `scrape_embed`, so a
            // page can't pose as its embed.
            let lower = key.to_ascii_lowercase();
            let is_reserved = embed_keys::PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix));
            if !value.trim().is_empty() && !is_reserved {
                tags.push((key.to_string(), value.to_string()));
            }
        }
    }

    if let Some(element) = document.select(&HTML_LANG_SELECTOR).next() {
        if let Some(lang) = element.value().attr("lang") {
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

#[cfg(test)]
pub(super) fn scrape_microdata_from_document(document: &scraper::Html) -> Jsonld {
    reset_copy_budget(document);
    microdata_items(document)
}

fn microdata_items(document: &scraper::Html) -> Jsonld {
    document
        .select(&MICRODATA_SELECTOR)
        // Top-level items only: an `itemscope` element that also has `itemprop`
        // is a nested item (a property value of its enclosing item), reached by
        // recursion from that parent — not a top-level item.
        .filter(|element| element.value().attr("itemprop").is_none())
        .map(|element| microdata_item_to_value(&element))
        .collect()
}

/// Build a flattened, JSON-LD-shaped object from an `itemscope` element.
fn microdata_item_to_value(item: &scraper::ElementRef) -> serde_json::Value {
    let mut object = serde_json::Map::new();

    // `@type` from `itemtype` (short names). Anonymous items (no `itemtype`)
    // simply get no `@type`, which `extract_schema_types` skips.
    if let Some(itemtype) = item.value().attr("itemtype") {
        let types: Vec<serde_json::Value> = itemtype
            .split_whitespace()
            .filter_map(short_schema_type)
            .map(serde_json::Value::String)
            .collect();
        match types.len() {
            0 => {}
            1 => {
                object.insert("@type".to_string(), types.into_iter().next().unwrap());
            }
            _ => {
                object.insert("@type".to_string(), serde_json::Value::Array(types));
            }
        }
    }

    if let Some(itemid) = item.value().attr("itemid") {
        let itemid = itemid.trim();
        if !itemid.is_empty() {
            object.insert(
                "@id".to_string(),
                serde_json::Value::String(itemid.to_string()),
            );
        }
    }

    let mut properties = Vec::new();
    collect_properties(item, "itemscope", "itemprop", &mut properties);
    for (element, names) in properties {
        let is_nested_item = element.value().attr("itemscope").is_some();
        // A property enclosing another holds that one's text again: a copy.
        let is_copy = !is_nested_item
            && element
                .select(&MICRODATA_PROPERTY_SELECTOR)
                .next()
                .is_some();
        if is_copy && !has_copy_budget() {
            continue;
        }
        let value = if is_nested_item {
            microdata_item_to_value(&element)
        } else {
            microdata_property_value(&element)
        };
        if is_copy && !spend_copy_budget(&value) {
            continue;
        }
        insert_under_names(&mut object, &names, value, insert_property);
    }

    serde_json::Value::Object(object)
}

/// Collect `(element, property-name)` pairs belonging to `item`, walking
/// descendants but **never crossing into a nested item** (the `scope_attribute`
/// boundary), whose properties belong to that nested item, not this one. Shared
/// by the Microdata (`itemscope`/`itemprop`) and RDFa (`typeof`/`property`) walks.
fn collect_properties<'a>(
    item: &scraper::ElementRef<'a>,
    scope_attribute: &str,
    property_attribute: &str,
    out: &mut Vec<(scraper::ElementRef<'a>, String)>,
) {
    for child in item.children().filter_map(scraper::ElementRef::wrap) {
        let has_scope = child.value().attr(scope_attribute).is_some();
        let property = child.value().attr(property_attribute).map(str::to_string);

        if has_scope {
            // Nested item: record it as a property if named, but do NOT descend
            // — its inner properties belong to the nested item.
            if let Some(name) = property {
                out.push((child, name));
            }
        } else {
            if let Some(name) = property {
                out.push((child, name));
            }
            // A plain element can still contain more properties of THIS item.
            collect_properties(&child, scope_attribute, property_attribute, out);
        }
    }
}

/// Bytes of *copies* one document may yield per byte of its markup, over
/// [`COPY_BUDGET_FLOOR_BYTES`], across its Microdata, RDFa, and microformats.
/// A value is stored free under the first property that holds it, so a page
/// is never cut for what it says once. Every further storing is a copy and is
/// charged: the value under each further name its attribute lists, a
/// property's text when it encloses another property's, a microformat element
/// read under each further class, a nested microformat's implied `value`. So
/// a document yields at most what each encoding says once plus this many
/// times its size in copies; one that lists thousands of names, or nests a
/// property hundreds deep, would copy gigabytes.
const COPY_BYTES_PER_MARKUP_BYTE: usize = 8;

/// The copy budget every document has, however small its markup.
const COPY_BUDGET_FLOOR_BYTES: usize = 64 * 1024;

thread_local! {
    static COPY_BUDGET: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Start a document's walk with the budget its size allows.
fn reset_copy_budget(document: &scraper::Html) {
    let budget = document.html().len() * COPY_BYTES_PER_MARKUP_BYTE + COPY_BUDGET_FLOOR_BYTES;
    COPY_BUDGET.with(|left| left.set(budget));
}

/// What one copy of `value` costs: its text and its keys.
fn copy_cost(value: &serde_json::Value) -> usize {
    let mut cost = 0usize;
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        cost += 8;
        match value {
            serde_json::Value::String(text) => cost += text.len(),
            serde_json::Value::Array(items) => pending.extend(items),
            serde_json::Value::Object(fields) => {
                cost += fields.keys().map(String::len).sum::<usize>();
                pending.extend(fields.values());
            }
            _ => {}
        }
    }
    cost
}

/// Whether any budget is left. Checked before a copy is even read, so a
/// document that has spent its budget pays nothing more for asking.
fn has_copy_budget() -> bool {
    COPY_BUDGET.with(|left| left.get() > 0)
}

/// Take one copy of `value` from the budget. A copy the budget can't cover
/// ends it: no later copy is made either.
fn spend_copy_budget(value: &serde_json::Value) -> bool {
    COPY_BUDGET.with(|left| {
        if left.get() == 0 {
            return false;
        }
        let cost = copy_cost(value);
        let before = left.get();
        left.set(before.saturating_sub(cost));
        before >= cost
    })
}

/// Store `value` under each of the space-separated `names`: under the first
/// for free, under the rest while the document's copy budget lasts.
fn insert_under_names(
    object: &mut serde_json::Map<String, serde_json::Value>,
    names: &str,
    value: serde_json::Value,
    insert: fn(&mut serde_json::Map<String, serde_json::Value>, &str, serde_json::Value),
) {
    let mut names = names.split_whitespace();
    let Some(first) = names.next() else {
        return;
    };
    for name in names {
        if !spend_copy_budget(&value) {
            break;
        }
        insert(object, name, value.clone());
    }
    insert(object, first, value);
}

/// Insert a property, promoting to an array when the same name repeats.
fn insert_property(
    object: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: serde_json::Value,
) {
    match object.get_mut(name) {
        Some(serde_json::Value::Array(array)) => array.push(value),
        Some(existing) => {
            let previous = existing.take();
            *existing = serde_json::Value::Array(vec![previous, value]);
        }
        None => {
            object.insert(name.to_string(), value);
        }
    }
}

/// The value of a non-`itemscope` `itemprop` element, per the WHATWG rules.
fn microdata_property_value(element: &scraper::ElementRef) -> serde_json::Value {
    let element_node = element.value();
    let attribute = |name: &str| element_node.attr(name).unwrap_or("").trim().to_string();
    let text = || element.text().collect::<String>().trim().to_string();

    let value = match element_node.name() {
        "meta" => attribute("content"),
        "audio" | "embed" | "iframe" | "img" | "source" | "track" | "video" => attribute("src"),
        "a" | "area" | "link" => attribute("href"),
        "object" => attribute("data"),
        "data" | "meter" => attribute("value"),
        "time" => {
            let datetime = attribute("datetime");
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
//   - Full `vocabulary`/`prefix` CURIE resolution — local names are surfaced as-is.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(super) fn scrape_rdfa_from_document(document: &scraper::Html) -> Jsonld {
    reset_copy_budget(document);
    rdfa_items(document)
}

fn rdfa_items(document: &scraper::Html) -> Jsonld {
    document
        .select(&RDFA_SELECTOR)
        // Top-level typed resources: a `typeof` that also has `property` is a
        // nested resource (the object of its parent's property), reached by
        // recursion from that parent.
        .filter(|element| element.value().attr("property").is_none())
        .map(|element| rdfa_item_to_value(&element))
        .collect()
}

/// Build a flattened, JSON-LD-shaped object from a `typeof` element.
fn rdfa_item_to_value(item: &scraper::ElementRef) -> serde_json::Value {
    let mut object = serde_json::Map::new();

    if let Some(types_attribute) = item.value().attr("typeof") {
        let types: Vec<serde_json::Value> = types_attribute
            .split_whitespace()
            .filter_map(short_schema_type)
            .map(serde_json::Value::String)
            .collect();
        match types.len() {
            0 => {}
            1 => {
                object.insert("@type".to_string(), types.into_iter().next().unwrap());
            }
            _ => {
                object.insert("@type".to_string(), serde_json::Value::Array(types));
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
            object.insert("@id".to_string(), serde_json::Value::String(id.to_string()));
        }
    }

    let mut properties = Vec::new();
    collect_properties(item, "typeof", "property", &mut properties);
    for (element, names) in properties {
        let is_nested_resource = element.value().attr("typeof").is_some();
        // A property enclosing another holds that one's text again: a copy.
        let is_copy =
            !is_nested_resource && element.select(&RDFA_PROPERTY_SELECTOR).next().is_some();
        if is_copy && !has_copy_budget() {
            continue;
        }
        let value = if is_nested_resource {
            rdfa_item_to_value(&element)
        } else {
            rdfa_property_value(&element)
        };
        if is_copy && !spend_copy_budget(&value) {
            continue;
        }
        insert_under_names(&mut object, &names, value, insert_property);
    }

    serde_json::Value::Object(object)
}

/// The value of a non-`typeof` `property` element, per RDFa precedence.
fn rdfa_property_value(element: &scraper::ElementRef) -> serde_json::Value {
    let element_node = element.value();

    // `@content` is an explicit literal and overrides text/href (the Open Graph
    // / `<meta property … content>` pattern).
    if let Some(content) = element_node.attr("content") {
        return serde_json::Value::String(content.trim().to_string());
    }

    let attribute = |name: &str| element_node.attr(name).unwrap_or("").trim().to_string();
    let text = || element.text().collect::<String>().trim().to_string();

    let value = match element_node.name() {
        "a" | "area" | "link" => attribute("href"),
        "audio" | "embed" | "iframe" | "img" | "source" | "track" | "video" => attribute("src"),
        "object" => attribute("data"),
        "time" => {
            let datetime = attribute("datetime");
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
// Parses Microformats2 (class-based: `h-*` roots, `p-*`/`u-*`/`dt-*`/`e-*` properties)
// into canonical Microformats2 JSON: `{"type": ["h-card"], "properties": {"name": [...]},
// "children": [...]}`. Microformats2 is a DISTINCT vocabulary from schema.org, so this
// returns `Microformats` (not `Jsonld`); `microformats_to_schema` normalizes it,
// and `scrape_jsonld` folds the result into the unified schema.org view.
//
// Robustness against CSS utility classes that share these prefixes:
//   - roots are whitelisted to known Microformats2 vocabularies, so Tailwind/Bootstrap
//     `h-screen`/`h-full`/`h-4` are NOT mistaken for microformats;
//   - property classes whose name carries no letter (`p-4`, `p-2` padding) are
//     ignored.
//
// Not supported: implied `name`/`url`/`photo` properties, `rel`-based properties (rel=me/tag),
// base-URL resolution, and the full p-/u- value-class patterns.
// ---------------------------------------------------------------------------

/// Known Microformats2 root vocabularies — whitelisted so CSS utility classes that share
/// the `h-` prefix (e.g. Tailwind `h-screen`) aren't parsed as microformats.
const MICROFORMAT_ROOTS: &[&str] = &[
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
enum PropertyKind {
    P,
    U,
    Dt,
    E,
}

#[cfg(test)]
pub(super) fn scrape_microformats_from_document(document: &scraper::Html) -> Microformats {
    reset_copy_budget(document);
    microformat_items(document)
}

fn microformat_items(document: &scraper::Html) -> Microformats {
    document
        .select(&CLASS_SELECTOR)
        // Top-level roots only: a root nested inside another root is reached by
        // recursion (as a property value or a child).
        .filter(|element| is_microformat_root(element) && !has_microformat_root_ancestor(element))
        .map(|element| parse_microformat_item(&element))
        // Drop empty top-level h-geo/h-adr — almost always a CSS class collision
        // on the bare words `geo`/`adr` rather than a real microformat.
        .filter(|item| !is_empty_geo_or_adr(item))
        .collect()
}

fn class_tokens(element: &scraper::ElementRef) -> Vec<String> {
    element
        .value()
        .attr("class")
        .map(|class| class.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn is_microformat_root(element: &scraper::ElementRef) -> bool {
    element.value().attr("class").is_some_and(|class| {
        class
            .split_whitespace()
            .any(|token| MICROFORMAT_ROOTS.contains(&token) || microformats1_root(token).is_some())
    })
}

/// A root's Microformats2 type list and, for a Microformats1 (backcompat) root, its vocabulary.
/// Prefers Microformats2: if the element carries an `h-*` class, that wins and `vocabulary` is
/// `None`, so Microformats2-prefixed properties aren't dropped on transitional markup.
fn root_types_and_vocabulary(
    element: &scraper::ElementRef,
) -> (Vec<String>, Option<Microformats1Vocabulary>) {
    let tokens = class_tokens(element);
    let microformats2: Vec<String> = tokens
        .iter()
        .filter(|token| MICROFORMAT_ROOTS.contains(&token.as_str()))
        .cloned()
        .collect();
    if !microformats2.is_empty() {
        return (microformats2, None);
    }
    for token in &tokens {
        if let Some((vocabulary, microformats2_type)) = microformats1_root(token) {
            return (vec![microformats2_type.to_string()], Some(vocabulary));
        }
    }
    (Vec::new(), None)
}

fn has_microformat_root_ancestor(element: &scraper::ElementRef) -> bool {
    let mut node = element.parent();
    while let Some(ancestor) = node {
        if let Some(ancestor_element) = scraper::ElementRef::wrap(ancestor) {
            if is_microformat_root(&ancestor_element) {
                return true;
            }
        }
        node = ancestor.parent();
    }
    false
}

/// Property `(kind, name)` pairs on an element: Microformats1's under its `vocabulary`,
/// Microformats2's outside one.
fn property_classes(
    element: &scraper::ElementRef,
    vocabulary: Option<Microformats1Vocabulary>,
) -> Vec<(PropertyKind, String)> {
    match vocabulary {
        Some(vocabulary) => microformats1_property_classes(element, vocabulary),
        None => microformat_property_classes(element),
    }
}

/// Property `(kind, name)` pairs on an element. The no-letter guard drops CSS
/// utility classes like `p-4`/`p-2` that share a property prefix.
fn microformat_property_classes(element: &scraper::ElementRef) -> Vec<(PropertyKind, String)> {
    class_tokens(element)
        .iter()
        .filter_map(|token| {
            let (kind, name) = if let Some(property_name) = token.strip_prefix("dt-") {
                (PropertyKind::Dt, property_name)
            } else if let Some(property_name) = token.strip_prefix("p-") {
                (PropertyKind::P, property_name)
            } else if let Some(property_name) = token.strip_prefix("u-") {
                (PropertyKind::U, property_name)
            } else if let Some(property_name) = token.strip_prefix("e-") {
                (PropertyKind::E, property_name)
            } else {
                return None;
            };
            if name
                .chars()
                .any(|character| character.is_ascii_alphabetic())
            {
                Some((kind, name.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn parse_microformat_item(root: &scraper::ElementRef) -> serde_json::Value {
    let (types, vocabulary) = root_types_and_vocabulary(root);
    let mut properties = serde_json::Map::new();
    let mut children = Vec::new();
    collect_microformat(root, vocabulary, &mut properties, &mut children);

    let mut object = serde_json::Map::new();
    object.insert(
        "type".to_string(),
        serde_json::Value::Array(types.into_iter().map(serde_json::Value::String).collect()),
    );
    object.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !children.is_empty() {
        object.insert("children".to_string(), serde_json::Value::Array(children));
    }
    serde_json::Value::Object(object)
}

/// Walk descendants of a root, populating its `properties` and `children`.
/// Never descends into a nested root (its properties belong to it). `vocabulary` is
/// `Some` inside a Microformats1 root (use the per-vocabulary map) and `None` inside Microformats2.
fn collect_microformat(
    element: &scraper::ElementRef,
    vocabulary: Option<Microformats1Vocabulary>,
    properties: &mut serde_json::Map<String, serde_json::Value>,
    children: &mut Vec<serde_json::Value>,
) {
    for child in element.children().filter_map(scraper::ElementRef::wrap) {
        let child_property_classes = property_classes(&child, vocabulary);

        if is_microformat_root(&child) {
            if child_property_classes.is_empty() {
                // Nested root with no property class → a child microformat.
                children.push(parse_microformat_item(&child));
            } else {
                // Nested root that is also a property → its parsed object is the
                // property value, with an implied `value`. That `value` repeats
                // the nested item's own text: a copy, left out when the budget
                // can't cover it. It's charged before the item is parsed, so
                // this level, the one the schema view reads, is covered before
                // the levels inside it.
                let value = has_copy_budget()
                    .then(|| serde_json::Value::String(microformat_implied_value(&child)))
                    .filter(spend_copy_budget);
                let nested = parse_microformat_item(&child);
                let nested_value = match value {
                    Some(value) => with_value(nested, value),
                    None => nested,
                };
                let names: Vec<&str> = child_property_classes
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect();
                insert_under_names(
                    properties,
                    &names.join(" "),
                    nested_value,
                    push_microformat_property,
                );
            }
            // Boundary: do not descend into the nested root.
        } else {
            // Each class reads the element its own way (`p-` text, `u-` URL), so
            // each is a value of its own: the first free, the rest copies. A
            // property enclosing another holds that one's text again: a copy.
            let encloses_a_property = !child_property_classes.is_empty()
                && child
                    .descendants()
                    .skip(1)
                    .filter_map(scraper::ElementRef::wrap)
                    .any(|descendant| !property_classes(&descendant, vocabulary).is_empty());
            for (index, (kind, name)) in child_property_classes.iter().enumerate() {
                let is_copy = index > 0 || encloses_a_property;
                if is_copy && !has_copy_budget() {
                    break;
                }
                let value = microformat_property_value(&child, *kind);
                if is_copy && !spend_copy_budget(&value) {
                    break;
                }
                push_microformat_property(properties, name, value);
            }
            collect_microformat(&child, vocabulary, properties, children);
        }
    }
}

fn push_microformat_property(
    properties: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: serde_json::Value,
) {
    match properties.get_mut(name) {
        Some(serde_json::Value::Array(array)) => array.push(value),
        _ => {
            properties.insert(name.to_string(), serde_json::Value::Array(vec![value]));
        }
    }
}

fn with_value(mut item: serde_json::Value, value: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut map) = item {
        map.insert("value".to_string(), value);
    }
    item
}

/// Implied `value` for a nested root used as a property: its first `name`, else
/// the element's normalized text.
fn microformat_implied_value(root: &scraper::ElementRef) -> String {
    let (_, vocabulary) = root_types_and_vocabulary(root);
    first_name_element(root, vocabulary)
        .map(|name| microformat_p_value(&name))
        .unwrap_or_else(|| microformat_text(root))
}

/// The element carrying a root's first `name` property, outside any nested
/// root, in document order.
fn first_name_element<'a>(
    element: &scraper::ElementRef<'a>,
    vocabulary: Option<Microformats1Vocabulary>,
) -> Option<scraper::ElementRef<'a>> {
    for child in element.children().filter_map(scraper::ElementRef::wrap) {
        if is_microformat_root(&child) {
            continue;
        }
        let is_name = property_classes(&child, vocabulary)
            .iter()
            .any(|(kind, name)| matches!(kind, PropertyKind::P) && name == "name");
        if is_name {
            return Some(child);
        }
        if let Some(found) = first_name_element(&child, vocabulary) {
            return Some(found);
        }
    }
    None
}

fn microformat_property_value(
    element: &scraper::ElementRef,
    kind: PropertyKind,
) -> serde_json::Value {
    match kind {
        PropertyKind::P => serde_json::Value::String(microformat_p_value(element)),
        PropertyKind::U => serde_json::Value::String(microformat_u_value(element)),
        PropertyKind::Dt => serde_json::Value::String(microformat_dt_value(element)),
        PropertyKind::E => microformat_e_value(element),
    }
}

fn microformat_text(element: &scraper::ElementRef) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn microformat_p_value(element: &scraper::ElementRef) -> String {
    let element_node = element.value();
    let attribute = |name: &str| {
        element_node
            .attr(name)
            .map(|value| value.trim().to_string())
    };
    match element_node.name() {
        "abbr" | "link" => attribute("title").unwrap_or_else(|| microformat_text(element)),
        "data" | "input" => attribute("value").unwrap_or_else(|| microformat_text(element)),
        "img" | "area" => attribute("alt").unwrap_or_default(),
        _ => microformat_text(element),
    }
}

fn microformat_u_value(element: &scraper::ElementRef) -> String {
    let element_node = element.value();
    let attribute = |name: &str| {
        element_node
            .attr(name)
            .map(|value| value.trim().to_string())
    };
    match element_node.name() {
        "a" | "area" | "link" => attribute("href"),
        "img" | "audio" | "video" | "source" => attribute("src"),
        "object" => attribute("data"),
        _ => None,
    }
    .unwrap_or_else(|| microformat_text(element))
}

fn microformat_dt_value(element: &scraper::ElementRef) -> String {
    let element_node = element.value();
    let attribute = |name: &str| {
        element_node
            .attr(name)
            .map(|value| value.trim().to_string())
    };
    match element_node.name() {
        "time" | "ins" | "del" => attribute("datetime"),
        "abbr" => attribute("title"),
        "data" | "input" => attribute("value"),
        _ => None,
    }
    .unwrap_or_else(|| microformat_text(element))
}

fn microformat_e_value(element: &scraper::ElementRef) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "html".to_string(),
        serde_json::Value::String(element.inner_html().trim().to_string()),
    );
    map.insert(
        "value".to_string(),
        serde_json::Value::String(microformat_text(element)),
    );
    serde_json::Value::Object(map)
}

// ---------------------------------------------------------------------------
// Microformats1 backcompat
//
// Microformats1 is class-based like Microformats2 but UNPREFIXED (`vcard`/`fn`/`url`, not
// `h-card`/`p-name`), and the property→Microformats2 mapping is PER-VOCABULARY. We map
// Microformats1 roots and properties to their Microformats2 equivalents and reuse the
// Microformats2 machinery (value rules, walk, boundary, children) verbatim.
//
// The per-vocabulary map doubles as a whitelist: only known property names for the
// current vocabulary are recognized. This BOUNDS but cannot eliminate (Microformats1 has
// no prefix) collisions with CSS classes that are English words — e.g. a CSS
// `class="title"` inside a vcard collides with hCard `title`→`p-job-title`.
//
// Scope: vcard/hCard, hentry+hfeed/hAtom, hrecipe, geo, adr. Deferred (spec maps
// not verifiable against real pages, with real quirk risk): vevent/hCalendar,
// hreview, hproduct, hresume, and rel-based properties.

#[derive(Clone, Copy)]
enum Microformats1Vocabulary {
    Vcard,
    Hentry,
    Hfeed,
    Hrecipe,
    Geo,
    Adr,
}

/// A Microformats1 root class → (vocabulary, Microformats2 type).
fn microformats1_root(class: &str) -> Option<(Microformats1Vocabulary, &'static str)> {
    use Microformats1Vocabulary::*;
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

fn microformats1_property_classes(
    element: &scraper::ElementRef,
    vocabulary: Microformats1Vocabulary,
) -> Vec<(PropertyKind, String)> {
    class_tokens(element)
        .iter()
        .filter_map(|token| {
            microformats1_property(vocabulary, token).map(|(kind, name)| (kind, name.to_string()))
        })
        .collect()
}

/// A Microformats1 `(vocabulary, class)` → Microformats2 `(kind, property-name)`. The match arms
/// ARE the per-vocabulary whitelist — an unlisted class is not a property.
fn microformats1_property(
    vocabulary: Microformats1Vocabulary,
    class: &str,
) -> Option<(PropertyKind, &'static str)> {
    use Microformats1Vocabulary::*;
    use PropertyKind::*;
    match (vocabulary, class) {
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
/// (those bare words double as common class names; the other Microformats1 roots are
/// microformat coinages that essentially never appear as CSS classes).
fn is_empty_geo_or_adr(item: &serde_json::Value) -> bool {
    let first_type = item
        .get("type")
        .and_then(serde_json::Value::as_array)
        .and_then(|types| types.first())
        .and_then(serde_json::Value::as_str);
    matches!(first_type, Some("h-geo") | Some("h-adr"))
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
    // Where each identity first appears in `base`.
    let mut index_of: HashMap<SchemaIdentity, usize> = HashMap::new();
    for (index, entity) in base.iter().enumerate() {
        if let Some(identity) = schema_identity(entity) {
            index_of.entry(identity).or_insert(index);
        }
    }
    for entity in extra {
        match schema_identity(&entity).map(|identity| index_of.entry(identity)) {
            Some(Entry::Occupied(existing)) => {
                fill_missing_properties(&mut base[*existing.get()], entity)
            }
            Some(Entry::Vacant(vacant)) => {
                vacant.insert(base.len());
                base.push(entity);
            }
            None => base.push(entity),
        }
    }
}

/// What makes two entities the same: their `@type` (short name) and their
/// `name`, case-insensitive.
type SchemaIdentity = (Option<String>, String);

/// The entity's identity, or `None` when it has no non-empty `name`: without
/// one there's no safe identity, so it's never merged.
fn schema_identity(entity: &serde_json::Value) -> Option<SchemaIdentity> {
    Some((
        schema_type_short(entity),
        schema_name(entity)?.to_ascii_lowercase(),
    ))
}

fn schema_name(entity: &serde_json::Value) -> Option<String> {
    match entity.get("name")? {
        serde_json::Value::String(name) if !name.trim().is_empty() => Some(name.trim().to_string()),
        serde_json::Value::Array(array) => array
            .first()
            .and_then(serde_json::Value::as_str)
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty()),
        _ => None,
    }
}

/// Short `@type` name (last segment of an IRI/CURIE; first of an array).
fn schema_type_short(entity: &serde_json::Value) -> Option<String> {
    schema_types(entity).into_iter().next()
}

/// Fill only missing/empty properties of `existing` from `incoming` — a richer
/// existing value is never overwritten.
fn fill_missing_properties(existing: &mut serde_json::Value, incoming: serde_json::Value) {
    let (Some(existing_fields), serde_json::Value::Object(incoming_fields)) =
        (existing.as_object_mut(), incoming)
    else {
        return;
    };
    for (key, value) in incoming_fields {
        match existing_fields.get(&key) {
            Some(current) if !schema_property_is_empty(current) => {}
            _ => {
                existing_fields.insert(key, value);
            }
        }
    }
}

fn schema_property_is_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(text) => text.trim().is_empty(),
        serde_json::Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Microformats2 → schema.org normalization
//
// `microformat_items` yields canonical Microformats2 items
// (`{type:["h-recipe"], properties:{name:[...], ingredient:[...]}, …}`) — a
// DISTINCT vocabulary from schema.org. This maps the common `h-*` types and
// their prefix-stripped properties into the same schema.org-shaped `Jsonld` that
// JSON-LD / Microdata / RDFa produce, so a microformat-only page flows through
// the same `@type`-keyed consumers. Generic: a vocabulary table, no per-site or
// per-entity heuristics.
// ---------------------------------------------------------------------------

pub(super) fn microformats_to_schema(items: &Microformats) -> Jsonld {
    // Every item and each of its `children` at any depth, parent first: a
    // recipe card inside `hentry` inside `hfeed` is an entity of the page too.
    let mut entities = Vec::new();
    let mut pending: Vec<&serde_json::Value> = items.iter().rev().collect();
    while let Some(item) = pending.pop() {
        entities.extend(microformat_item_to_schema(item));
        if let Some(children) = item.get("children").and_then(serde_json::Value::as_array) {
            pending.extend(children.iter().rev());
        }
    }
    entities
}

/// Convert one Microformats2 item to a schema.org-shaped object, or `None` if its type is
/// unrecognized or nothing mapped beyond `@type`.
fn microformat_item_to_schema(item: &serde_json::Value) -> Option<serde_json::Value> {
    let h_type = item
        .get("type")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|item_type| item_type.starts_with("h-"))?;
    let schema_type = microformat_type_to_schema(h_type)?;
    let properties = item.get("properties")?.as_object()?;

    let mut object = serde_json::Map::new();
    object.insert(
        "@type".to_string(),
        serde_json::Value::String(schema_type.to_string()),
    );
    for (microformat_key, values) in properties {
        let Some(schema_key) = microformat_property_to_schema(h_type, microformat_key) else {
            continue;
        };
        let Some(array) = values.as_array() else {
            continue;
        };
        let value = convert_microformat_values(array, is_steplist_property(schema_key));
        if !microformat_value_is_empty(&value) {
            // First mapping wins (two microformat properties can map to one schema key, e.g.
            // `photo`/`featured` → `image`).
            object.entry(schema_key).or_insert(value);
        }
    }

    // An entity that mapped nothing beyond `@type` carries no usable data.
    if object.len() <= 1 {
        return None;
    }
    Some(serde_json::Value::Object(object))
}

/// Common `h-*` microformat types → schema.org `@type`. Unrecognized types are
/// dropped (return `None`) rather than emitted as opaque entities.
fn microformat_type_to_schema(h_type: &str) -> Option<&'static str> {
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

/// Map a Microformats2 property (prefix already stripped: `name`, `ingredient`, …) to its
/// schema.org name for the owning `h-*` type. Per-type rules first, then the
/// cross-type common set; unmapped properties are dropped.
fn microformat_property_to_schema(h_type: &str, property: &str) -> Option<&'static str> {
    let mapped = match (h_type, property) {
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
        _ => return microformat_common_property(property),
    };
    Some(mapped)
}

fn microformat_common_property(property: &str) -> Option<&'static str> {
    Some(match property {
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
fn is_steplist_property(schema_key: &str) -> bool {
    matches!(schema_key, "recipeInstructions")
}

/// Flatten Microformats2 property values (always an array) into a schema.org value: a lone
/// value collapses to a scalar, multiple to an array. For a `split_blocks`
/// property — one schema.org models as a list, see [`is_steplist_property`] — an
/// `e-*` block whose markup has multiple `<li>`/`<p>` children expands to an
/// array of those texts (recipe steps). Otherwise the block stays a single flat
/// string, matching schema.org's `Text` typing (`articleBody`, `description`).
fn convert_microformat_values(
    values: &[serde_json::Value],
    split_blocks: bool,
) -> serde_json::Value {
    let mut out: Vec<serde_json::Value> = Vec::new();
    for microformat_value in values {
        match microformat_value {
            serde_json::Value::String(text) => push_nonempty(&mut out, text),
            serde_json::Value::Object(fields) => {
                if let Some(html) = fields.get("html").and_then(serde_json::Value::as_str) {
                    // `e-*` parsed-element property.
                    let blocks = if split_blocks {
                        split_e_blocks(html)
                    } else {
                        Vec::new()
                    };
                    if blocks.len() > 1 {
                        out.extend(blocks.into_iter().map(serde_json::Value::String));
                    } else if let Some(value) =
                        fields.get("value").and_then(serde_json::Value::as_str)
                    {
                        push_nonempty(&mut out, value);
                    } else if let Some(block) = blocks.into_iter().next() {
                        out.push(serde_json::Value::String(block));
                    }
                } else if let Some(value) = fields
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| {
                        microformat_value
                            .pointer("/properties/name/0")
                            .and_then(serde_json::Value::as_str)
                    })
                {
                    // Nested-root property (e.g. an h-card used as `author`): its
                    // implied `value`, or its `name` when the budget left the
                    // `value` out.
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

fn push_nonempty(out: &mut Vec<serde_json::Value>, text: &str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(serde_json::Value::String(trimmed.to_string()));
    }
}

/// Split an `e-*` HTML fragment into step texts: prefer `<li>`, else `<p>`.
/// Text before the first block is a paragraph of its own: the fragment is
/// parsed inside an open `<p>`, which the parser closes at the first block.
/// Empty when there is no text (the caller falls back to the flat `value`).
fn split_e_blocks(html: &str) -> Vec<String> {
    let fragment = scraper::Html::parse_fragment(&format!("<p>{html}"));
    for selector in [&*LI_SELECTOR, &*P_SELECTOR] {
        let steps: Vec<String> = fragment
            .select(selector)
            .map(|element| {
                element
                    .text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|step| !step.is_empty())
            .collect();
        if !steps.is_empty() {
            return steps;
        }
    }
    Vec::new()
}

fn microformat_value_is_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => items.is_empty(),
        serde_json::Value::String(text) => text.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    }
}
