use crate::selectors::{JSONLD_SELECTOR, LINK_SELECTOR};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
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

/// HTML tag name for main content elements.
const MAIN_TAG: &str = "main";

/// HTML tags to exclude from pattern detection (non-content elements).
const JUNK_TAGS: &[&str] = &["script", "style", "iframe", "noscript"];

/// HTML tags that indicate navigation/non-main-content (should be deprioritized).
const NAV_TAGS: &[&str] = &["nav", "footer", "aside", "header"];

/// Structure pattern for sibling detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
/// Domain filtering happens during detection to affect group selection.
pub(super) fn map_siblings(html: &str, base_url: &str) -> Vec<String> {
    let siblings = map_body_siblings(html);
    map_sibling_link(&siblings, base_url)
}

/// Map child URLs from JSON-LD ItemList.
///
/// Extracts ItemList from JSON-LD and resolves URLs (including anchor references).
pub(super) fn map_itemlist(html: &str, base_url: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let itemlist = map_jsonld_itemlist_from_doc(&doc);
    map_itemlist_link(&itemlist, &doc, base_url)
}

/// Map body content to sibling HTML fragments.
///
/// Detects repeating sibling patterns in HTML structure by scanning
/// the entire DOM tree and finding the richest sibling group.
///
/// # Detection Algorithm
///
/// 1. **Scan**: Recursively traverse DOM to find all groups of repeating siblings
/// 2. **Filter**: Apply domain filters to each group (removes blocked URLs)
/// 3. **Score**: Rank groups using hierarchical criteria (see below)
/// 4. **Select**: Return the highest-scoring group
///
/// # Scoring Hierarchy (highest to lowest priority)
///
/// 1. **Not in navigation** - Excludes groups in `<nav>`, `<footer>`, `<aside>`, `<header>`
/// 2. **In main tag** - Groups inside `<main>` tag win (semantic main content)
/// 3. **Coverage** - pattern_len × quantity (richer patterns preferred)
/// 4. **Quantity** - More siblings preferred
/// 5. **Pattern length** - Longer patterns preferred (richer structure)
///
/// # Domain Filtering
///
/// Domain filtering happens during detection to affect group selection.
/// Groups with only blocked domains are excluded before scoring.
///
pub(super) fn map_body_siblings(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
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

/// Check if URL scheme is acceptable (http/https).
fn is_valid_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
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

    // Try pattern lengths from MIN_PATTERN_LEN up to n/MAX_PATTERN_RATIO
    for pattern_len in MIN_PATTERN_LEN..=(n / MAX_PATTERN_RATIO) {
        if n < pattern_len * MIN_SIBLING_GROUP_SIZE {
            break;
        }

        let mut multi_pattern_groups: HashMap<Vec<StructurePattern>, Vec<usize>> = HashMap::new();

        // Scan through all possible positions
        let mut idx = 0;
        while idx + pattern_len <= n {
            let pattern: Vec<StructurePattern> = (0..pattern_len)
                .map(|offset| map_structure_pattern(&children[idx + offset]))
                .collect();

            multi_pattern_groups.entry(pattern).or_default().push(idx);
            idx += 1;
        }

        // For each multi-element pattern, if it appears 2+ times, add to groups
        for (pattern, start_indices) in multi_pattern_groups {
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
                    let siblings: Vec<String> = non_overlapping
                        .iter()
                        .map(|&start_idx| {
                            (0..pattern_len)
                                .map(|offset| children[start_idx + offset].html())
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .collect();

                    if siblings.len() >= MIN_SIBLING_GROUP_SIZE {
                        let first_child = &children[non_overlapping[0]];
                        all_groups.push(SiblingGroup {
                            in_main: is_inside_tag(first_child, MAIN_TAG),
                            in_navigation: NAV_TAGS
                                .iter()
                                .any(|tag| is_inside_tag(first_child, tag)),
                            pattern_len,
                            siblings,
                        });
                    }
                }
            }
        }
    }
}

/// Map sibling HTML fragment to URL.
///
/// Finds the FIRST valid HTTP(S) link in each sibling fragment that passes domain filters.
/// Skips excluded domains to avoid returning Pinterest/TikTok/etc share buttons.
///
/// # Performance Note
/// Parses each sibling HTML fragment individually. This is acceptable because:
/// - Fragments are small (individual sibling elements, not full pages)
/// - Parsing overhead is minimal compared to network I/O
/// - Alternative (keeping ElementRefs) would require major API refactor
pub(super) fn map_sibling_link(siblings: &[String], base_url: &str) -> Vec<String> {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Warning: Invalid base URL '{}': {}", base_url, e);
            return Vec::new();
        }
    };

    siblings
        .iter()
        .filter_map(|html| {
            let doc = Html::parse_fragment(html);
            select_primary_link_in_document(&doc, &base)
        })
        .collect()
}

