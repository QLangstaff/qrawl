#![cfg(test)]
mod tests {
    use crate::tools::clean::*;
    use crate::tools::clean::utils::{
        canonicalize_domain, canonicalize_url, clean_email, clean_phone, decode_html_entities,
    };

    #[test]
    fn test_html_entities_named() {
        assert_eq!(clean_text("&amp;"), "&");
        assert_eq!(clean_text("&lt;"), "<");
        assert_eq!(clean_text("&gt;"), ">");
        assert_eq!(clean_text("&quot;"), "\"");
        assert_eq!(clean_text("&apos;"), "'");
        // nbsp is decoded but then normalized by whitespace normalization
        assert_eq!(clean_text("&nbsp;"), "");
    }

    #[test]
    fn test_html_entities_numeric() {
        assert_eq!(clean_text("&#39;"), "'");
        assert_eq!(clean_text("&#x27;"), "'");
        assert_eq!(clean_text("&#34;"), "\"");
        assert_eq!(clean_text("&#x22;"), "\"");
    }

    #[test]
    fn test_html_entities_combined() {
        assert_eq!(clean_text("&lt;div&gt;"), "<div>");
        assert_eq!(clean_text("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(clean_text("It&#39;s &quot;great&quot;"), "It's \"great\"");
    }

    #[test]
    fn test_unicode_normalization() {
        // Precomposed vs decomposed characters
        let precomposed = "\u{00E9}"; // é (single character)
        let decomposed = "e\u{0301}"; // e + combining acute accent

        // After cleaning, both should be the same
        assert_eq!(clean_text(precomposed), clean_text(decomposed));
        assert_eq!(clean_text(precomposed), "é");
    }

    #[test]
    fn test_zero_width_characters() {
        assert_eq!(clean_text("hello\u{200B}world"), "helloworld");
        assert_eq!(clean_text("test\u{200C}ing"), "testing");
        assert_eq!(clean_text("word\u{200D}join"), "wordjoin");
        assert_eq!(clean_text("\u{FEFF}text"), "text");
    }

    #[test]
    fn test_control_characters() {
        // Should remove most control characters
        assert_eq!(clean_text("hello\x00world"), "helloworld");
        assert_eq!(clean_text("test\x01ing"), "testing");

        // But keep newlines and tabs (then normalized to space)
        assert_eq!(clean_text("line1\nline2"), "line1 line2"); // Normalized to space
        assert_eq!(clean_text("tab\there"), "tab here"); // Normalized to space
    }

    #[test]
    fn test_clean_text_handles_crlf_mixture() {
        let mixed = "First\r\nSecond\rThird\tFourth";
        assert_eq!(clean_text(mixed), "First Second Third Fourth");
    }

    #[test]
    fn test_whitespace_normalization() {
        assert_eq!(clean_text("hello   world"), "hello world");
        assert_eq!(clean_text("  trim  me  "), "trim me");
        assert_eq!(clean_text("multiple\n\n\nlines"), "multiple lines");
        assert_eq!(clean_text("lots\t\t\tof\t\ttabs"), "lots of tabs");
        assert_eq!(
            clean_text("  leading and trailing  "),
            "leading and trailing"
        );
    }

    #[test]
    fn test_combined_cleaning() {
        let dirty = "Hello &amp; &#39;world&#39;   with   \u{200B}spaces\x00";
        assert_eq!(clean_text(dirty), "Hello & 'world' with spaces");
    }

    #[test]
    fn test_real_world_examples() {
        // Recipe title with entities
        assert_eq!(
            clean_text("Ben &amp; Jerry&#39;s Ice Cream"),
            "Ben & Jerry's Ice Cream"
        );

        // Description with extra whitespace
        assert_eq!(
            clean_text("there   are   too   many   spaces!"),
            "there are too many spaces!"
        );

        // Mixed issues
        assert_eq!(
            clean_text("  &lt;b&gt;Bold&lt;/b&gt;   text  "),
            "<b>Bold</b> text"
        );
    }

    #[test]
    fn test_empty_and_whitespace() {
        assert_eq!(clean_text(""), "");
        assert_eq!(clean_text("   "), "");
        assert_eq!(clean_text("\n\n\n"), "");
        assert_eq!(clean_text("\t\t\t"), "");
    }

    #[test]
    fn test_no_changes_needed() {
        assert_eq!(clean_text("perfect text"), "perfect text");
        assert_eq!(clean_text("no entities here"), "no entities here");
    }

    #[test]
    fn test_preserves_intentional_characters() {
        // Should not remove these
        assert_eq!(clean_text("hello-world"), "hello-world");
        assert_eq!(clean_text("under_score"), "under_score");
        assert_eq!(clean_text("with.period"), "with.period");
        assert_eq!(clean_text("a/b/c"), "a/b/c");
    }

    #[test]
    fn test_unicode_characters() {
        // Emoji and other unicode should be preserved
        assert_eq!(clean_text("Hello 👋 World 🌍"), "Hello 👋 World 🌍");
        assert_eq!(clean_text("Café"), "Café");
        assert_eq!(clean_text("日本語"), "日本語");
    }

    // Tests for clean_html()

    #[test]
    fn test_clean_html_removes_comments() {
        assert_eq!(
            clean_html("<div><!-- comment --><p>Text</p></div>"),
            "<div><p>Text</p></div>"
        );
        assert_eq!(
            clean_html("<!-- start --><p>Content</p><!-- end -->"),
            "<p>Content</p>"
        );
    }

    #[test]
    fn test_clean_html_multiline_comments() {
        assert_eq!(
            clean_html("<div><!--\nmultiline\ncomment\n--><p>Text</p></div>"),
            "<div><p>Text</p></div>"
        );
    }

    #[test]
    fn test_clean_html_normalizes_whitespace() {
        assert_eq!(
            clean_html("<div>  <p>Text</p>  </div>"),
            "<div> <p>Text</p> </div>"
        );
    }

    #[test]
    fn test_clean_html_removes_empty_lines() {
        assert_eq!(
            clean_html("<div>\n\n<p>Text</p>\n\n</div>"),
            "<div> <p>Text</p> </div>"
        );
    }

    #[test]
    fn test_clean_html_combined() {
        let dirty = "<div>  <!-- comment -->  \n\n<p>Text</p>  \n\n  </div>";
        let expected = "<div> <p>Text</p> </div>";
        assert_eq!(clean_html(dirty), expected);
    }

    #[test]
    fn test_clean_html_removes_scripts() {
        assert_eq!(
            clean_html("<div><script>alert('hi')</script><p>Text</p></div>"),
            "<div><p>Text</p></div>"
        );
        assert_eq!(
            clean_html("<script src='app.js'></script><p>Content</p>"),
            "<p>Content</p>"
        );
    }

    #[test]
    fn test_clean_html_removes_styles() {
        assert_eq!(
            clean_html("<div><style>.red{color:red}</style><p>Text</p></div>"),
            "<div><p>Text</p></div>"
        );
    }

    #[test]
    fn test_clean_html_removes_noscript_iframe_svg() {
        assert_eq!(
            clean_html("<noscript>Enable JS</noscript><p>Text</p>"),
            "<p>Text</p>"
        );
        assert_eq!(
            clean_html("<iframe src='ad.html'></iframe><p>Text</p>"),
            "<p>Text</p>"
        );
        assert_eq!(clean_html("<svg><circle/></svg><p>Text</p>"), "<p>Text</p>");
    }

    #[test]
    fn test_clean_html_removes_junk_attributes() {
        assert_eq!(
            clean_html("<div class='container' id='main' style='color:red'>Text</div>"),
            "<div>Text</div>"
        );
        assert_eq!(
            clean_html("<button onclick='alert()' data-id='123'>Click</button>"),
            "<button>Click</button>"
        );
        assert_eq!(
            clean_html("<div aria-label='info' role='button'>Text</div>"),
            "<div>Text</div>"
        );
    }

    #[test]
    fn test_clean_html_preserves_jsonld() {
        let html = r#"<script>alert('bad')</script><script type="application/ld+json">{"@context":"schema.org"}</script><p>Text</p>"#;
        let cleaned = clean_html(html);
        assert!(cleaned.contains(r#"<script type="application/ld+json">"#));
        assert!(cleaned.contains(r#"{"@context":"schema.org"}"#));
        assert!(!cleaned.contains("alert('bad')"));
    }

    #[test]
    fn test_clean_html_handles_uppercase_tags() {
        let html = "<DIV><SCRIPT>alert('hi')</SCRIPT><STYLE>body{}</STYLE><P>Text</P></DIV>";
        assert_eq!(clean_html(html), "<DIV><P>Text</P></DIV>");
    }

    #[test]
    fn test_clean_html_preserves_jsonld_case_variants() {
        let html = r#"<SCRIPT TYPE="APPLICATION/LD+JSON">{"@type":"Thing"}</SCRIPT><p>Text</p>"#;
        let cleaned = clean_html(html);
        assert!(cleaned
            .to_lowercase()
            .contains(r#"<script type="application/ld+json">"#));
        assert!(cleaned.contains(r#"{"@type":"Thing"}"#));
    }

    #[test]
    fn test_clean_html_normalizes_escaped_newlines() {
        let html = "<div>Line\\nBreak</div>";
        assert_eq!(clean_html(html), "<div>Line Break</div>");
    }

    // Tests for clean_urls()

    #[test]
    fn test_clean_urls_exact_duplicates() {
        let urls = vec![
            "https://example.com".to_string(),
            "https://example.com".to_string(),
            "https://example.com".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0], "https://example.com");
    }

    #[test]
    fn test_clean_urls_protocol_normalization() {
        let urls = vec![
            "http://example.com".to_string(),
            "https://example.com".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1); // Both normalize to https
    }

    #[test]
    fn test_clean_urls_case_normalization() {
        let urls = vec![
            "https://Example.com".to_string(),
            "https://EXAMPLE.COM".to_string(),
            "https://example.com".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
    }

    #[test]
    fn test_clean_urls_www_stripping() {
        let urls = vec![
            "https://www.example.com".to_string(),
            "https://example.com".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
    }

    #[test]
    fn test_clean_urls_trailing_slash() {
        let urls = vec![
            "https://example.com/path".to_string(),
            "https://example.com/path/".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
    }

    #[test]
    fn test_clean_urls_query_param_order() {
        let urls = vec![
            "https://example.com?b=2&a=1".to_string(),
            "https://example.com?a=1&b=2".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
    }

    #[test]
    fn test_clean_urls_fragment_removal() {
        let urls = vec![
            "https://example.com/page#section1".to_string(),
            "https://example.com/page#section2".to_string(),
            "https://example.com/page".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1);
    }

    #[test]
    fn test_clean_urls_combined() {
        let urls = vec![
            "https://example.com/path".to_string(),
            "HTTP://www.example.com/path/".to_string(),
            "https://EXAMPLE.COM/path".to_string(),
            "http://example.com/path#frag".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 1); // All canonicalize to same URL
    }

    #[test]
    fn test_clean_urls_preserves_order() {
        let urls = vec![
            "https://example.com/first".to_string(),
            "https://example.com/second".to_string(),
            "https://example.com/first".to_string(), // Duplicate
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 2);
        assert_eq!(cleaned[0], "https://example.com/first");
        assert_eq!(cleaned[1], "https://example.com/second");
    }

    #[test]
    fn test_clean_urls_returns_canonical() {
        // Returns canonical URL
        let urls = vec!["HTTP://Example.com/Path".to_string()];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned[0], "https://example.com/Path");
    }

    #[test]
    fn test_clean_urls_canonicalizes_idna_domains() {
        let urls = vec!["https://münich.com/path".to_string()];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned[0], "https://xn--mnich-kva.com/path");
    }

    #[test]
    fn test_clean_urls_malformed() {
        let urls = vec![
            "not-a-url".to_string(),
            "https://example.com".to_string(),
            "also-not-url".to_string(),
        ];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 3); // Malformed URLs kept as-is, all different
    }

    #[test]
    fn test_clean_urls_empty_list() {
        let urls: Vec<String> = vec![];
        let cleaned = clean_urls(&urls);
        assert_eq!(cleaned.len(), 0);
    }

    // Tests for clean_emails()

    #[test]
    fn test_clean_emails_deduplication() {
        let emails = vec![
            " john@example.com ".to_string(),
            "John@Example.COM".to_string(),
            "\"John Doe\" <john@example.com>".to_string(),
        ];
        let cleaned = clean_emails(&emails);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0], "john@example.com");
    }

    #[test]
    fn test_clean_emails_with_punctuation() {
        // Test CLI use case where emails have trailing punctuation
        let emails = vec![
            "john@example.com,".to_string(),
            "john@example.com".to_string(),
        ];
        let cleaned = clean_emails(&emails);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0], "john@example.com");
    }

    #[test]
    fn test_clean_emails_filters_empty() {
        let emails = vec![
            "john@example.com".to_string(),
            "".to_string(),
            "jane@example.com".to_string(),
            "   ".to_string(),
        ];
        let cleaned = clean_emails(&emails);
        assert_eq!(cleaned.len(), 2);
    }

    #[test]
    fn test_clean_emails_preserves_order() {
        let emails = vec![
            "first@example.com".to_string(),
            "second@example.com".to_string(),
            "first@example.com".to_string(), // Duplicate
        ];
        let cleaned = clean_emails(&emails);
        assert_eq!(cleaned.len(), 2);
        assert_eq!(cleaned[0], "first@example.com");
        assert_eq!(cleaned[1], "second@example.com");
    }

    #[test]
    fn test_clean_emails_empty_list() {
        let emails: Vec<String> = vec![];
        let cleaned = clean_emails(&emails);
        assert_eq!(cleaned.len(), 0);
    }

    // Tests for clean_phones()

    #[test]
    fn test_clean_phones_deduplication() {
        let phones = vec![
            "(555) 123-4567".to_string(),
            "555-123-4567".to_string(),
            "555.123.4567".to_string(),
        ];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0], "5551234567");
    }

    #[test]
    fn test_clean_phones_filters_empty() {
        let phones = vec![
            "555-123-4567".to_string(),
            "".to_string(),
            "555-987-6543".to_string(),
            "   ".to_string(),
        ];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 2);
    }

    #[test]
    fn test_clean_phones_preserves_order() {
        let phones = vec![
            "555-123-4567".to_string(),
            "555-987-6543".to_string(),
            "(555) 123-4567".to_string(), // Duplicate
        ];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 2);
        assert_eq!(cleaned[0], "5551234567");
        assert_eq!(cleaned[1], "5559876543");
    }

    #[test]
    fn test_clean_phones_with_extensions() {
        let phones = vec![
            "555-123-4567 ext. 123".to_string(),
            "555-123-4567 x456".to_string(),
        ];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 1); // Same after stripping extensions
        assert_eq!(cleaned[0], "5551234567");
    }

    #[test]
    fn test_clean_phones_international_vs_local() {
        // International and local versions should be treated as different
        let phones = vec!["+1-555-123-4567".to_string(), "555-123-4567".to_string()];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 2);
        assert_eq!(cleaned[0], "+15551234567");
        assert_eq!(cleaned[1], "5551234567");
    }

