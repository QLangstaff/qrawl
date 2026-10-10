#![cfg(test)]
use crate::tools::normalize::utils::{decode_html_entities, normalize_email, normalize_phone};
use crate::tools::normalize::*;
use crate::utils::canonical_domain;

// Tests for normalize_text()

#[test]
fn test_html_entities_named() {
    assert_eq!(normalize_text("&amp;"), "&");
    assert_eq!(normalize_text("&lt;"), "<");
    assert_eq!(normalize_text("&gt;"), ">");
    assert_eq!(normalize_text("&quot;"), "\"");
    assert_eq!(normalize_text("&apos;"), "'");
    // nbsp is decoded but then normalized by whitespace normalization
    assert_eq!(normalize_text("&nbsp;"), "");
}

#[test]
fn test_html_entities_numeric() {
    assert_eq!(normalize_text("&#39;"), "'");
    assert_eq!(normalize_text("&#x27;"), "'");
    assert_eq!(normalize_text("&#34;"), "\"");
    assert_eq!(normalize_text("&#x22;"), "\"");
}

#[test]
fn test_html_entities_combined() {
    assert_eq!(normalize_text("&lt;div&gt;"), "<div>");
    assert_eq!(normalize_text("Tom &amp; Jerry"), "Tom & Jerry");
    assert_eq!(
        normalize_text("It&#39;s &quot;great&quot;"),
        "It's \"great\""
    );
}

#[test]
fn test_unicode_normalization() {
    // Precomposed vs decomposed characters
    let precomposed = "\u{00E9}"; // é (single character)
    let decomposed = "e\u{0301}"; // e + combining acute accent
                                  // After normalization, both should be the same
    assert_eq!(normalize_text(precomposed), normalize_text(decomposed));
    assert_eq!(normalize_text(precomposed), "é");
}

#[test]
fn test_zero_width_characters() {
    assert_eq!(normalize_text("hello\u{200B}world"), "helloworld");
    assert_eq!(normalize_text("test\u{200C}ing"), "testing");
    assert_eq!(normalize_text("word\u{200D}join"), "wordjoin");
    assert_eq!(normalize_text("\u{FEFF}text"), "text");
}

#[test]
fn test_control_characters() {
    // Should remove most control characters
    assert_eq!(normalize_text("hello\x00world"), "helloworld");
    assert_eq!(normalize_text("test\x01ing"), "testing");
    // But keep newlines and tabs (then normalized to space)
    assert_eq!(normalize_text("line1\nline2"), "line1 line2"); // Normalized to space
    assert_eq!(normalize_text("tab\there"), "tab here"); // Normalized to space
}

#[test]
fn test_normalize_text_handles_crlf_mixture() {
    assert_eq!(
        normalize_text("First\r\nSecond\rThird\tFourth"),
        "First Second Third Fourth"
    );
}

#[test]
fn test_whitespace_normalization() {
    assert_eq!(normalize_text("hello   world"), "hello world");
    assert_eq!(normalize_text("  trim  me  "), "trim me");
    assert_eq!(normalize_text("multiple\n\n\nlines"), "multiple lines");
    assert_eq!(normalize_text("lots\t\t\tof\t\ttabs"), "lots of tabs");
    assert_eq!(
        normalize_text("  leading and trailing  "),
        "leading and trailing"
    );
}

#[test]
fn test_combined_normalization() {
    assert_eq!(
        normalize_text("Hello &amp; &#39;world&#39;   with   \u{200B}spaces\x00"),
        "Hello & 'world' with spaces",
    );
}

#[test]
fn test_real_world_examples() {
    // Recipe title with entities
    assert_eq!(
        normalize_text("Ben &amp; Jerry&#39;s Ice Cream"),
        "Ben & Jerry's Ice Cream"
    );
    // Description with extra whitespace
    assert_eq!(
        normalize_text("there   are   too   many   spaces!"),
        "there are too many spaces!"
    );
    // Mixed issues
    assert_eq!(
        normalize_text("  &lt;b&gt;Bold&lt;/b&gt;   text  "),
        "<b>Bold</b> text"
    );
}

#[test]
fn test_empty_and_whitespace() {
    assert_eq!(normalize_text(""), "");
    assert_eq!(normalize_text("   "), "");
    assert_eq!(normalize_text("\n\n\n"), "");
    assert_eq!(normalize_text("\t\t\t"), "");
}

#[test]
fn test_no_changes_needed() {
    assert_eq!(normalize_text("perfect text"), "perfect text");
    assert_eq!(normalize_text("no entities here"), "no entities here");
}

#[test]
fn test_preserves_intentional_characters() {
    // Should not remove these
    assert_eq!(normalize_text("hello-world"), "hello-world");
    assert_eq!(normalize_text("under_score"), "under_score");
    assert_eq!(normalize_text("with.period"), "with.period");
    assert_eq!(normalize_text("a/b/c"), "a/b/c");
}

#[test]
fn test_unicode_characters() {
    // Emoji and other unicode should be preserved
    assert_eq!(normalize_text("Hello 👋 World 🌍"), "Hello 👋 World 🌍");
    assert_eq!(normalize_text("Café"), "Café");
    assert_eq!(normalize_text("日本語"), "日本語");
}

