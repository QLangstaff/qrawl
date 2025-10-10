#![doc = include_str!("./README.md")]

pub mod cli;
mod tests;
mod utils;

use std::collections::HashSet;
use utils::*;

/// Options for parsing HTML content.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Remove scripts, styles, and unnecessary attributes.
    pub clean: bool,
    /// Extract main content area first (only applies to `parse()`, not siblings/children).
    pub main: bool,
    /// Domains to exclude from sibling detection (e.g., ["reddit.com", "instagram.com"]).
    /// Sibling groups containing only URLs from these domains will be filtered out.
    pub exclude_domains: Option<HashSet<String>>,
    /// Domains to include in sibling detection (e.g., ["example.com", "recipes.org"]).
    /// If set, only sibling groups with URLs from these domains will be kept.
    /// Takes precedence over exclude_domains if both are set.
    pub include_domains: Option<HashSet<String>>,
}

impl ParseOptions {
    /// Create default options (clean + main, no domain filtering).
    /// Works for all parse functions: `parse()`, `parse_siblings()`, `parse_children()`.
    pub fn default() -> Self {
        Self {
            clean: true,
            main: true,
            exclude_domains: None,
            include_domains: None,
        }
    }

    /// Add excluded domains (blacklist mode).
    pub fn with_exclude_domains(mut self, domains: HashSet<String>) -> Self {
        self.exclude_domains = Some(domains);
        self
    }

    /// Add included domains (whitelist mode).
    pub fn with_include_domains(mut self, domains: HashSet<String>) -> Self {
        self.include_domains = Some(domains);
        self
    }
}

/// Parse HTML content with options.
///
/// Primary function for extracting the main content region of a page.
/// The `main` option in ParseOptions controls whether to extract the main content area.
///
/// # Arguments
/// * `html` - HTML string to parse
/// * `options` - Parse options (clean, main)
///
/// # Examples
/// ```rust
/// use qrawl::tools::parse::{parse, ParseOptions};
///
/// let html = "<html>...</html>";
///
/// // Extract main content with cleaning (default)
/// let main = parse(html, &ParseOptions::default());
///
/// // Just clean HTML without main extraction
/// let clean = parse(html, &ParseOptions { clean: true, main: false, ..Default::default() });
///
/// // Raw main content (no cleaning)
/// let raw_main = parse(html, &ParseOptions { clean: false, main: true, ..Default::default() });
/// ```
pub fn parse(html: &str, options: &ParseOptions) -> String {
    let mut result = html.to_string();

    if options.clean {
        result = clean_html(&result);
    }

    if options.main {
        result = main_html(&result);
    }

    result
}

/// Parse siblings from HTML with options.
///
/// Detects repeating sibling patterns (e.g., recipe roundups, article lists).
/// Domain filtering is applied DURING sibling detection (before scoring by quantity).
///
/// # Arguments
/// * `html` - HTML string to parse
/// * `options` - Parse options (clean, main, domain filters)
///
/// # Examples
/// ```rust
/// use qrawl::tools::parse::{parse_siblings, ParseOptions};
///
/// let html = "<html>...</html>";
///
/// // Default: clean + main
/// let siblings = parse_siblings(html, &ParseOptions::default());
///
/// // With domain filtering (filters during detection, not after!)
/// let options = ParseOptions::default()
///     .with_exclude_domains(["reddit.com", "instagram.com"].iter().map(|s| s.to_string()).collect());
/// let siblings = parse_siblings(html, &options);
/// ```
pub fn parse_siblings(html: &str, options: &ParseOptions) -> Vec<String> {
    siblings_html(
        &parse(html, options),
        options.exclude_domains.as_ref(),
        options.include_domains.as_ref(),
    )
}

/// Parse children from HTML with options.
///
/// Children are siblings that contain external links (href=).
/// This applies the same sibling detection as `parse_siblings()`,
/// then filters to only include siblings with links.
///
/// Domain filtering is applied DURING sibling detection (before scoring).
///
/// # Arguments
/// * `html` - HTML string to parse
/// * `options` - Parse options (clean, main, domain filters)
///
/// # Examples
/// ```rust
/// use qrawl::tools::parse::{parse_children, ParseOptions};
///
/// let html = "<html>...</html>";
/// // Default: clean + main
/// let children = parse_children(html, &ParseOptions::default());
///
/// // With domain filtering (filters during detection, not after!)
/// let options = ParseOptions::default()
///     .with_exclude_domains(["reddit.com", "instagram.com"].iter().map(|s| s.to_string()).collect());
/// let children = parse_children(html, &options);
/// ```
pub fn parse_children(html: &str, options: &ParseOptions) -> Vec<String> {
    children_html(
        &parse(html, options),
        options.exclude_domains.as_ref(),
        options.include_domains.as_ref(),
    )
}
