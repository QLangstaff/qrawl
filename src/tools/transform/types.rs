use serde::Serialize;

/// LLM-ready Markdown — produced by
/// [`transform_markdown`](super::transform_markdown).
///
/// A *terminal* substrate: once HTML is rendered to Markdown, the
/// `scrape`/`extract`/`map` tools can no longer operate on it. Follows the same
/// string-newtype conventions as [`Html`](crate::types::Html) (construct via `new`; read via
/// `as_str` / `into_inner`; `AsRef<str>` + `Display`, no `Deref`; transparent
/// JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Markdown(String);

impl Markdown {
    pub fn new(markdown: String) -> Self {
        Markdown(markdown)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for Markdown {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Markdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