// Tests for normalize_html()

#[tokio::test]
async fn test_normalize_html_removes_comments() {
    assert_eq!(
        normalize_html(&"<div><!-- comment --><p>Text</p></div>".into())
            .await
            .as_str(),
        "<div><p>Text</p></div>"
    );
    assert_eq!(
        normalize_html(&"<!-- start --><p>Content</p><!-- end -->".into())
            .await
            .as_str(),
        "<p>Content</p>"
    );
}

#[tokio::test]
async fn test_normalize_html_multiline_comments() {
    assert_eq!(
        normalize_html(&"<div><!--\nmultiline\ncomment\n--><p>Text</p></div>".into())
            .await
            .as_str(),
        "<div><p>Text</p></div>"
    );
}

#[tokio::test]
async fn test_normalize_html_normalizes_whitespace() {
    assert_eq!(
        normalize_html(&"<div>  <p>Text</p>  </div>".into())
            .await
            .as_str(),
        "<div> <p>Text</p> </div>"
    );
}

#[tokio::test]
async fn test_normalize_html_removes_empty_lines() {
    assert_eq!(
        normalize_html(&"<div>\n\n<p>Text</p>\n\n</div>".into())
            .await
            .as_str(),
        "<div> <p>Text</p> </div>"
    );
}

#[tokio::test]
async fn test_normalize_html_combined() {
    assert_eq!(
        normalize_html(&"<div>  <!-- comment -->  \n\n<p>Text</p>  \n\n  </div>".into())
            .await
            .as_str(),
        "<div> <p>Text</p> </div>",
    );
}

#[tokio::test]
async fn test_normalize_html_removes_scripts() {
    assert_eq!(
        normalize_html(&"<div><script>alert('hi')</script><p>Text</p></div>".into())
            .await
            .as_str(),
        "<div><p>Text</p></div>"
    );
    assert_eq!(
        normalize_html(&"<script src='app.js'></script><p>Content</p>".into())
            .await
            .as_str(),
        "<p>Content</p>"
    );
}

#[tokio::test]
async fn test_normalize_html_removes_styles() {
    assert_eq!(
        normalize_html(&"<div><style>.red{color:red}</style><p>Text</p></div>".into())
            .await
            .as_str(),
        "<div><p>Text</p></div>"
    );
}

#[tokio::test]
async fn test_normalize_html_removes_noscript_iframe_svg() {
    assert_eq!(
        normalize_html(&"<noscript>Enable JS</noscript><p>Text</p>".into())
            .await
            .as_str(),
        "<p>Text</p>"
    );
    assert_eq!(
        normalize_html(&"<iframe src='ad.html'></iframe><p>Text</p>".into())
            .await
            .as_str(),
        "<p>Text</p>"
    );
    assert_eq!(
        normalize_html(&"<svg><circle/></svg><p>Text</p>".into())
            .await
            .as_str(),
        "<p>Text</p>"
    );
}

#[tokio::test]
async fn test_normalize_html_removes_junk_attributes() {
    assert_eq!(
        normalize_html(&"<div class='container' id='main' style='color:red'>Text</div>".into())
            .await
            .as_str(),
        "<div>Text</div>"
    );
    assert_eq!(
        normalize_html(&"<button onclick='alert()' data-id='123'>Click</button>".into())
            .await
            .as_str(),
        "<button>Click</button>"
    );
    assert_eq!(
        normalize_html(&"<div aria-label='info' role='button'>Text</div>".into())
            .await
            .as_str(),
        "<div>Text</div>"
    );
}

