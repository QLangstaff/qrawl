mod utils;

use utils::*;

use scraper::Html;

/// Parse clean HTML (removes scripts, styles, etc).
pub fn parse_clean(html: &str) -> String {
    clean_html(html)
}

/// Parse main content area from HTML.
pub fn parse_main(html: &str) -> String {
    main_html(html)
}

/// Parse siblings from HTML (e.g. roundup).
pub fn parse_siblings(html: &str) -> Vec<String> {
    siblings_html(html)
}

/// Parse children from HTML (e.g. roundup with external links).
pub fn parse_children(html: &str) -> Vec<String> {
    children_html(html)
}
