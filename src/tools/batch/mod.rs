//! Batch Tools

mod tests;

use futures_util::stream::{self, StreamExt};

/// Batch execute async operations with bounded concurrency.
pub async fn batch<T, F, Fut, R>(items: Vec<T>, concurrency: usize, operation: F) -> Vec<R>
where
    T: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    stream::iter(items)
        .map(operation)
        .buffer_unordered(concurrency)
        .collect()
        .await
}