#[tokio::test]
async fn test_normalize_html_preserves_jsonld() {
    let input = r#"<script>alert('bad')</script><script type="application/ld+json">{"@context":"schema.org"}</script><p>Text</p>"#;
    let output = normalize_html(&input.into()).await;
    assert!(output
        .as_str()
        .contains(r#"<script type="application/ld+json">"#));
    assert!(output.as_str().contains(r#"{"@context":"schema.org"}"#));
    assert!(!output.as_str().contains("alert('bad')"));
}

#[tokio::test]
async fn test_normalize_html_handles_uppercase_tags() {
    assert_eq!(
        normalize_html(
            &"<DIV><SCRIPT>alert('hi')</SCRIPT><STYLE>body{}</STYLE><P>Text</P></DIV>".into()
        )
        .await
        .as_str(),
        "<div><p>Text</p></div>",
    );
}

#[tokio::test]
async fn test_normalize_html_preserves_jsonld_case_variants() {
    let input = r#"<SCRIPT TYPE="APPLICATION/LD+JSON">{"@type":"Thing"}</SCRIPT><p>Text</p>"#;
    let output = normalize_html(&input.into()).await;
    assert!(output
        .as_str()
        .to_lowercase()
        .contains(r#"<script type="application/ld+json">"#));
    assert!(output.as_str().contains(r#"{"@type":"Thing"}"#));
}

#[tokio::test]
async fn normalize_html_keeps_text_as_written() {
    for (html, expected) in [
        // A backslash-n in text is text, not a newline.
        ("<p>C:\\new\\table</p>", "<p>C:\\new\\table</p>"),
        // A non-breaking space is content; it isn't collapsed into the spaces,
        // or trimmed from the ends.
        ("<p>a&nbsp;&nbsp;b  c</p>", "<p>a\u{a0}\u{a0}b c</p>"),
        (" &nbsp;<p>x</p>&nbsp; ", "\u{a0}<p>x</p>\u{a0}"),
    ] {
        assert_eq!(normalize_html(&html.into()).await.as_str(), expected);
    }
}

#[tokio::test]
async fn normalize_html_keeps_attributes_in_source_order_with_their_prefixes() {
    for (html, expected) in [
        (
            r#"<a href="/x" title="t" rel="nofollow" target="_blank" name="n" class="c">go</a>"#,
            r#"<a href="/x" title="t" rel="nofollow" target="_blank" name="n">go</a>"#,
        ),
        (
            r##"<math xlink:href="#a" xml:lang="en"><mi>x</mi></math>"##,
            r##"<math xlink:href="#a" xml:lang="en"><mi>x</mi></math>"##,
        ),
        // A foreign element's `xmlns` is junk like any other.
        (
            r#"<math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math>"#,
            "<math><mi>x</mi></math>",
        ),
        // A stray `<` or quote in a tag makes no attribute.
        (r#"<a href="x"<b>y</a>"#, r#"<a href="x">y</a>"#),
    ] {
        for _ in 0..8 {
            assert_eq!(normalize_html(&html.into()).await.as_str(), expected);
        }
    }
}

#[tokio::test]
async fn normalize_html_reads_a_document_from_its_first_tag_alone() {
    for (html, expected) in [
        // `<html>` inside an attribute doesn't make a fragment a document.
        (
            r#"<p title="<html>">x</p>"#,
            r#"<p title="&lt;html&gt;">x</p>"#,
        ),
        (
            "<textarea>&lt;!doctype html&gt;</textarea>",
            "<textarea>&lt;!doctype html&gt;</textarea>",
        ),
        // A document keeps its doctype, even after a comment, an empty
        // comment, a byte order mark, or an XML prolog.
        (
            "<!-- c --><!DOCTYPE html><html><body><p>x</p></body></html>",
            "<!DOCTYPE html><html><head></head><body><p>x</p></body></html>",
        ),
        (
            "\u{feff}<!--><!DOCTYPE html><html><body><p>x</p></body></html>",
            "<!DOCTYPE html><html><head></head><body><p>x</p></body></html>",
        ),
        (
            "\u{feff}<?xml version=\"1.0\"?><!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0//EN\" \"x.dtd\"><html><body><p>x</p></body></html>",
            "<!DOCTYPE html><html><head></head><body><p>x</p></body></html>",
        ),
        (
            "<!DOCTYPE><html><body><p>x</p></body></html>",
            "<!DOCTYPE><html><head></head><body><p>x</p></body></html>",
        ),
    ] {
        assert_eq!(
            normalize_html(&html.into()).await.as_str(),
            expected,
            "{html}"
        );
    }
}

#[tokio::test]
async fn normalize_html_keeps_a_table_fragments_structure() {
    for html in [
        "<tr><td>a</td><td>b</td></tr>",
        r#"<td nowrap="">y</td>"#,
        "<caption>c</caption><thead><tr><th>h</th></tr></thead>",
        "<tbody><tr><td>a</td></tr></tbody>",
        "<col><col>",
    ] {
        assert_eq!(normalize_html(&html.into()).await.as_str(), html);
    }
}

// Tests for normalize_urls()

#[test]
fn test_normalize_urls_exact_duplicates() {
    let input = vec![
        "https://example.com".to_string(),
        "https://example.com".to_string(),
        "https://example.com".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0], "https://example.com");
}

#[test]
fn test_normalize_urls_protocol_normalization() {
    let input = vec![
        "http://example.com".to_string(),
        "https://example.com".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1); // Both normalize to https
}

#[test]
fn test_normalize_urls_case_normalization() {
    let input = vec![
        "https://Example.com".to_string(),
        "https://EXAMPLE.COM".to_string(),
        "https://example.com".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_normalize_urls_www_stripping() {
    let input = vec![
        "https://www.example.com".to_string(),
        "https://example.com".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_normalize_urls_trailing_slash() {
    let input = vec![
        "https://example.com/path".to_string(),
        "https://example.com/path/".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_normalize_urls_query_param_order() {
    let input = vec![
        "https://example.com?b=2&a=1".to_string(),
        "https://example.com?a=1&b=2".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_normalize_urls_fragment_removal() {
    let input = vec![
        "https://example.com/page#section1".to_string(),
        "https://example.com/page#section2".to_string(),
        "https://example.com/page".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_normalize_urls_combined() {
    let input = vec![
        "https://example.com/path".to_string(),
        "HTTP://www.example.com/path/".to_string(),
        "https://EXAMPLE.COM/path".to_string(),
        "http://example.com/path#frag".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 1); // All canonicalize to same URL
}

#[test]
fn test_normalize_urls_preserves_order() {
    let input = vec![
        "https://example.com/first".to_string(),
        "https://example.com/second".to_string(),
        "https://example.com/first".to_string(), // Duplicate
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], "https://example.com/first");
    assert_eq!(output[1], "https://example.com/second");
}

#[test]
fn test_normalize_urls_returns_canonical() {
    // Returns canonical URL
    let input = vec!["HTTP://Example.com/Path".to_string()];
    let output = normalize_urls(&input);
    assert_eq!(output[0], "https://example.com/Path");
}

#[test]
fn test_normalize_urls_canonicalizes_idna_domains() {
    let input = vec!["https://münich.com/path".to_string()];
    let output = normalize_urls(&input);
    assert_eq!(output[0], "https://xn--mnich-kva.com/path");
}

#[test]
fn test_normalize_urls_malformed() {
    let input = vec![
        "not-a-url".to_string(),
        "https://example.com".to_string(),
        "also-not-url".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 3); // Malformed URLs kept as-is, all different
}

#[test]
fn test_normalize_urls_empty_list() {
    let input: Vec<String> = vec![];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 0);
}

// Tests for normalize_emails()

#[test]
fn test_normalize_emails_deduplication() {
    let input = vec![
        " john@example.com ".to_string(),
        "John@Example.COM".to_string(),
        "\"John Doe\" <john@example.com>".to_string(),
    ];
    let output = normalize_emails(&input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0], "john@example.com");
}

#[test]
fn test_normalize_emails_with_punctuation() {
    // Test CLI use case where emails have trailing punctuation
    let input = vec![
        "john@example.com,".to_string(),
        "john@example.com".to_string(),
    ];
    let output = normalize_emails(&input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0], "john@example.com");
}

#[test]
fn test_normalize_emails_filters_empty() {
    let input = vec![
        "john@example.com".to_string(),
        "".to_string(),
        "jane@example.com".to_string(),
        "   ".to_string(),
    ];
    let output = normalize_emails(&input);
    assert_eq!(output.len(), 2);
}

#[test]
fn test_normalize_emails_preserves_order() {
    let input = vec![
        "first@example.com".to_string(),
        "second@example.com".to_string(),
        "first@example.com".to_string(), // Duplicate
    ];
    let output = normalize_emails(&input);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], "first@example.com");
    assert_eq!(output[1], "second@example.com");
}

#[test]
fn test_normalize_emails_empty_list() {
    let input: Vec<String> = vec![];
    let output = normalize_emails(&input);
    assert_eq!(output.len(), 0);
}

// Tests for normalize_phones()

#[test]
fn test_normalize_phones_deduplication() {
    let input = vec![
        "(555) 123-4567".to_string(),
        "555-123-4567".to_string(),
        "555.123.4567".to_string(),
    ];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0], "5551234567");
}

#[test]
fn normalize_phones_drops_rfc_3966_parameters() {
    assert_eq!(
        normalize_phones(&["+1-212-555-1212;ext=12".to_string()]),
        ["+12125551212"]
    );
}

#[test]
fn test_normalize_phones_filters_empty() {
    let input = vec![
        "555-123-4567".to_string(),
        "".to_string(),
        "555-987-6543".to_string(),
        "   ".to_string(),
    ];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 2);
}

#[test]
fn test_normalize_phones_preserves_order() {
    let input = vec![
        "555-123-4567".to_string(),
        "555-987-6543".to_string(),
        "(555) 123-4567".to_string(), // Duplicate
    ];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], "5551234567");
    assert_eq!(output[1], "5559876543");
}

#[test]
fn test_normalize_phones_with_extensions() {
    let input = vec![
        "555-123-4567 ext. 123".to_string(),
        "555-123-4567 x456".to_string(),
    ];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 1); // Same after stripping extensions
    assert_eq!(output[0], "5551234567");
}

#[test]
fn test_normalize_phones_international_vs_local() {
    // International and local versions should be treated as different
    let input = vec!["+1-555-123-4567".to_string(), "555-123-4567".to_string()];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], "+15551234567");
    assert_eq!(output[1], "5551234567");
}

#[test]
fn test_normalize_phones_empty_list() {
    let input: Vec<String> = vec![];
    let output = normalize_phones(&input);
    assert_eq!(output.len(), 0);
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
fn test_normalize_domain() {
    // Basic lowercase
    assert_eq!(canonical_domain("Example.com"), "example.com");
    assert_eq!(canonical_domain("GITHUB.COM"), "github.com");
    // Strip www
    assert_eq!(canonical_domain("www.example.com"), "example.com");
    assert_eq!(canonical_domain("WWW.EXAMPLE.COM"), "example.com");
    // Combined
    assert_eq!(canonical_domain("WWW.GitHub.com"), "github.com");
    // Don't strip www if it's the whole domain
    assert_eq!(canonical_domain("www"), "www");
    // Subdomains preserved
    assert_eq!(canonical_domain("api.example.com"), "api.example.com");
    assert_eq!(canonical_domain("www.api.example.com"), "api.example.com");
}

#[test]
fn test_normalize_url() {
    // Protocol normalization
    assert_eq!(normalize_url("http://example.com"), "https://example.com");
    assert_eq!(normalize_url("example.com"), "https://example.com");
    assert_eq!(normalize_url("www.example.com"), "https://example.com");
    // Domain normalization
    assert_eq!(
        normalize_url("https://WWW.Example.COM"),
        "https://example.com"
    );
    // Trailing slash normalization
    assert_eq!(
        normalize_url("https://example.com/path/"),
        "https://example.com/path"
    );
    assert_eq!(normalize_url("https://example.com/"), "https://example.com");
    // Query parameter sorting
    assert_eq!(
        normalize_url("https://example.com?b=2&a=1"),
        "https://example.com/?a=1&b=2"
    );
    // Fragment removal
    assert_eq!(
        normalize_url("https://example.com/page#section"),
        "https://example.com/page"
    );
    // Combined
    assert_eq!(
        normalize_url("HTTP://www.Example.COM/path/?b=2&a=1#frag"),
        "https://example.com/path?a=1&b=2"
    );
    // Malformed URLs kept as-is
    assert_eq!(normalize_url("not-a-url"), "not-a-url");
}

#[test]
fn test_canonical_url_new_canonicalizes() {
    let url = CanonicalUrl::new("HTTP://www.Example.COM/path/?utm_source=x&id=7");
    assert_eq!(url.as_str(), "https://example.com/path?id=7");
}

#[test]
fn test_canonical_url_idempotent() {
    let once = CanonicalUrl::new("https://Example.com/");
    let twice = CanonicalUrl::new(once.as_str());
    assert_eq!(once, twice);
}

#[test]
fn test_canonical_url_serialize_transparent() {
    let url = CanonicalUrl::new("https://example.com");
    let json = serde_json::to_string(&url).unwrap();
    // Bare string, not a wrapper object.
    assert_eq!(json, "\"https://example.com\"");
}

#[test]
fn test_canonical_url_deserialize_strict() {
    // Non-canonical input gets canonicalized on deserialize.
    let url: CanonicalUrl =
        serde_json::from_str("\"HTTP://www.Example.COM/path/?utm_source=x\"").unwrap();
    assert_eq!(url.as_str(), "https://example.com/path");
}

#[test]
fn test_canonical_url_from_str_and_string() {
    let a: CanonicalUrl = "https://Example.com/".into();
    let b: CanonicalUrl = String::from("https://Example.com/").into();
    assert_eq!(a, b);
    assert_eq!(a.as_str(), "https://example.com");
}

#[test]
fn test_normalize_url_strips_tracking_params() {
    // utm_* family stripped entirely
    assert_eq!(
        normalize_url("https://example.com?utm_source=x&utm_medium=y"),
        "https://example.com"
    );
    // tracking mixed with real params — real params preserved and sorted
    assert_eq!(
        normalize_url("https://example.com?utm_source=x&id=7"),
        "https://example.com/?id=7"
    );
    // each exact-match tracking param
    for param in &[
        "fbclid", "gclid", "mc_eid", "mc_cid", "_ga", "igshid", "ref_src", "ref_url",
    ] {
        assert_eq!(
            normalize_url(&format!("https://example.com?{}=abc&foo=1", param)),
            "https://example.com/?foo=1",
            "tracking param {} should be stripped",
            param
        );
    }
    // look-alikes NOT stripped
    assert_eq!(
        normalize_url("https://example.com?referral=abc"),
        "https://example.com/?referral=abc"
    );
    assert_eq!(
        normalize_url("https://example.com?utmx=abc"),
        "https://example.com/?utmx=abc"
    );
    // bare `ref` is intentionally preserved (legitimate query param on
    // many sites — e.g. GitHub `?ref=branch`)
    assert_eq!(
        normalize_url("https://example.com?ref=branch"),
        "https://example.com/?ref=branch"
    );
    // two "same" URLs differing only by trackers canonicalize identically
    assert_eq!(
        normalize_url("https://example.com/recipe?fbclid=abc"),
        normalize_url("https://example.com/recipe?utm_source=fb"),
    );
}

#[test]
fn test_normalize_email() {
    // Trim whitespace
    assert_eq!(normalize_email(" john@example.com "), "john@example.com");
    // Lowercase
    assert_eq!(normalize_email("John@Example.COM"), "john@example.com");
    // URL decode
    assert_eq!(normalize_email("john%40example.com"), "john@example.com");
    // Extract from display name
    assert_eq!(
        normalize_email("\"John Doe\" <john@example.com>"),
        "john@example.com"
    );
    assert_eq!(
        normalize_email("John Doe <john@example.com>"),
        "john@example.com"
    );
    // Strip trailing punctuation
    assert_eq!(normalize_email("john@example.com,"), "john@example.com");
    assert_eq!(normalize_email("john@example.com;"), "john@example.com");
    assert_eq!(normalize_email("john@example.com."), "john@example.com");
    // Combined
    assert_eq!(
        normalize_email(" \"John\" <John@Example.COM> "),
        "john@example.com"
    );
    // Already normalized
    assert_eq!(normalize_email("john@example.com"), "john@example.com");
}

#[test]
fn normalize_email_keeps_long_gtld() {
    // Reconciled with `extract`'s email regex (TLD up to 24 chars): a long gTLD
    // is no longer extracted and then silently dropped here.
    assert_eq!(
        normalize_email("hello@studio.photography"),
        "hello@studio.photography"
    );
    assert_eq!(
        normalize_email("team@acme.international"),
        "team@acme.international"
    );
}

#[test]
fn test_normalize_phone() {
    // Strip separators
    assert_eq!(normalize_phone("(555) 123-4567"), "5551234567");
    assert_eq!(normalize_phone("555-123-4567"), "5551234567");
    assert_eq!(normalize_phone("555.123.4567"), "5551234567");
    // Keep international prefix
    assert_eq!(normalize_phone("+1-555-123-4567"), "+15551234567");
    assert_eq!(normalize_phone("+1 (555) 123-4567"), "+15551234567");
    // Strip extensions
    assert_eq!(normalize_phone("555-123-4567 ext. 123"), "5551234567");
    assert_eq!(normalize_phone("555-123-4567 x123"), "5551234567");
    assert_eq!(normalize_phone("555-123-4567 extension 123"), "5551234567");
    // Trim whitespace
    assert_eq!(normalize_phone(" 555-123-4567 "), "5551234567");
    // Already normalized
    assert_eq!(normalize_phone("5551234567"), "5551234567");
}

#[test]
fn test_normalize_social_urls_dedups_without_lowercasing() {
    let input = vec![
        "https://youtube.com/@NASA".to_string(),
        "https://youtube.com/@NASA".to_string(),
        "https://instagram.com/nasa".to_string(),
    ];
    let output = normalize_urls(&input);
    assert_eq!(output.len(), 2);
    // Case preserved — `@NASA` must NOT be lowercased (unlike emails).
    assert!(output.contains(&"https://youtube.com/@NASA".to_string()));
}

// Tests for normalize_url() — it must reproduce classify's prior canonical
// forms for social URLs (the basis for classify dropping its own canonicalize),
// the only intended delta being sorted query params.

#[test]
fn test_normalize_social_strips_share_tokens() {
    // YouTube `si` and Twitter `s` are social share tokens `normalize_url` keeps
    // but `normalize_social` drops.
    assert_eq!(
        normalize_url("https://youtu.be/dQw4w9WgXcQ?si=trackingtoken"),
        "https://youtu.be/dQw4w9WgXcQ"
    );
    assert_eq!(
        normalize_url("https://x.com/jack/status/20?s=20"),
        "https://x.com/jack/status/20"
    );
}

#[test]
fn test_normalize_social_strips_mobile_prefix() {
    // `m.`/`mobile.` are stripped for social hosts — but not the `vm.` shortener.
    assert_eq!(
        normalize_url("https://m.facebook.com/zuck"),
        "https://facebook.com/zuck"
    );
    assert_eq!(
        normalize_url("https://vm.tiktok.com/ZMabcdef/"),
        "https://vm.tiktok.com/ZMabcdef"
    );
}

#[test]
fn test_normalize_social_is_web_safe() {
    // A non-social host gets only the general pass — a functional `?s=` and an
    // `m.` host survive (no platform → no social rules).
    assert_eq!(
        normalize_url("https://example.com/search?s=recipes"),
        "https://example.com/search?s=recipes"
    );
    assert_eq!(
        normalize_url("https://m.example.com/page"),
        "https://m.example.com/page"
    );
}

#[test]
fn test_normalize_social_matches_classify_forms() {
    // Equivalence with classify's prior canonicalization; the only delta is
    // sorted query params (classify preserved input order).
    assert_eq!(
        normalize_url("https://www.tiktok.com/@user/video/123"),
        "https://tiktok.com/@user/video/123"
    );
    assert_eq!(
        normalize_url("https://www.instagram.com/p/Cabc/?igshid=xyz"),
        "https://instagram.com/p/Cabc"
    );
    assert_eq!(
        normalize_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30"),
        "https://youtube.com/watch?t=30&v=dQw4w9WgXcQ"
    );
}

#[test]
fn normalize_url_keeps_percent_encoded_query_values() {
    let url = "https://example.com/p?q=a%26b&x=1%2B2";
    assert_eq!(normalize_url(url), url);
    assert_eq!(normalize_url(&normalize_url(url)), url);
}

#[test]
fn normalize_url_keeps_repeated_query_keys_in_order() {
    assert_eq!(
        normalize_url("https://example.com/p?id=2&a=0&id=1"),
        "https://example.com/p?a=0&id=2&id=1"
    );
}

#[test]
fn normalize_url_leaves_non_http_inputs_unchanged() {
    for input in ["mailto:a@b.com", "/about.html", "ftp://files.example.com/a"] {
        assert_eq!(normalize_url(input), input);
    }
}

#[test]
fn normalize_url_keeps_percent_encoded_query_values_on_a_social_host() {
    assert_eq!(
        normalize_url("https://youtube.com/results?search_query=a%26b&si=x"),
        "https://youtube.com/results?search_query=a%26b"
    );
}

#[test]
fn normalize_url_is_idempotent() {
    for input in [
        "https://example.com/?/",
        "https://example.com/?next=/",
        "https://example.com/a/?redirect=/login/",
        "https://m.m.facebook.com/zuck",
        "https://m.www.facebook.com/zuck",
        "https://mobile.m.twitter.com/jack",
        "https://www.www.example.com/a",
        "HTTP://WWW.Example.com//a//?b=2&a=1#frag",
    ] {
        let once = normalize_url(input);
        assert_eq!(normalize_url(&once), once, "{input}");
    }
    assert_eq!(
        normalize_url("https://example.com/a/?redirect=/login/"),
        "https://example.com/a?redirect=/login/"
    );
}

#[tokio::test]
async fn normalize_html_strips_attributes_in_tags_not_in_text() {
    let html = r#"<p class="x" id="y">Set role = admin and id=5 then style = bold</p><script>var a = 1;</script><script type="application/ld+json">{"name":"x", "text":"a class = b"}</script><p>after</p>"#;
    let normalized = normalize_html(&html.into()).await;
    assert_eq!(
        normalized.as_str(),
        r#"<p>Set role = admin and id=5 then style = bold</p><script type="application/ld+json">{"name":"x", "text":"a class = b"}</script><p>after</p>"#
    );
}

#[test]
fn absolute_url_completes_what_names_an_http_url_and_refuses_the_rest() {
    for (input, expected) in [
        (
            "http://www.Example.com/a/",
            Some("http://www.Example.com/a/"),
        ),
        ("example.com/a", Some("https://example.com/a")),
        (
            "my_host.example.com/a",
            Some("https://my_host.example.com/a"),
        ),
        ("localhost:8080/a", Some("https://localhost:8080/a")),
        ("https:example.com", Some("https://example.com/")),
        ("HTTPS:example.com/a", Some("https://example.com/a")),
        ("mailto:a@b.com", None),
        ("ftp://example.com/file.txt", None),
        ("/about.html", None),
        ("a@b.com", None),
        ("not a url", None),
    ] {
        assert_eq!(absolute_url(input).as_deref(), expected, "{input}");
    }
}

#[test]
fn dedupe_urls_keeps_the_first_address_of_each_page() {
    let urls = [
        "http://www.example.com/a/",
        "https://example.com/a?utm_source=x",
        "",
        "https://example.com/b",
    ]
    .map(String::from);
    assert_eq!(
        dedupe_urls(&urls),
        ["http://www.example.com/a/", "https://example.com/b"]
    );
}

#[test]
fn canonical_domain_never_reduces_a_host_to_a_tld() {
    assert_eq!(canonical_domain("www.com"), "www.com");
    assert_eq!(canonical_domain("WWW.Example.com."), "example.com");
    assert_eq!(normalize_url("https://www.com/a"), "https://www.com/a");
}

#[test]
fn normalize_emails_and_phones_take_any_string_extract_returns() {
    // A `>` before the `<` is not a display-name bracket; it must not panic.
    normalize_emails(&[">x<a@b.com".to_string()]);
    assert_eq!(
        normalize_emails(&["Jane <jane@Example.com>".to_string()]),
        ["jane@example.com"]
    );
    assert!(normalize_phones(&["İİİ x".to_string()]).is_empty());
    assert_eq!(
        normalize_phones(&["555-123-4567 X12".to_string()]),
        ["5551234567"]
    );
}

#[test]
fn absolute_url_refuses_schemes_and_numbers_that_look_like_hosts() {
    for input in [
        "tel:911",
        "sms:12345",
        "javascript:1",
        "http:8080",
        "3.14",
        ".",
        "e.g.",
        ":pw@example.com",
    ] {
        assert_eq!(absolute_url(input), None, "{input}");
    }
    assert_eq!(
        absolute_url("example.com:8080/a").as_deref(),
        Some("https://example.com:8080/a")
    );
    assert_eq!(
        absolute_url("127.0.0.1:8080").as_deref(),
        Some("https://127.0.0.1:8080")
    );
}

#[tokio::test]
async fn normalize_html_keeps_jsonld_bytes_and_survives_hostile_markup() {
    let cases = [
        // A script that only mentions JSON-LD is a script.
        (
            r#"<p>a</p><script>document.write('<script type="application/ld+json">')</script><p>b</p>"#,
            "<p>a</p><p>b</p>",
        ),
        // JSON-LD is kept byte for byte: escapes, HTML inside strings.
        (
            r#"<script type="application/ld+json">{"path":"C:\\new","body":"<p class=\"lead big\">x</p>"}</script>"#,
            r#"<script type="application/ld+json">{"path":"C:\\new","body":"<p class=\"lead big\">x</p>"}</script>"#,
        ),
        (
            r#"<script type=application/ld+json>{"a":1}</script>"#,
            r#"<script type="application/ld+json">{"a":1}</script>"#,
        ),
        // A comment can't open a script.
        (
            "<!-- <script> --><p>real text</p><script>x()</script><p>after</p>",
            "<p>real text</p><p>after</p>",
        ),
        // A quoted value keeps its spaces and equals signs.
        (
            r#"<a class="x" href="/a?x=1 id=5">go</a>"#,
            r#"<a href="/a?x=1 id=5">go</a>"#,
        ),
    ];
    for (html, expected) in cases {
        assert_eq!(
            normalize_html(&html.into()).await.as_str(),
            expected,
            "{html}"
        );
    }
}

#[test]
fn absolute_url_takes_any_host_behind_an_explicit_scheme() {
    for input in [
        "https://пример.рф",
        "http://example.xn--p1ai",
        "http://intranet/",
        "http://printer/status",
        "http://intranet:8080/a",
        "http://[::1]:8080/a",
        "HTTPS://Example.com/a",
        "http://user:pass@example.com/a",
    ] {
        assert_eq!(absolute_url(input).as_deref(), Some(input), "{input}");
    }
}

#[test]
fn absolute_url_reads_a_schemeless_host_with_port_query_or_fragment() {
    for (input, expected) in [
        ("[::1]:8080/a", "https://[::1]:8080/a"),
        ("LOCALHOST:8080", "https://LOCALHOST:8080"),
        ("example.com?q=a:b", "https://example.com?q=a:b"),
        ("example.com#a:b", "https://example.com#a:b"),
        ("example.xn--p1ai/a", "https://example.xn--p1ai/a"),
        ("example.com./a", "https://example.com./a"),
    ] {
        assert_eq!(absolute_url(input).as_deref(), Some(expected), "{input}");
    }
    for input in ["3.14/v1.2.3", "intranet", "intranet:8080", "page", "1.5"] {
        assert_eq!(absolute_url(input), None, "{input}");
    }
}

#[tokio::test]
async fn normalize_html_reads_comments_and_scripts_leftmost_first() {
    let cases = [
        (
            r#"<p>a</p><script>var s="<!--";</script><p>keep me</p><!-- c --><p>b</p>"#,
            "<p>a</p><p>keep me</p><p>b</p>",
        ),
        (
            "<p>a</p><svg><script>x()</script><path/></svg><p>b</p>",
            "<p>a</p><p>b</p>",
        ),
        (
            r#"<p>a</p><script type="application/ld+jsonp">evil()</script>"#,
            "<p>a</p>",
        ),
        (
            r#"<p>a</p><script data-type="application/ld+json">evil()</script>"#,
            "<p>a</p>",
        ),
        (
            "<p>a</p> <script>x()</script> <p>b</p>",
            "<p>a</p> <p>b</p>",
        ),
        ("<p>a</p><script>x()</script ><p>b</p>", "<p>a</p><p>b</p>"),
        // Markup a browser reads as text stays text; no script comes out.
        (
            "<scr<script></script>ipt>alert(1)</script><p>b</p>",
            "<scr<script>ipt&gt;alert(1)<p>b</p></scr<script>",
        ),
        (
            "<p>a</p><script/src=x>evil()</script><p>b</p>",
            "<p>a</p><p>b</p>",
        ),
        (
            r#"<svg class="i"/><p>text</p><script type="application/ld+json">{"a":1}</script><svg><path/></svg><p>b</p>"#,
            r#"<p>text</p><script type="application/ld+json">{"a":1}</script><p>b</p>"#,
        ),
        (
            r#"<script type="text/javascript" type="application/ld+json">evil()</script><p>b</p>"#,
            "<p>b</p>",
        ),
        (
            r#"<script type="application/ld+json; charset=utf-8">{"a":1}</script>"#,
            r#"<script type="application/ld+json; charset=utf-8">{"a":1}</script>"#,
        ),
        (
            r#"<script type = "application/ld+json">{"articleBody":"a <!-- wp --> b"}</script>"#,
            r#"<script type="application/ld+json">{"articleBody":"a <!-- wp --> b"}</script>"#,
        ),
    ];
    for (html, expected) in cases {
        assert_eq!(
            normalize_html(&html.into()).await.as_str(),
            expected,
            "{html}"
        );
    }
}

#[tokio::test]
async fn normalize_html_is_linear_on_nested_comment_and_script_openers() {
    let layers = 20_000;
    let comments = format!(
        "{}<!---->{}<p>b</p>",
        "<!-".repeat(layers),
        "-->".repeat(layers)
    );
    let scripts = format!("{}<p>b</p>", "<scr".repeat(layers * 5));
    for html in [comments, scripts] {
        let started = std::time::Instant::now();
        normalize_html(&html.as_str().into()).await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }
}

#[test]
fn absolute_url_refuses_text_that_only_resembles_a_host() {
    for input in [
        "\"example.com\"",
        "example.com;x",
        "1.e2",
        "example.c0m",
        "example.com:",
        "",
    ] {
        assert_eq!(absolute_url(input), None, "{input}");
    }
    assert_eq!(
        absolute_url("http:/example.com").as_deref(),
        Some("http://example.com/")
    );
}
