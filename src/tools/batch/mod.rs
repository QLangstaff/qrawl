//! Batch execution with bounded concurrency.
//!
//! See README.md for full documentation.

#[cfg(test)]
mod tests;

use futures_util::stream::{self, StreamExt};

/// Batch execute async operations with bounded concurrency.
///
/// Processes items concurrently with a configurable limit, yielding results
/// as they complete (unordered). Uses `buffer_unordered` for optimal I/O performance.
///
/// # Arguments
/// * `items` - Items to process
/// * `concurrency` - Max concurrent operations (recommended: 10-50 for network I/O)
/// * `operation` - Async function to apply to each item
///
/// # Returns
/// Vec of results in completion order (not input order)
///
/// # Examples
/// ```rust
/// use qrawl::tools::batch::batch;
/// use qrawl::tools::fetch::fetch;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let urls = vec![
///     "https://example.com/1",
///     "https://example.com/2",
///     "https://example.com/3",
/// ];
///
/// // Fetch all URLs with max 10 concurrent requests
/// let results = batch(urls, 10, |url| async move {
///     fetch(url).await
/// }).await;
/// # Ok(())
/// # }
/// ```
pub async fn batch<T, F, Fut, R>(items: Vec<T>, concurrency: usize, operation: F) -> Vec<R>
where
    F: Fn(T) -> Fut,
    Fut: std::future::Future<Output = R>,
{
    stream::iter(items)
        .map(operation)
        .buffer_unordered(concurrency)
        .collect()
        .await
}
