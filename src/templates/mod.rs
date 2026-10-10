//! Templates
//!
//! Compositions of the tools, one per file. [`qrawl_any_url`] is the one
//! template for any URL; [`qrawl_many_urls`] runs it over a crawl. The rest
//! are raw-HTML crawls and an example. Private helpers shared across templates
//! live in `utils`; all tests live in `tests`.

mod types;
mod utils;

mod qrawl_any_url;
mod qrawl_children;
mod qrawl_children_stream;
mod qrawl_discover_children;
mod qrawl_emails;
mod qrawl_fetch_stream;
mod qrawl_many_urls;

#[cfg(test)]
mod tests;

pub use qrawl_any_url::qrawl_any_url;
pub use qrawl_children::qrawl_children;
pub use qrawl_children_stream::qrawl_children_stream;
pub use qrawl_discover_children::qrawl_discover_children;
pub use qrawl_emails::qrawl_emails;
pub use qrawl_fetch_stream::qrawl_fetch_stream;
pub use qrawl_many_urls::qrawl_many_urls;
pub use types::PageContent;