    #[test]
    fn test_clean_phones_empty_list() {
        let phones: Vec<String> = vec![];
        let cleaned = clean_phones(&phones);
        assert_eq!(cleaned.len(), 0);
    }

    // Tests for utility functions

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;&gt;"), "<>");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&#39;"), "'");
        assert_eq!(decode_html_entities("&#x27;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), "\u{00A0}");
    }

    #[test]
    fn test_canonicalize_domain() {
        // Basic lowercase
        assert_eq!(canonicalize_domain("Example.com"), "example.com");
        assert_eq!(canonicalize_domain("GITHUB.COM"), "github.com");

        // Strip www
        assert_eq!(canonicalize_domain("www.example.com"), "example.com");
        assert_eq!(canonicalize_domain("WWW.EXAMPLE.COM"), "example.com");

        // Combined
        assert_eq!(canonicalize_domain("WWW.GitHub.com"), "github.com");

        // Don't strip www if it's the whole domain
        assert_eq!(canonicalize_domain("www"), "www");

        // Subdomains preserved
        assert_eq!(canonicalize_domain("api.example.com"), "api.example.com");
        assert_eq!(
            canonicalize_domain("www.api.example.com"),
            "api.example.com"
        );
    }

    #[test]
    fn test_canonicalize_url() {
        // Protocol normalization
        assert_eq!(
            canonicalize_url("http://example.com"),
            "https://example.com"
        );

        assert_eq!(canonicalize_url("example.com"), "https://example.com");

        assert_eq!(canonicalize_url("www.example.com"), "https://example.com");

        // Domain canonicalization
        assert_eq!(
            canonicalize_url("https://WWW.Example.COM"),
            "https://example.com"
        );

        // Trailing slash normalization
        assert_eq!(
            canonicalize_url("https://example.com/path/"),
            "https://example.com/path"
        );
        assert_eq!(
            canonicalize_url("https://example.com/"),
            "https://example.com"
        ); // Root slash stripped

        // Query parameter sorting
        assert_eq!(
            canonicalize_url("https://example.com?b=2&a=1"),
            "https://example.com/?a=1&b=2"
        );

        // Fragment removal
        assert_eq!(
            canonicalize_url("https://example.com/page#section"),
            "https://example.com/page"
        );

        // Combined
        assert_eq!(
            canonicalize_url("HTTP://www.Example.COM/path/?b=2&a=1#frag"),
            "https://example.com/path?a=1&b=2"
        );

        // Malformed URLs kept as-is
        assert_eq!(canonicalize_url("not-a-url"), "not-a-url");
    }

    #[test]
    fn test_clean_email() {
        // Trim whitespace
        assert_eq!(clean_email(" john@example.com "), "john@example.com");

        // Lowercase
        assert_eq!(clean_email("John@Example.COM"), "john@example.com");

        // URL decode
        assert_eq!(clean_email("john%40example.com"), "john@example.com");

        // Extract from display name
        assert_eq!(
            clean_email("\"John Doe\" <john@example.com>"),
            "john@example.com"
        );
        assert_eq!(
            clean_email("John Doe <john@example.com>"),
            "john@example.com"
        );

        // Strip trailing punctuation
        assert_eq!(clean_email("john@example.com,"), "john@example.com");
        assert_eq!(clean_email("john@example.com;"), "john@example.com");
        assert_eq!(clean_email("john@example.com."), "john@example.com");

        // Combined
        assert_eq!(
            clean_email(" \"John\" <John@Example.COM> "),
            "john@example.com"
        );

        // Already clean
        assert_eq!(clean_email("john@example.com"), "john@example.com");
    }

    #[test]
    fn test_clean_phone() {
        // Strip separators
        assert_eq!(clean_phone("(555) 123-4567"), "5551234567");
        assert_eq!(clean_phone("555-123-4567"), "5551234567");
        assert_eq!(clean_phone("555.123.4567"), "5551234567");

        // Keep international prefix
        assert_eq!(clean_phone("+1-555-123-4567"), "+15551234567");
        assert_eq!(clean_phone("+1 (555) 123-4567"), "+15551234567");

        // Strip extensions
        assert_eq!(clean_phone("555-123-4567 ext. 123"), "5551234567");
        assert_eq!(clean_phone("555-123-4567 x123"), "5551234567");
        assert_eq!(clean_phone("555-123-4567 extension 123"), "5551234567");

        // Trim whitespace
        assert_eq!(clean_phone(" 555-123-4567 "), "5551234567");

        // Already clean
        assert_eq!(clean_phone("5551234567"), "5551234567");
    }
}
