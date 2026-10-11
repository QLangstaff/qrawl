use crate::tools::scrape::jsonld_entities;
use crate::utils::LINK_SELECTOR;
use crate::utils::{canonical_domain, schema_types};
use scraper::{ElementRef, Html};
use serde_json::Value;
use std::cmp::Reverse;
use url::Url;

/// Minimum number of siblings required to form a valid group.
const MIN_SIBLING_GROUP_SIZE: usize = 3;

/// Minimum common prefix length for matching single-element patterns.
const MIN_COMMON_PREFIX_LEN: usize = 2;

/// Pattern length value for single-element patterns.
const SINGLE_ELEMENT_PATTERN_LEN: usize = 1;

/// Minimum pattern length for multi-element pattern detection.
const MIN_PATTERN_LEN: usize = 2;

/// Maximum pattern length as a ratio of total children (e.g., 2 = half).
const MAX_PATTERN_RATIO: usize = 2;

/// Longest run of sibling elements that counts as one repeating item, and how
/// many candidate lengths are scanned, so a page of thousands of siblings
/// stays cheap.
const MAX_PATTERN_LEN: usize = 64;
const MAX_PATTERN_CANDIDATES: usize = 8;
/// A candidate unit length has at least one in this many of the repeats the
/// most repeated length has. A unit's length is where most of a listing's
/// structures repeat; a length only a few repeat at is the cycle of something
/// between the items (an ad after every third item), which would outrank the
/// unit by covering the items and the ads.
const MIN_UNIT_REPEATS_SHARE: usize = 4;

/// HTML tag name for main content elements.
const MAIN_TAG: &str = "main";

/// HTML tags to exclude from pattern detection (non-content elements).
const JUNK_TAGS: &[&str] = &["script", "style", "iframe", "noscript"];

/// HTML tags that indicate navigation/non-main-content (should be deprioritized).
const NAV_TAGS: &[&str] = &["nav", "footer", "aside", "header"];

/// Structure pattern for sibling detection: an element's child tag names, so
/// a list of `<h2>` and `<h3>` items is one list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct StructurePattern {
    pub tags: Vec<String>,
}

/// A group of sibling elements with the same pattern.
///
/// Groups are scored and compared to find the "best" sibling group on a page.
/// Scoring hierarchy: !in_navigation > in_main > coverage > quantity > pattern_len
#[derive(Debug)]
struct SiblingGroup {
    /// Whether the group is inside <main> tag (highest priority for content).
    in_main: bool,
    /// Whether the group is inside navigation tags (nav/footer/aside/header) - these are excluded.
    in_navigation: bool,
    /// Number of elements in the repeating pattern (higher = richer pattern).
    pattern_len: usize,
    /// The actual HTML fragments of the siblings.
    siblings: Vec<String>,
}

impl SiblingGroup {
    /// Calculate coverage score (pattern richness × quantity).
    fn coverage(&self) -> usize {
        self.pattern_len * self.siblings.len()
    }

    /// Get quantity of siblings in group.
    fn quantity(&self) -> usize {
        self.siblings.len()
    }
}

/// Map child URLs from HTML siblings.
///
/// Detects sibling patterns in HTML structure and extracts the first URL from each sibling.
#[cfg(test)]
pub(super) fn map_siblings(html: &str, url: &str) -> Vec<String> {
    let siblings = map_body_siblings(html);
    map_sibling_link(&siblings, url)
}

/// Map child URLs from HTML siblings using a pre-parsed document.
pub(super) fn map_siblings_from_doc(doc: &Html, url: &str) -> Vec<String> {
    let siblings = map_body_siblings_from_doc(doc);
    map_sibling_link(&siblings, url)
}

/// Map child URLs from JSON-LD ItemList.
///
/// Extracts ItemList from JSON-LD and resolves URLs (including anchor references).
#[cfg(test)]
pub(super) fn map_itemlist(html: &str, url: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let itemlist = map_jsonld_itemlist_from_doc(&doc);
    map_itemlist_link(&itemlist, &doc, url)
}

