//! Transform Tools

mod tests;
mod types;

use htmd::HtmlToMarkdown;

use crate::types::Html;
use crate::utils::with_document;
pub use types::Markdown;

/// Transform HTML to Markdown suitable for LLM input.
pub async fn transform_markdown(html: &Html) -> Markdown {
    with_document(html, |document| {
        let markdown = HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style", "head", "noscript", "iframe", "svg"])
            .build()
            .convert(&document.html())
            .unwrap_or_default();
        Markdown::new(markdown)
    })
    .await
}