/// Map JSON-LD script tags to ItemList objects from parsed HTML document.
pub(super) fn map_jsonld_itemlist_from_doc(doc: &Html) -> Vec<Value> {
    let mut itemlists = Vec::new();

    for script in doc.select(&JSONLD_SELECTOR) {
        let json_str = script.inner_html();
        if let Ok(value) = serde_json::from_str::<Value>(&json_str) {
            collect_itemlists(&value, &mut itemlists);
        }
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
            if obj
                .get("@type")
                .and_then(Value::as_str)
                .map(|t| t.eq_ignore_ascii_case("ItemList"))
                .unwrap_or(false)
            {
                out.push(Value::Object(obj.clone()));
            }

            if let Some(graph) = obj.get("@graph") {
                collect_itemlists(graph, out);
            }

            if let Some(main_entity) = obj.get("mainEntity") {
                collect_itemlists(main_entity, out);
            }
        }
        _ => {}
    }
}

/// Map ItemList items to URLs, resolving anchors to real links and filtering by domain.
///
/// Handles three cases:
/// 1. Full external URLs - Return as-is
/// 2. Anchor references (#id) - Find element and extract link
/// 3. Relative URLs - Resolve to absolute
pub(super) fn map_itemlist_link(itemlist: &[Value], doc: &Html, base_url: &str) -> Vec<String> {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Warning: Invalid base URL '{}': {}", base_url, e);
            return Vec::new();
        }
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

                        // Case 2: Absolute URL
                        if let Ok(url) = Url::parse(url_str) {
                            if is_valid_scheme(&url) {
                                if let Some(fragment) = url.fragment() {
                                    // Compare hosts with canonicalization (strips www., lowercases, etc.)
                                    let hosts_match = match (url.host_str(), base.host_str()) {
                                        (Some(url_host), Some(base_host)) => {
                                            use crate::tools::clean::utils::canonicalize_domain;
                                            canonicalize_domain(url_host)
                                                == canonicalize_domain(base_host)
                                        }
                                        _ => false,
                                    };

                                    if url.scheme() == base.scheme() && hosts_match {
                                        if let Some(resolved) =
                                            map_anchor_to_link(fragment, doc, &base)
                                        {
                                            return Some(resolved);
                                        }
                                        return None;
                                    }
                                }
                                return Some(url.to_string());
                            }
                        }

                        // Case 3: Relative URL
                        base.join(url_str)
                            .ok()
                            .filter(is_valid_scheme)
                            .map(|u| u.to_string())
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
    // Dynamic selector - necessary because anchor_id is runtime data
    let selector = Selector::parse(&format!("[id='{}']", anchor_id)).ok()?;
    let element = doc.select(&selector).next()?;
    select_primary_link_in_element(&element, base)
}

fn has_meaningful_text(text: &str) -> bool {
    !text.trim().is_empty()
}

fn is_heading_link(link: &ElementRef, text: &str) -> bool {
    if !has_meaningful_text(text) {
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
                "h1" | "h2" | "h3" | "h4" => return true,
                "strong" | "b" if has_meaningful_text(text) => return true,
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

fn normalize_text(text: &str) -> String {
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
        .map(|el| normalize_text(&el.text().collect::<String>()))
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
        let href_raw = match link.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let href = clean_href(href_raw);

        let url = if href.starts_with("//") {
            let full_href = format!("{}:{}", base.scheme(), href);
            match Url::parse(&full_href).ok() {
                Some(u) => u,
                None => continue,
            }
        } else {
            match Url::parse(&href).ok().or_else(|| base.join(&href).ok()) {
                Some(u) => u,
                None => continue,
            }
        };

        if !is_valid_scheme(&url) {
            continue;
        }

        if fallback.is_none() {
            fallback = Some(url.to_string());
        }

        let text_raw = link.text().collect::<String>();
        let text_norm = normalize_text(&text_raw);
        let is_heading =
            is_heading_link(&link, &text_raw) || link_matches_heading(&text_norm, &headings);
        let is_meaningful = has_meaningful_text(&text_raw) && !is_utility_text(&text_raw);

        if is_heading {
            heading_links.push((url.to_string(), text_norm.clone()));
        }

        if primary_text.is_none() && is_meaningful {
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