/// Map child URLs from JSON-LD ItemList using a pre-parsed document.
pub(super) fn map_itemlist_from_doc(doc: &Html, url: &str) -> Vec<String> {
    let itemlist = map_jsonld_itemlist_from_doc(doc);
    map_itemlist_link(&itemlist, doc, url)
}

/// Map body content to sibling HTML fragments.
///
/// Detects repeating sibling patterns in HTML structure by scanning
/// the entire DOM tree and finding the richest sibling group.
///
/// # Detection Algorithm
///
/// 1. **Scan**: Recursively traverse DOM to find all groups of repeating siblings
/// 2. **Score**: Rank groups using hierarchical criteria (see below)
/// 3. **Select**: Return the highest-scoring group
///
/// # Scoring Hierarchy (highest to lowest priority)
///
/// 1. **Not in navigation** - Excludes groups in `<nav>`, `<footer>`, `<aside>`, `<header>`
/// 2. **In main tag** - Groups inside `<main>` tag win (semantic main content)
/// 3. **Coverage** - pattern_len × quantity (richer patterns preferred)
/// 4. **Quantity** - More siblings preferred
/// 5. **Pattern length** - Longer patterns preferred (richer structure)
///
#[cfg(test)]
pub(super) fn map_body_siblings(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    map_body_siblings_from_doc(&doc)
}

/// Same as `map_body_siblings` but reuses an already-parsed document.
pub(super) fn map_body_siblings_from_doc(doc: &Html) -> Vec<String> {
    let root = doc.root_element();

    // Scan entire tree and find ALL sibling groups at ALL levels
    let mut all_sibling_groups: Vec<SiblingGroup> = Vec::new();
    map_sibling_groups_recursive(&root, &mut all_sibling_groups);

    // Select best group using scoring hierarchy
    let selected = all_sibling_groups.into_iter().max_by_key(|group| {
        (
            !group.in_navigation, // Exclude navigation/footer first
            group.in_main,        // Prefer <main> content
            group.coverage(),     // Prefer richer patterns (pattern_len × quantity)
            group.quantity(),     // Prefer more siblings
            group.pattern_len,    // Prefer longer patterns
        )
    });

    selected.map(|group| group.siblings).unwrap_or_default()
}

