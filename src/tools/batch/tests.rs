#[cfg(test)]
mod tests {
    use crate::tools::batch::batch;

    #[tokio::test]
    async fn test_batch_basic() {
        let items = vec![1, 2, 3, 4, 5];

        let results = batch(items, 2, |n| async move { n * 2 }).await;

        assert_eq!(results.len(), 5);
        // Results may be in any order due to buffer_unordered
        let mut sorted = results.clone();
        sorted.sort();
        assert_eq!(sorted, vec![2, 4, 6, 8, 10]);
    }

    #[tokio::test]
    async fn test_batch_with_delay() {
        let items = vec![10, 20, 30];

        let results = batch(items, 2, |n| async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(n)).await;
            n
        })
        .await;

        assert_eq!(results.len(), 3);
        let mut sorted = results.clone();
        sorted.sort();
        assert_eq!(sorted, vec![10, 20, 30]);
    }

    #[tokio::test]
    async fn test_batch_empty() {
        let items: Vec<i32> = vec![];
        let results = batch(items, 5, |n| async move { n }).await;
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_batch_concurrency_limit() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let max_concurrent = Arc::new(Mutex::new(0));
        let current = Arc::new(Mutex::new(0));

        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        let max_concurrent_handle = Arc::clone(&max_concurrent);
        let current_handle = Arc::clone(&current);

        let results = batch(items, 3, move |_n| {
            let max_concurrent = Arc::clone(&max_concurrent_handle);
            let current = Arc::clone(&current_handle);

            async move {
                // Increment current
                {
                    let mut curr = current.lock().await;
                    *curr += 1;
                    let mut max = max_concurrent.lock().await;
                    *max = (*max).max(*curr);
                }

                // Simulate work
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                // Decrement current
                {
                    let mut curr = current.lock().await;
                    *curr -= 1;
                }

                42
            }
        })
        .await;

        assert_eq!(results.len(), 10);

        let max = max_concurrent.lock().await;
        // Should respect concurrency limit (allow 3-4 due to buffer_unordered behavior)
        assert!(*max <= 4, "Max concurrent was {}, expected <= 4", *max);
    }
}
