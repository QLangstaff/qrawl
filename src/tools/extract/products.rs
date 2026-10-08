//! Product extraction: schema.org `Product` / `ProductGroup` → [`ExtractProductResult`],
//! with an Open Graph `product:*` fallback.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;
use url::Url;

use super::types::{ExtractProductResult, ExtractProductVariant};
use crate::tools::classify::entity_types;
use crate::tools::normalize::normalize_text;
use crate::types::{Jsonld, Metadata};

/// schema.org types treated as a product.
const PRODUCT_TYPES: &[&str] = &[
    "Product",
    "ProductGroup",
    "IndividualProduct",
    "ProductModel",
    "SomeProducts",
];

/// Scalar / QuantitativeValue properties surfaced as `attributes`.
const ATTRIBUTE_KEYS: &[&str] = &[
    "color", "material", "pattern", "size", "width", "height", "depth", "weight",
];

/// Availability values that mean the product can be bought now.
const AVAILABLE: &[&str] = &[
    "InStock",
    "InStoreOnly",
    "OnlineOnly",
    "LimitedAvailability",
];

pub(super) fn extract_products(
    jsonld: &Jsonld,
    metadata: &Metadata,
    page_url: &str,
) -> Vec<ExtractProductResult> {
    let base = Url::parse(page_url).ok();

    let mut nodes = Vec::new();
    for value in jsonld {
        collect_product_nodes(value, &mut nodes);
    }
    let mut products: Vec<ExtractProductResult> = nodes
        .into_iter()
        .map(|node| product_from_node(node, base.as_ref(), page_url))
        .filter(has_content)
        .collect();

    let og = product_from_metadata(metadata, base.as_ref(), page_url);
    match products.len() {
        // No structured product: fall back to Open Graph, if the page declares
        // itself a product.
        0 if og.declared && has_content(&og.product) => products.push(og.product),
        // A single product page: fill gaps (commonly images) from Open Graph.
        1 => fill_missing(&mut products[0], og.product),
        _ => {}
    }

    // An entity encoded twice (e.g. a Product inside both `mainEntity` and an
    // `ItemList`) keeps its first occurrence.
    let mut seen = HashSet::new();
    products.retain(|p| seen.insert((p.url.clone(), p.name.clone())));
    products
}

/// Collect product nodes anywhere in the tree (`@graph`, `ItemList`,
/// `mainEntity`, …) without descending into a product itself, so its
/// variants / related products don't surface as separate results.
fn collect_product_nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect_product_nodes(v, out)),
        Value::Object(obj) => {
            if is_product(value) {
                out.push(value);
            } else {
                obj.values().for_each(|v| collect_product_nodes(v, out));
            }
        }
        _ => {}
    }
}

fn is_product(value: &Value) -> bool {
    entity_types(value)
        .iter()
        .any(|t| PRODUCT_TYPES.contains(&t.as_str()))
}

fn has_content(p: &ExtractProductResult) -> bool {
    p.name.is_some() || p.price.is_some() || !p.images.is_empty()
}

fn product_from_node(node: &Value, base: Option<&Url>, page_url: &str) -> ExtractProductResult {
    let mut offers = parse_offers(node.get("offers"));

    let variants: Vec<ExtractProductVariant> = as_items(node.get("hasVariant"))
        .into_iter()
        .filter(|v| v.is_object())
        .map(|v| variant_from_node(v, base))
        .collect();
    // A group's price range spans its variants.
    offers.extend(variants.iter().map(|v| Offer {
        low: v.price,
        high: v.price,
        currency: v.currency.clone(),
        availability: v.availability.clone(),
    }));

    let mut images = Vec::new();
    collect_images(node.get("image"), base, &mut images);
    if images.is_empty() {
        for variant in as_items(node.get("hasVariant")) {
            collect_images(variant.get("image"), base, &mut images);
        }
    }

    let (price, price_max) = price_range(&offers);
    ExtractProductResult {
        name: text(node.get("name")),
        url: product_url(node, base).or_else(|| Some(page_url.to_string())),
        brand: name_of(node.get("brand")).or_else(|| name_of(node.get("manufacturer"))),
        sku: text(node.get("sku")).or_else(|| text(node.get("mpn"))),
        description: text(node.get("description")).map(|d| strip_tags(&d)),
        price,
        price_max,
        currency: offers.iter().find_map(|o| o.currency.clone()),
        availability: best_availability(&offers),
        images: dedupe(images),
        rating: node
            .get("aggregateRating")
            .and_then(|r| number(r.get("ratingValue"))),
        review_count: node.get("aggregateRating").and_then(|r| {
            number(r.get("reviewCount"))
                .or_else(|| number(r.get("ratingCount")))
                .map(|n| n as u64)
        }),
        attributes: attributes(node),
        variants,
    }
}