/// Clean href by stripping escape sequences, quotes, and whitespace.
/// Handles malformed HTML where hrefs have literal quote characters or escape sequences.
fn clean_href(href: &str) -> String {
    // Remove backslashes, HTML entities for quotes, and quotes, then trim
    href.replace('\\', "")
        .replace("&quot;", "")
        .replace("&#34;", "")
        .replace("&apos;", "")
        .replace("&#39;", "")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// The `http(s)` URL an `href` points to from `base`: absolute, relative, or
/// protocol-relative (`//host/path`). `None` for any other scheme
/// (`mailto:`, `javascript:`).
pub(super) fn resolve_href(href: &str, base: &Url) -> Option<Url> {
    let url = base.join(&clean_href(href)).ok()?;
    is_valid_scheme(&url).then_some(url)
}

/// Check if URL scheme is acceptable (http/https).
fn is_valid_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// Parse the page URL into the base that relative hrefs resolve against.
/// `None` (with a warning) when it can't be parsed — the caller has no base to
/// resolve against and yields no links.
fn parse_base_url(url: &str) -> Option<Url> {
    Url::parse(url)
        .map_err(|e| log::warn!("invalid URL '{}': {}", url, e))
        .ok()
}

/// Check if element is inside a specific HTML tag.
fn is_inside_tag(element: &ElementRef, tag_name: &str) -> bool {
    let mut ancestor = element.parent();
    while let Some(node) = ancestor {
        if let Some(elem) = ElementRef::wrap(node) {
            if elem.value().name() == tag_name {
                return true;
            }
        }
        ancestor = node.parent();
    }
    false
}

/// Map structure pattern from element (ordered list of child tag names).
fn map_structure_pattern(element: &ElementRef) -> StructurePattern {
    let tags: Vec<String> = element
        .children()
        .filter_map(ElementRef::wrap)
        .map(|e| e.value().name().to_string())
        .collect();
    StructurePattern { tags }
}

/// Recursively scan for sibling groups in DOM tree.
///
/// Finds repeating patterns at each level by:
/// 1. Detecting single-element patterns (e.g., repeated <div> with same child structure)
/// 2. Detecting multi-element patterns (e.g., repeated <h3><p><a> sequences)
/// 3. Recursing into children to scan deeper levels
///
/// Each discovered group is added to `all_groups` for later scoring.
fn map_sibling_groups_recursive<'a>(
    element: &'a ElementRef<'a>,
    all_groups: &mut Vec<SiblingGroup>,
) {
    // Get children at this level (filter junk)
    let children: Vec<_> = element
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|child| {
            let tag = child.value().name();
            !JUNK_TAGS.contains(&tag)
        })
        .collect();

    if children.len() >= MIN_SIBLING_GROUP_SIZE {
        // 1. Detect single-element patterns with common-prefix matching
        let mut pattern_groups: Vec<(Vec<String>, Vec<usize>)> = Vec::new();

        for (idx, child) in children.iter().enumerate() {
            let pattern = map_structure_pattern(child);

            // Find existing group with compatible pattern (shares common prefix)
            let mut matched = false;
            for (group_tags, indices) in pattern_groups.iter_mut() {
                // Check if patterns share a common prefix of at least 2 elements
                let min_len = group_tags.len().min(pattern.tags.len());
                if min_len >= MIN_COMMON_PREFIX_LEN
                    && group_tags[..min_len] == pattern.tags[..min_len]
                {
                    indices.push(idx);
                    // Update group to use shortest pattern (core pattern)
                    if pattern.tags.len() < group_tags.len() {
                        *group_tags = pattern.tags.clone();
                    }
                    matched = true;
                    break;
                }
            }

            if !matched {
                pattern_groups.push((pattern.tags.clone(), vec![idx]));
            }
        }

        // Convert to sibling groups, filtering out trivial patterns
        for (tags, indices) in pattern_groups {
            if indices.len() >= MIN_SIBLING_GROUP_SIZE && !tags.is_empty() {
                let siblings: Vec<String> = indices.iter().map(|&i| children[i].html()).collect();

                if siblings.len() >= MIN_SIBLING_GROUP_SIZE {
                    let first_child = &children[indices[0]];
                    all_groups.push(SiblingGroup {
                        in_main: is_inside_tag(first_child, MAIN_TAG),
                        in_navigation: NAV_TAGS.iter().any(|tag| is_inside_tag(first_child, tag)),
                        pattern_len: SINGLE_ELEMENT_PATTERN_LEN,
                        siblings,
                    });
                }
            }
        }

        // 2. Detect multi-element patterns
        map_multi_element_patterns(&children, all_groups);
    }

    // Recurse into ALL children to scan deeper levels
    for child in children {
        map_sibling_groups_recursive(&child, all_groups);
    }
}

