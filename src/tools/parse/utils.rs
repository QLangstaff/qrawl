use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;

// Lazy static regexes for HTML cleaning - compiled once
static SCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap());
static STYLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap());
static NOSCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<noscript[^>]*>.*?</noscript>").unwrap());
static IFRAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<iframe[^>]*>.*?</iframe>").unwrap());
static SVG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<svg[^>]*>.*?</svg>").unwrap());
static NAV_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<nav[^>]*>.*?</nav>").unwrap());
static HEADER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<header[^>]*>.*?</header>").unwrap());
static FOOTER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<footer[^>]*>.*?</footer>").unwrap());
static ASIDE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<aside[^>]*>.*?</aside>").unwrap());
static FORM_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<form[^>]*>.*?</form>").unwrap());
static COMMENT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--.*?-->").unwrap());

// Comprehensive attribute removal regex - handles all quote styles and common junk attributes
static JUNK_ATTR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        \s+                                    # Leading whitespace
        (?:                                    # Attribute name (non-capturing group)
            class|id|style|                    # Common styling attributes
            data-[\w-]+|                       # All data-* attributes
            aria-[\w-]+|                       # All aria-* attributes
            role|tabindex|                     # Accessibility attributes
            xmlns(?::[\w-]+)?|                 # XML namespaces
            version|viewBox|                   # SVG attributes
            fill|fill-rule|stroke(?:-[\w-]+)?| # SVG styling
            onclick|onload|on[\w-]+            # Event handlers
        )
        \s*=\s*                                # Equals with optional whitespace
        (?:                                    # Value (non-capturing group)
            "[^"]*"|                           # Double-quoted value
            '[^']*'|                           # Single-quoted value
            [^\s>]+                            # Unquoted value
        )
        "#,
    )
    .unwrap()
});

// Whitespace normalization regex
static WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static NEWLINE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\n").unwrap());

/// Clean HTML by removing scripts, styles, and unnecessary attributes.
/// Fast regex-based cleaning - preserves content structure.
pub fn clean_html(html: &str) -> String {
    let mut cleaned = html.to_string();

    // Normalize escaped newlines first (e.g., literal \n in strings)
    cleaned = NEWLINE_REGEX.replace_all(&cleaned, " ").to_string();

    // Remove junk elements
    cleaned = SCRIPT_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = STYLE_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = NOSCRIPT_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = IFRAME_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = SVG_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = NAV_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = HEADER_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = FOOTER_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = ASIDE_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = FORM_REGEX.replace_all(&cleaned, "").to_string();
    cleaned = COMMENT_REGEX.replace_all(&cleaned, "").to_string();

    // Remove junk attributes (single comprehensive regex)
    cleaned = JUNK_ATTR_REGEX.replace_all(&cleaned, "").to_string();

    // Normalize whitespace - collapse multiple spaces/newlines into single space
    cleaned = WHITESPACE_REGEX.replace_all(&cleaned, " ").to_string();

    // Trim and return
    cleaned.trim().to_string()
}

/// Structure pattern for sibling detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StructurePattern {
    pub tags: Vec<String>,
}

/// Extract structure pattern from an element (ordered list of child tag names).
fn get_structure_pattern(element: &ElementRef) -> StructurePattern {
    let tags: Vec<String> = element
        .children()
        .filter_map(ElementRef::wrap)
        .map(|e| e.value().name().to_string())
        .collect();
    StructurePattern { tags }
}

/// Find the main content region of the page.
pub fn main_html(html: &str) -> String {
    let document = Html::parse_document(&html);
    let selectors = [
        "main",
        "article",
        "[role='main']",
        "#content",
        ".content",
        "body",
    ];

    for sel_str in selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            if let Some(el) = document.select(&selector).next() {
                return el.html();
            }
        }
    }

    document.root_element().html()
}

/// Detect siblings by scanning entire tree and finding richest sibling group.
/// Goes all the way down, finds all groups at all levels, returns the richest
/// (highest structural complexity, then most items).
///
/// # Arguments
/// * `html` - HTML to parse
/// * `exclude_domains` - Optional domains to exclude (filters groups during detection)
/// * `include_domains` - Optional domains to include (filters groups during detection)
pub fn siblings_html(
    html: &str,
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) -> Vec<String> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();

    // Scan entire tree and find ALL sibling groups at ALL levels
    // Each entry is (in_article, pattern_length, group_of_siblings)
    let mut all_sibling_groups: Vec<(bool, usize, Vec<String>)> = Vec::new();
    scan_for_all_sibling_groups(
        &root,
        &mut all_sibling_groups,
        exclude_domains,
        include_domains,
    );

    // Return using deterministic hierarchy: article context > coverage > quantity > pattern_len
    // Coverage = pattern_len * quantity (total elements explained by this pattern)
    // This handles remainders: [job+timestamp]*30 covers 60 elements vs timestamp*31 covers 31
    all_sibling_groups
        .into_iter()
        .max_by_key(|(in_article, pattern_len, group)| {
            let coverage = *pattern_len * group.len();
            (*in_article, coverage, group.len(), *pattern_len)
        })
        .map(|(_, _, group)| group)
        .unwrap_or_default()
}