fn variant_from_node(node: &Value, base: Option<&Url>) -> ExtractProductVariant {
    let offers = parse_offers(node.get("offers"));
    ExtractProductVariant {
        name: text(node.get("name")),
        url: product_url(node, base),
        sku: text(node.get("sku")),
        price: price_range(&offers).0,
        currency: offers.iter().find_map(|o| o.currency.clone()),
        availability: best_availability(&offers),
        attributes: attributes(node),
    }
}

/// `url`, else an absolute `@id` (minus its `#fragment`).
fn product_url(node: &Value, base: Option<&Url>) -> Option<String> {
    text(node.get("url"))
        .and_then(|u| resolve(&u, base))
        .or_else(|| {
            let id = text(node.get("@id"))?;
            let mut url = Url::parse(&id).ok()?;
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            url.set_fragment(None);
            Some(url.to_string())
        })
}

// ---------------------------------------------------------------------------
// Offers / prices
// ---------------------------------------------------------------------------

struct Offer {
    low: Option<f64>,
    high: Option<f64>,
    currency: Option<String>,
    availability: Option<String>,
}

/// Flatten `offers` (an `Offer`, `AggregateOffer`, or array of either; an
/// `AggregateOffer` may nest its own `offers`) into price points.
fn parse_offers(value: Option<&Value>) -> Vec<Offer> {
    let mut out = Vec::new();
    for offer in as_items(value) {
        if !offer.is_object() {
            continue;
        }
        let spec = as_items(offer.get("priceSpecification"))
            .into_iter()
            .find(|s| s.get("price").is_some());
        let amount = price(offer.get("price")).or_else(|| spec.and_then(|s| price(s.get("price"))));
        out.push(Offer {
            low: price(offer.get("lowPrice")).or(amount),
            high: price(offer.get("highPrice")).or(amount),
            currency: text(offer.get("priceCurrency"))
                .or_else(|| spec.and_then(|s| text(s.get("priceCurrency")))),
            availability: text(offer.get("availability")).and_then(|a| last_segment(&a)),
        });
        out.extend(parse_offers(offer.get("offers")));
    }
    out
}

/// Lowest price, plus the highest when it differs (a range).
fn price_range(offers: &[Offer]) -> (Option<f64>, Option<f64>) {
    let low = offers.iter().filter_map(|o| o.low).reduce(f64::min);
    let high = offers.iter().filter_map(|o| o.high).reduce(f64::max);
    match (low, high) {
        (Some(l), Some(h)) if h > l => (Some(l), Some(h)),
        (Some(l), _) => (Some(l), None),
        (None, h) => (h, None),
    }
}

/// Purchasable wins over any other state; otherwise the first one listed.
fn best_availability(offers: &[Offer]) -> Option<String> {
    let all: Vec<&String> = offers
        .iter()
        .filter_map(|o| o.availability.as_ref())
        .collect();
    all.iter()
        .find(|a| AVAILABLE.contains(&a.as_str()))
        .or(all.first())
        .map(|a| a.to_string())
}

/// A positive price from a JSON number or a formatted string.
fn price(value: Option<&Value>) -> Option<f64> {
    let p = match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_price(s),
        _ => None,
    }?;
    (p.is_finite() && p > 0.0).then_some(p)
}