/// Detect multi-element repeating patterns.
///
/// Searches for sequences like `<h3><p><a>` that repeat multiple times.
/// Tries pattern lengths from MIN_PATTERN_LEN up to n/MAX_PATTERN_RATIO.
///
/// Handles overlapping patterns by selecting non-overlapping instances.
fn map_multi_element_patterns(children: &[ElementRef], all_groups: &mut Vec<SiblingGroup>) {
    use std::collections::HashMap;

    let n = children.len();
    let structures: Vec<StructurePattern> = children.iter().map(map_structure_pattern).collect();

    // A unit of `p` elements repeats its structures `p` apart, so the distance
    // from a sibling to the next with its structure is a unit's length; the
    // most frequent few are scanned, in a fixed order. Only the last sibling
    // of a run of equal structures counts: inside a run (a unit's filler
    // paragraphs), every short distance matches, which would outscore the
    // unit's own length. Only the next repeat counts: a repeat further on is
    // the length of several units, which an ad between items or two item
    // lengths would make frequent, and which would outrank the unit itself.
    let ends_a_run = |idx: usize| structures.get(idx + 1) != Some(&structures[idx]);
    let mut last_seen: HashMap<&StructurePattern, usize> = HashMap::new();
    let mut distances: HashMap<usize, usize> = HashMap::new();
    for (idx, structure) in structures
        .iter()
        .enumerate()
        .filter(|(idx, _)| ends_a_run(*idx))
    {
        if let Some(previous) = last_seen.insert(structure, idx) {
            *distances.entry(idx - previous).or_default() += 1;
        }
    }
    let mut candidates: Vec<(usize, usize)> = distances
        .into_iter()
        .filter(|(len, _)| {
            (MIN_PATTERN_LEN..=(n / MAX_PATTERN_RATIO).min(MAX_PATTERN_LEN)).contains(len)
                && n >= len * MIN_SIBLING_GROUP_SIZE
        })
        .collect();
    let top_repeats = candidates
        .iter()
        .map(|(_, repeats)| *repeats)
        .max()
        .unwrap_or(0);
    candidates.retain(|(_, repeats)| repeats * MIN_UNIT_REPEATS_SHARE >= top_repeats);
    candidates.sort_by_key(|(len, repeats)| (std::cmp::Reverse(*repeats), *len));
    candidates.truncate(MAX_PATTERN_CANDIDATES);

    for (pattern_len, _) in candidates {
        let mut multi_pattern_groups: HashMap<&[StructurePattern], Vec<usize>> = HashMap::new();

        // Scan through all possible positions
        let mut idx = 0;
        while idx + pattern_len <= n {
            let pattern = &structures[idx..idx + pattern_len];

            multi_pattern_groups.entry(pattern).or_default().push(idx);
            idx += 1;
        }

        // Each multi-element pattern that repeats `MIN_SIBLING_GROUP_SIZE` times is a group
        // In order of first appearance, so equal-scoring groups always rank
        // the same way.
        let mut pattern_groups: Vec<_> = multi_pattern_groups.into_iter().collect();
        pattern_groups.sort_by_key(|(_, start_indices)| start_indices[0]);
        // Each group kept, by the rotation its structures are a cyclic shift
        // of, to tell a unit from a rotation of it: the same unit read from
        // another of its elements on (`div, p, p, h2` for `h2, div, p, p`).
        // Of the two, the unit is the one whose elements keep their tags in
        // every instance: a rotation's last element is the unit's first in
        // most instances, and whatever follows the listing in the last. Then
        // the one with the longer run of adjacent instances; a rotation never
        // trades instances for tags, so a listing whose first heading differs
        // keeps its first item. Then the one with fewer instances standing
        // alone: a rotation picks up stray instances from another listing's
        // elements, while a listing split by a block of other elements keeps
        // its runs. When all tie, the one that starts first. A
        // listing whose first item alone uses another heading tag, followed by
        // a link under the items' tag, reads as the rotation: its tags can't
        // tell it from the unit; so can a lone item before a block of other
        // elements, since a run of one looks like a stray. An item with an
        // extra element that has children matches no group and is lost alone.
        let mut kept: HashMap<Vec<StructurePattern>, KeptGroup> = HashMap::new();
        for (pattern, start_indices) in pattern_groups {
            if start_indices.len() >= MIN_SIBLING_GROUP_SIZE {
                // Skip homogeneous patterns (handled by single-element detection)
                let first = &pattern[0];
                if pattern.iter().all(|p| p == first) {
                    continue;
                }

                // Filter out overlapping instances
                let mut non_overlapping: Vec<usize> = Vec::new();
                for &idx in &start_indices {
                    let overlaps = non_overlapping.iter().any(|&selected_idx| {
                        let range1 = selected_idx..(selected_idx + pattern_len);
                        let range2 = idx..(idx + pattern_len);
                        range1.contains(&idx)
                            || range1.contains(&(idx + pattern_len - 1))
                            || range2.contains(&selected_idx)
                            || range2.contains(&(selected_idx + pattern_len - 1))
                    });

                    if !overlaps {
                        non_overlapping.push(idx);
                    }
                }

                if non_overlapping.len() >= MIN_SIBLING_GROUP_SIZE {
                    let first = non_overlapping[0];
                    let tag_consistency = (0..pattern_len)
                        .filter(|&offset| {
                            let tag = children[first + offset].value().name();
                            non_overlapping
                                .iter()
                                .all(|&start| children[start + offset].value().name() == tag)
                        })
                        .count();
                    // An instance is adjacent to the one before when it starts where
                    // that one ends, or less than a unit after it with only the
                    // unit's own structures between: one listing's items can
                    // differ by some elements, and an ad shaped like one passes.
                    let runs = adjacent_runs(&non_overlapping, |previous, start| {
                        (pattern_len..2 * pattern_len).contains(&(start - previous))
                            && structures[previous + pattern_len..start]
                                .iter()
                                .all(|structure| pattern.contains(structure))
                    });
                    let adjacent_run = runs.iter().copied().max().unwrap_or(0);
                    let stragglers = runs.iter().filter(|&&run| run == 1).count();
                    let rotation = canonical_rotation(pattern);
                    let replaces = match kept.get(&rotation) {
                        Some(group)
                            if group.adjacent_run <= adjacent_run
                                && group.rank()
                                    < (tag_consistency, adjacent_run, Reverse(stragglers)) =>
                        {
                            Some(group.group_index)
                        }
                        Some(_) => continue,
                        None => None,
                    };
                    let siblings: Vec<String> = non_overlapping
                        .iter()
                        .map(|&start_idx| {
                            (0..pattern_len)
                                .map(|offset| children[start_idx + offset].html())
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .collect();

                    let first_child = &children[first];
                    let group = SiblingGroup {
                        in_main: is_inside_tag(first_child, MAIN_TAG),
                        in_navigation: NAV_TAGS.iter().any(|tag| is_inside_tag(first_child, tag)),
                        pattern_len,
                        siblings,
                    };
                    let group_index = match replaces {
                        Some(group_index) => {
                            all_groups[group_index] = group;
                            group_index
                        }
                        None => {
                            all_groups.push(group);
                            all_groups.len() - 1
                        }
                    };
                    kept.insert(
                        rotation,
                        KeptGroup {
                            adjacent_run,
                            stragglers,
                            tag_consistency,
                            group_index,
                        },
                    );
                }
            }
        }
    }
}

/// A multi-element group of one parent, as far as telling its rotations apart
/// needs: its longest run of adjacent instances, its instances standing alone,
/// how many of its positions keep one tag across every instance, and where it
/// sits in `all_groups`.
struct KeptGroup {
    adjacent_run: usize,
    stragglers: usize,
    tag_consistency: usize,
    group_index: usize,
}

impl KeptGroup {
    /// What ranks a group above a rotation of it, best first.
    fn rank(&self) -> (usize, usize, Reverse<usize>) {
        (
            self.tag_consistency,
            self.adjacent_run,
            Reverse(self.stragglers),
        )
    }
}

/// The least of a pattern's cyclic shifts: one key for a unit and every
/// rotation of it.
fn canonical_rotation(pattern: &[StructurePattern]) -> Vec<StructurePattern> {
    (0..pattern.len())
        .map(|shift| {
            pattern
                .iter()
                .cycle()
                .skip(shift)
                .take(pattern.len())
                .cloned()
                .collect::<Vec<_>>()
        })
        .min()
        .expect("a pattern has an element")
}

/// The lengths of the runs of instances, each `is_adjacent` to the one before;
/// an instance that isn't starts a run of its own.
fn adjacent_runs(starts: &[usize], is_adjacent: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let mut runs = Vec::new();
    for (index, &start) in starts.iter().enumerate() {
        match index.checked_sub(1) {
            Some(previous) if is_adjacent(starts[previous], start) => {
                *runs.last_mut().expect("a run before an adjacent instance") += 1
            }
            _ => runs.push(1),
        }
    }
    runs
}

/// Map sibling HTML fragment to URL.
///
/// Finds the first valid HTTP(S) link in each sibling fragment.
///
/// # Performance Note
/// Parses each sibling HTML fragment individually. This is acceptable because:
/// - Fragments are small (individual sibling elements, not full pages)
/// - Parsing overhead is minimal compared to network I/O
/// - Alternative (keeping ElementRefs) would require major API refactor
pub(super) fn map_sibling_link(siblings: &[String], url: &str) -> Vec<String> {
    let Some(base) = parse_base_url(url) else {
        return Vec::new();
    };

    siblings
        .iter()
        .filter_map(|html| {
            let doc = Html::parse_fragment(html);
            select_primary_link_in_document(&doc, &base)
        })
        .collect()
}

/// The `ItemList` entities a document's JSON-LD declares, at the top or under
/// a page's `mainEntity`, read by the same reader as `scrape_jsonld`.
pub(super) fn map_jsonld_itemlist_from_doc(doc: &Html) -> Vec<Value> {
    let mut itemlists = Vec::new();
    for entity in jsonld_entities(doc) {
        collect_itemlists(&entity, &mut itemlists);
    }
    itemlists
}

fn collect_itemlists(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(arr) => {
            for item in arr {
                collect_itemlists(item, out);
            }
        }
        Value::Object(obj) => {
            if schema_types(value)
                .iter()
                .any(|schema_type| schema_type.eq_ignore_ascii_case("ItemList"))
            {
                out.push(Value::Object(obj.clone()));
            }
            // A nested `@graph` is data to the reader, which opens the top one.
            for key in ["mainEntity", "@graph"] {
                if let Some(nested) = obj.get(key) {
                    collect_itemlists(nested, out);
                }
            }
        }
        _ => {}
    }
}

/// Map ItemList items to URLs, resolving anchors to real links.
///
/// Handles three cases:
/// 1. Full external URLs - Return as-is
/// 2. Anchor references (#id) - Find element and extract link
/// 3. Relative URLs - Resolve to absolute
pub(super) fn map_itemlist_link(itemlist: &[Value], doc: &Html, url: &str) -> Vec<String> {
    let Some(base) = parse_base_url(url) else {
        return Vec::new();
    };

    itemlist
        .iter()
        .filter_map(|item| {
            let elements = item.get("itemListElement")?.as_array()?;

            Some(
                elements
                    .iter()
                    .filter_map(|elem| {
                        let url_str = elem.get("url")?.as_str()?;

                        // Case 1: Anchor reference (#id)
                        if let Some(anchor_id) = url_str.strip_prefix('#') {
                            if let Some(resolved) = map_anchor_to_link(anchor_id, doc, &base) {
                                return Some(resolved);
                            }
                            return None;
                        }

                        // Case 2: a link to an anchor of this same page stands
                        // for the link inside that anchor's element
                        let url = resolve_href(url_str, &base)?;
                        let is_this_page = url.path() == base.path()
                            && url.query() == base.query()
                            && url.port_or_known_default() == base.port_or_known_default()
                            && url.host_str().map(canonical_domain)
                                == base.host_str().map(canonical_domain);
                        match url.fragment() {
                            Some(fragment) if is_this_page => {
                                let anchor_id = urlencoding::decode(fragment).ok()?;
                                map_anchor_to_link(&anchor_id, doc, &base)
                            }
                            // Case 3: any other URL, absolute or relative
                            _ => Some(url.to_string()),
                        }
                    })
                    .collect::<Vec<String>>(),
            )
        })
        .flatten()
        .collect()
}

/// Map anchor ID to real URL by finding element and extracting link.
///
/// # Performance Note
/// Selector must be dynamically created per anchor_id (cannot reuse a static Lazy value).
/// This is acceptable because anchor resolution is rare compared to other operations.
fn map_anchor_to_link(anchor_id: &str, doc: &Html, base: &Url) -> Option<String> {
    let element = doc
        .root_element()
        .descendants()
        .filter_map(ElementRef::wrap)
        .find(|element| element.value().id() == Some(anchor_id))?;
    select_primary_link_in_element(&element, base)
}

fn is_heading_link(link: &ElementRef, text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }

    let tag = link.value().name();
    if matches!(tag, "h1" | "h2" | "h3" | "h4") {
        return true;
    }

    for heading in ["h1", "h2", "h3", "h4"].iter() {
        if is_inside_tag(link, heading) {
            return true;
        }
    }

    for descendant in link.descendants() {
        if let Some(elem) = ElementRef::wrap(descendant) {
            match elem.value().name() {
                // `text` is already known non-blank (checked at entry).
                "h1" | "h2" | "h3" | "h4" | "strong" | "b" => return true,
                _ => {}
            }
        }
    }

    is_inside_tag(link, "strong") || is_inside_tag(link, "b")
}

