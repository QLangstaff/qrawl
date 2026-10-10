//! Templates
//!
//! Compositions of the tools. [`qrawl_any_url`] turns any URL into one
//! [`PageContent`]; [`qrawl_many_urls`] runs it over many URLs and their
//! children. Private helpers live in `utils`; all tests live in `tests`.

mod types;
mod utils;

mod qrawl_any_url;
mod qrawl_many_urls;

#[cfg(test)]
mod tests;

pub use qrawl_any_url::qrawl_any_url;
pub use qrawl_many_urls::qrawl_many_urls;
pub use types::{CrawlOptions, PageContent, MAX_DEPTH};
