#![cfg(test)]
use crate::tools::transform::*;

#[tokio::test]
async fn converts_headings_and_emphasis() {
    let md = transform_markdown(&"<h1>Title</h1><p>Hello <strong>world</strong>.</p>".into()).await;
    assert!(md.as_str().contains("# Title"), "got: {md:?}");
    assert!(md.as_str().contains("**world**"), "got: {md:?}");
}

#[tokio::test]
async fn skips_script_and_style() {
    let html = r#"<style>.x{color:red}</style><p>Visible</p><script>alert(1)</script>"#;
    let md = transform_markdown(&html.into()).await;
    assert!(md.as_str().contains("Visible"), "got: {md:?}");
    assert!(!md.as_str().contains("color:red"), "got: {md:?}");
    assert!(!md.as_str().contains("alert"), "got: {md:?}");
}

#[tokio::test]
async fn preserves_links_and_lists() {
    let html = r#"<ul><li><a href="https://ex.com">link</a></li><li>two</li></ul>"#;
    let md = transform_markdown(&html.into()).await;
    assert!(
        md.as_str().contains("[link](https://ex.com)"),
        "got: {md:?}"
    );
    assert!(md.as_str().contains("two"), "got: {md:?}");
}

#[tokio::test]
async fn empty_input_yields_empty_output() {
    assert_eq!(transform_markdown(&"".into()).await.as_str(), "");
}