fn is_utility_text(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "share"
            | "print"
            | "save"
            | "pin"
            | "email"
            | "tweet"
            | "facebook"
            | "pinterest"
            | "linkedin"
            | "reddit"
            | "copy link"
            | "comment"
            | "buy"
    )
}

fn match_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn collect_heading_texts(element: &ElementRef) -> Vec<String> {
    element
        .descendants()
        .filter_map(ElementRef::wrap)
        .filter(|el| matches!(el.value().name(), "h1" | "h2" | "h3" | "h4"))
        .map(|el| match_text(&el.text().collect::<String>()))
        .filter(|text| !text.is_empty())
        .collect()
}

fn link_matches_heading(link_text_norm: &str, headings: &[String]) -> bool {
    headings.iter().any(|h| {
        !h.is_empty()
            && !link_text_norm.is_empty()
            && (link_text_norm == *h || link_text_norm.contains(h) || h.contains(link_text_norm))
    })
}

fn select_primary_link_in_element(element: &ElementRef, base: &Url) -> Option<String> {
    let headings = collect_heading_texts(element);
    let mut primary_text: Option<String> = None;
    let mut fallback: Option<String> = None;
    let mut heading_links: Vec<(String, String)> = Vec::new(); // (url, text) for heading links

    // Collect links and categorize them
    for link in element.select(&LINK_SELECTOR) {
        let Some(url) = link
            .value()
            .attr("href")
            .and_then(|href| resolve_href(href, base))
        else {
            continue;
        };

        if fallback.is_none() {
            fallback = Some(url.to_string());
        }

        let link_text = link.text().collect::<String>();
        let link_match_text = match_text(&link_text);
        let is_heading =
            is_heading_link(&link, &link_text) || link_matches_heading(&link_match_text, &headings);
        let has_content = !link_text.trim().is_empty() && !is_utility_text(&link_text);

        if is_heading {
            heading_links.push((url.to_string(), link_match_text.clone()));
        }

        if primary_text.is_none() && has_content {
            primary_text = Some(url.to_string());
        }
    }

    // Select heading link using deterministic priority matching
    let heading_link = match heading_links.len() {
        0 => None,
        1 => Some(heading_links[0].0.clone()),
        _ => {
            // Multiple heading links: use deterministic priority matching
            // Priority 1: Perfect match (link text == heading)
            for (url, link_text) in &heading_links {
                for h in &headings {
                    if link_text == h {
                        return Some(url.clone());
                    }
                }
            }

            // Priority 2: Link contains heading (more specific)
            for (url, link_text) in &heading_links {
                for h in &headings {
                    if !h.is_empty() && link_text.contains(h) {
                        return Some(url.clone());
                    }
                }
            }

            // Priority 3: Heading contains link (less specific)
            for (url, link_text) in &heading_links {
                for h in &headings {
                    if !link_text.is_empty() && h.contains(link_text) {
                        return Some(url.clone());
                    }
                }
            }

            // Fallback: return last heading link
            heading_links.last().map(|(url, _)| url.clone())
        }
    };

    heading_link.or(primary_text).or(fallback)
}

fn select_primary_link_in_document(doc: &Html, base: &Url) -> Option<String> {
    for node in doc.tree.nodes() {
        if let Some(element) = ElementRef::wrap(node) {
            if let Some(link) = select_primary_link_in_element(&element, base) {
                return Some(link);
            }
        }
    }
    None
}