/// Check if element is inside <article> tag (deterministic context check)
fn is_in_article(element: &ElementRef) -> bool {
    let mut ancestor = element.parent();
    while let Some(node) = ancestor {
        if let Some(elem) = ElementRef::wrap(node) {
            if elem.value().name() == "article" {
                return true;
            }
        }
        ancestor = node.parent();
    }
    false
}

fn scan_for_all_sibling_groups<'a>(
    element: &'a ElementRef<'a>,
    all_groups: &mut Vec<(bool, usize, Vec<String>)>,
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) {
    // Get children at this level (filter junk)
    let children: Vec<_> = element
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|child| {
            let tag = child.value().name();
            !matches!(tag, "script" | "style" | "iframe" | "noscript")
        })
        .collect();

    if children.len() >= 2 {
        // 1. Detect single-element patterns with common-prefix matching
        // This groups elements that share a core pattern even if they have different trailing elements
        let mut pattern_groups: Vec<(Vec<String>, Vec<usize>)> = Vec::new();

        for (idx, child) in children.iter().enumerate() {
            let pattern = get_structure_pattern(child);

            // Find existing group with compatible pattern (shares common prefix)
            let mut matched = false;
            for (group_tags, indices) in pattern_groups.iter_mut() {
                // Check if patterns share a common prefix of at least 2 elements
                let min_len = group_tags.len().min(pattern.tags.len());
                if min_len >= 2 && group_tags[..min_len] == pattern.tags[..min_len] {
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
            if indices.len() >= 2 && !tags.is_empty() {
                let siblings: Vec<String> = indices.iter().map(|&i| children[i].html()).collect();

                // Step 1: Validate group has ≥1 valid URL
                let should_include = if exclude_domains.is_some() || include_domains.is_some() {
                    group_has_valid_urls(&siblings, exclude_domains, include_domains)
                } else {
                    true // No filtering when no domain options set
                };

                if should_include {
                    // Step 2: Strip excluded URL siblings (noise removal) - only if filters are non-empty
                    let has_filters = exclude_domains.map_or(false, |e| !e.is_empty())
                        || include_domains.map_or(false, |i| !i.is_empty());

                    let filtered_siblings = if has_filters {
                        filter_siblings_by_domain(&siblings, exclude_domains, include_domains)
                    } else {
                        siblings
                    };

                    // Step 3: Only add if still has ≥2 siblings after filtering
                    if filtered_siblings.len() >= 2 {
                        let in_article = is_in_article(&children[indices[0]]);
                        all_groups.push((in_article, 1, filtered_siblings));
                    }
                }
            }
        }

        // 2. Detect multi-element patterns (new behavior)
        detect_multi_element_patterns(&children, all_groups, exclude_domains, include_domains);
    }

    // Recurse into ALL children to scan deeper levels
    for child in children {
        scan_for_all_sibling_groups(&child, all_groups, exclude_domains, include_domains);
    }
}

/// Detect multi-element repeating patterns in a sequence of children.
/// For example, [A,B,A,B,A,B] would be detected as 3 siblings with pattern length 2.
fn detect_multi_element_patterns(
    children: &[ElementRef],
    all_groups: &mut Vec<(bool, usize, Vec<String>)>,
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) {
    use std::collections::HashMap;

    let n = children.len();

    // Try pattern lengths from 2 up to n/2
    for pattern_len in 2..=(n / 2) {
        if n < pattern_len * 2 {
            break; // Need at least 2 repetitions
        }

        // Build a HashMap of multi-element patterns to their instance positions
        // Key: Vec<StructurePattern> representing the multi-element pattern
        // Value: Vec<usize> representing starting indices where this pattern appears
        let mut multi_pattern_groups: HashMap<Vec<StructurePattern>, Vec<usize>> = HashMap::new();

        // Scan through all possible positions to find pattern instances
        // This allows gaps between instances (like ads/navigation)
        let mut idx = 0;
        while idx + pattern_len <= n {
            // Extract the pattern at this position
            let pattern: Vec<StructurePattern> = (0..pattern_len)
                .map(|offset| get_structure_pattern(&children[idx + offset]))
                .collect();

            // Record this pattern instance position
            multi_pattern_groups.entry(pattern).or_default().push(idx);

            // Move to next position (not pattern_len ahead - allows overlapping detection)
            idx += 1;
        }

        // For each multi-element pattern found, if it appears 2+ times, add to groups
        for (pattern, start_indices) in multi_pattern_groups {
            if start_indices.len() >= 2 {
                // Skip homogeneous patterns (all elements have same structure)
                // These are already handled by single-element detection
                let first = &pattern[0];
                if pattern.iter().all(|p| p == first) {
                    continue;
                }

                // Filter out overlapping instances - only keep non-overlapping ones
                let mut non_overlapping: Vec<usize> = Vec::new();
                for &idx in &start_indices {
                    // Check if this instance overlaps with any previously selected instance
                    let overlaps = non_overlapping.iter().any(|&selected_idx| {
                        // Two instances overlap if their ranges intersect
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

                // Need at least 2 non-overlapping instances
                if non_overlapping.len() >= 2 {
                    // Found a repeating multi-element pattern with variation!
                    let siblings: Vec<String> = non_overlapping
                        .iter()
                        .map(|&start_idx| {
                            // Concatenate HTML of all elements in this pattern instance
                            (0..pattern_len)
                                .map(|offset| children[start_idx + offset].html())
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .collect();

                    // Step 1: Validate group has ≥1 valid URL
                    let should_include = if exclude_domains.is_some() || include_domains.is_some() {
                        group_has_valid_urls(&siblings, exclude_domains, include_domains)
                    } else {
                        true // No filtering when no domain options set
                    };

                    if should_include {
                        // Step 2: Strip excluded URL siblings (noise removal) - only if filters are non-empty
                        let has_filters = exclude_domains.map_or(false, |e| !e.is_empty())
                            || include_domains.map_or(false, |i| !i.is_empty());

                        let filtered_siblings = if has_filters {
                            filter_siblings_by_domain(&siblings, exclude_domains, include_domains)
                        } else {
                            siblings
                        };

                        // Step 3: Only add if still has ≥2 siblings after filtering
                        if filtered_siblings.len() >= 2 {
                            let in_article = is_in_article(&children[non_overlapping[0]]);
                            all_groups.push((in_article, pattern_len, filtered_siblings));
                        }
                    }
                }
            }
        }
    }
}

/// Check if a sibling group has valid URLs according to domain filters.
/// Used during sibling detection to filter groups before scoring.
pub fn group_has_valid_urls(
    siblings: &[String],
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) -> bool {
    // Extract all URLs from the sibling group
    let href_regex = Regex::new(r#"href=["']([^"']+)["']"#).unwrap();
    let urls: Vec<String> = siblings
        .iter()
        .flat_map(|html| {
            href_regex
                .captures_iter(html)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        })
        .collect();

    // If no URLs found, group is invalid
    if urls.is_empty() {
        return false;
    }

    // Check domain filters
    if let Some(include) = include_domains {
        // Whitelist mode: at least one URL must match included domains
        urls.iter()
            .any(|url| include.iter().any(|domain| url.contains(domain.as_str())))
    } else if let Some(exclude) = exclude_domains {
        // Blacklist mode: at least one URL must NOT match excluded domains
        urls.iter()
            .any(|url| !exclude.iter().any(|domain| url.contains(domain.as_str())))
    } else {
        // No filtering: group is valid if it has URLs
        true
    }
}

/// Filter individual siblings to remove excluded domain URLs.
/// Returns only siblings containing valid URLs according to domain rules.
///
/// This removes "noise" siblings (social share buttons, ads) from sibling groups
/// before scoring, ensuring group richness reflects actual content.
fn filter_siblings_by_domain(
    siblings: &[String],
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) -> Vec<String> {
    let href_regex = Regex::new(r#"href=["']([^"']+)["']"#).unwrap();

    siblings
        .iter()
        .filter(|sibling| {
            // Extract URLs from this sibling
            let urls: Vec<String> = href_regex
                .captures_iter(sibling)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            // No URLs = skip sibling
            if urls.is_empty() {
                return false;
            }

            // Check domain filters (same logic as group_has_valid_urls)
            if let Some(include) = include_domains {
                // Whitelist: sibling must have included URL
                urls.iter()
                    .any(|url| include.iter().any(|domain| url.contains(domain.as_str())))
            } else if let Some(exclude) = exclude_domains {
                // Blacklist: sibling must have non-excluded URL
                urls.iter()
                    .any(|url| !exclude.iter().any(|domain| url.contains(domain.as_str())))
            } else {
                // No filtering
                true
            }
        })
        .cloned()
        .collect()
}

/// Return HTML from siblings with children links (e.g. roundups).
///
/// # Arguments
/// * `html` - HTML to parse
/// * `exclude_domains` - Optional domains to exclude (filters groups during detection)
/// * `include_domains` - Optional domains to include (filters groups during detection)
pub fn children_html(
    html: &str,
    exclude_domains: Option<&HashSet<String>>,
    include_domains: Option<&HashSet<String>>,
) -> Vec<String> {
    let siblings = siblings_html(html, exclude_domains, include_domains);

    siblings
        .into_iter()
        .filter(|sibling| {
            // Check if sibling contains links (relative or absolute)
            sibling.contains("href=")
        })
        .collect()
}
