//! Example Templates
//!
//! Each `qrawl_*` template is a composition of the leaf tools, one per file.
//! Private helpers shared across templates live in `utils`; all tests live in
//! `tests`.

mod utils;

mod qrawl_any_url;
mod qrawl_children;
mod qrawl_children_stream;
mod qrawl_discover_children;
mod qrawl_emails;
mod qrawl_fetch_stream;
mod qrawl_many_urls;
mod qrawl_schema;
mod qrawl_share;

#[cfg(test)]
mod tests;

pub use qrawl_any_url::qrawl_any_url;
pub use qrawl_children::qrawl_children;
pub use qrawl_children_stream::qrawl_children_stream;
pub use qrawl_discover_children::qrawl_discover_children;
pub use qrawl_emails::qrawl_emails;
pub use qrawl_fetch_stream::qrawl_fetch_stream;
pub use qrawl_many_urls::qrawl_many_urls;
pub use qrawl_schema::qrawl_schema;
pub use qrawl_share::qrawl_share;