/// Parse the first number in a price string, accepting both `1,299.00` and
/// `1.299,00` grouping. With one separator kind only, a single `,` followed by
/// 1–2 digits is a decimal comma; `.` is a decimal point unless repeated.
pub(super) fn parse_price(s: &str) -> Option<f64> {
    let raw: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let raw = raw.trim_end_matches([',', '.']);
    if raw.is_empty() {
        return None;
    }

    let last_comma = raw.rfind(',');
    let last_dot = raw.rfind('.');
    let decimal = match (last_comma, last_dot) {
        (Some(c), Some(d)) => Some(if c > d { ',' } else { '.' }),
        (Some(c), None) => {
            let decimals = raw.len() - c - 1;
            (raw.matches(',').count() == 1 && decimals <= 2).then_some(',')
        }
        (None, Some(_)) => (raw.matches('.').count() == 1).then_some('.'),
        (None, None) => None,
    };

    let normalized: String = raw
        .chars()
        .filter_map(|c| match c {
            ',' | '.' if Some(c) == decimal => Some('.'),
            ',' | '.' => None,
            c => Some(c),
        })
        .collect();
    normalized.parse().ok()
}

// ---------------------------------------------------------------------------
// Attributes
// ---------------------------------------------------------------------------

fn attributes(node: &Value) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    for key in ATTRIBUTE_KEYS {
        if let Some(v) = quantity_or_text(node.get(*key)) {
            attrs.insert(key.to_string(), v);
        }
    }
    for prop in as_items(node.get("additionalProperty")) {
        let (Some(name), Some(value)) = (text(prop.get("name")), quantity_or_text(Some(prop)))
        else {
            continue;
        };
        attrs.entry(name).or_insert(value);
    }
    attrs
}

/// A plain value, or a `QuantitativeValue` / `PropertyValue` rendered as
/// `"60 in"`. Arrays join with `", "`.
fn quantity_or_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| quantity_or_text(Some(v)))
                .collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        obj @ Value::Object(_) => {
            let v = text(obj.get("value")).or_else(|| text(obj.get("name")))?;
            let unit = text(obj.get("unitText"))
                .or_else(|| text(obj.get("unitCode")).map(|c| unit_symbol(&c)));
            Some(match unit {
                Some(u) => format!("{v} {u}"),
                None => v,
            })
        }
        other => text(Some(other)),
    }
}

/// UN/CEFACT unit codes commonly used in product data.
fn unit_symbol(code: &str) -> String {
    match code {
        "INH" => "in",
        "FOT" => "ft",
        "CMT" => "cm",
        "MMT" => "mm",
        "MTR" => "m",
        "LBR" => "lb",
        "ONZ" => "oz",
        "KGM" => "kg",
        "GRM" => "g",
        other => other,
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Open Graph fallback
// ---------------------------------------------------------------------------

struct OgProduct {
    product: ExtractProductResult,
    /// Whether the page declares itself a product (`og:type` product, or a
    /// `product:price:*` / `og:price:*` tag) — required to use it standalone.
    declared: bool,
}

/// Product fields from Open Graph `og:*` / `product:*` / `og:price:*` tags.
fn product_from_metadata(metadata: &Metadata, base: Option<&Url>, page_url: &str) -> OgProduct {
    let get = |keys: &[&str]| {
        keys.iter().find_map(|k| {
            metadata
                .iter()
                .find(|(key, v)| key.eq_ignore_ascii_case(k) && !v.trim().is_empty())
                .map(|(_, v)| normalize_text(v))
        })
    };

    let price_text = get(&["product:price:amount", "og:price:amount"]);
    let declared = price_text.is_some()
        || get(&["og:type"]).is_some_and(|t| t.to_lowercase().contains("product"));

    let mut images = Vec::new();
    for (key, value) in metadata {
        if matches!(
            key.to_ascii_lowercase().as_str(),
            "og:image" | "og:image:secure_url" | "og:image:url"
        ) {
            images.extend(resolve(value.trim(), base));
        }
    }

    let product = ExtractProductResult {
        name: get(&["og:title", "twitter:title", "title"]),
        url: get(&["og:url"])
            .and_then(|u| resolve(&u, base))
            .or_else(|| Some(page_url.to_string())),
        brand: get(&["product:brand", "og:brand"]),
        description: get(&["og:description", "description"]),
        price: price_text
            .as_deref()
            .and_then(parse_price)
            .filter(|p| *p > 0.0),
        currency: get(&["product:price:currency", "og:price:currency"]),
        availability: get(&["product:availability", "og:availability"])
            .map(|a| og_availability(&a)),
        images: dedupe(images),
        ..Default::default()
    };
    OgProduct { product, declared }
}

/// Open Graph availability (`instock`, `in stock`, `oos`, …) → schema.org name.
fn og_availability(value: &str) -> String {
    let compact: String = value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    match compact.as_str() {
        "instock" | "available" => "InStock".to_string(),
        "oos" | "outofstock" => "OutOfStock".to_string(),
        "preorder" => "PreOrder".to_string(),
        "backorder" => "BackOrder".to_string(),
        _ => last_segment(value).unwrap_or_else(|| value.to_string()),
    }
}

fn fill_missing(product: &mut ExtractProductResult, og: ExtractProductResult) {
    product.name = product.name.take().or(og.name);
    product.brand = product.brand.take().or(og.brand);
    product.description = product.description.take().or(og.description);
    if product.price.is_none() {
        product.price = og.price;
        product.currency = product.currency.take().or(og.currency);
    }
    product.availability = product.availability.take().or(og.availability);
    if product.images.is_empty() {
        product.images = og.images;
    }
}

// ---------------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------------

/// A value as a list: arrays as-is, a single value as one item, `None` as none.
fn as_items(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(v) => vec![v],
    }
}

/// Non-empty normalized text from a string / number (first of an array).
fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(normalize_text(s)).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(items) => items.iter().find_map(|v| text(Some(v))),
        _ => None,
    }
}

