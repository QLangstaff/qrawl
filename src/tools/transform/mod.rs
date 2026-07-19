//! Transform Tools

mod tests;

use htmd::HtmlToMarkdown;

use crate::types::{Html, Markdown};

/// Transform HTML to Markdown suitable for LLM input.
pub async fn transform_markdown(html: &Html) -> Markdown {
    let html = html.clone();
    // htmd parses the DOM (CPU-bound), so convert off the async runtime.
    tokio::task::spawn_blocking(move || {
        let markdown = HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style", "head", "noscript", "iframe", "svg"])
            .build()
            .convert(html.as_str())
            .unwrap_or_default();
        Markdown::new(markdown)
    })
    .await
    .expect("transform_markdown: spawn_blocking failed")
}