/// Text, or an entity's `name` (`brand: {"@type": "Brand", "name": "…"}`).
fn name_of(value: Option<&Value>) -> Option<String> {
    match value? {
        obj @ Value::Object(_) => text(obj.get("name")),
        Value::Array(items) => items.iter().find_map(|v| name_of(Some(v))),
        other => text(Some(other)),
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_price(s),
        _ => None,
    }
}

/// Last IRI segment: `https://schema.org/InStock` → `InStock`.
fn last_segment(value: &str) -> Option<String> {
    value
        .rsplit(['/', '#', ':'])
        .find(|s| !s.is_empty())
        .map(str::to_string)
}

/// Image URLs from a string, an `ImageObject`, or an array of either.
fn collect_images(value: Option<&Value>, base: Option<&Url>, out: &mut Vec<String>) {
    for item in as_items(value) {
        let src = match item {
            Value::String(s) => Some(s.clone()),
            obj @ Value::Object(_) => text(obj.get("url")).or_else(|| text(obj.get("contentUrl"))),
            _ => None,
        };
        out.extend(src.and_then(|s| resolve(s.trim(), base)));
    }
}

/// Absolute http(s) URL, resolving relative / protocol-relative references.
fn resolve(href: &str, base: Option<&Url>) -> Option<String> {
    let url = match base {
        Some(base) => base.join(href).ok()?,
        None => Url::parse(href).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    crate::dedupe!(items)
}

/// Plain text from an HTML-bearing description.
fn strip_tags(text: &str) -> String {
    if !text.contains('<') {
        return text.to_string();
    }
    let fragment = scraper::Html::parse_fragment(text);
    normalize_text(&fragment.root_element().text().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::parse_price;

    #[test]
    fn parses_price_formats() {
        assert_eq!(parse_price("1299"), Some(1299.0));
        assert_eq!(parse_price("$1,299.00"), Some(1299.0));
        assert_eq!(parse_price("1.299,00 €"), Some(1299.0));
        assert_eq!(parse_price("12,5"), Some(12.5));
        assert_eq!(parse_price("1,299"), Some(1299.0));
        assert_eq!(parse_price("1.299.000"), Some(1299000.0));
        assert_eq!(parse_price("19.99"), Some(19.99));
        assert_eq!(parse_price("USD 45.00 - 60.00"), Some(45.0));
        assert_eq!(parse_price("free"), None);
        assert_eq!(parse_price(""), None);
    }
}
