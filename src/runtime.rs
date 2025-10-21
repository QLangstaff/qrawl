//! Shared Runtime

use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};

/// Global multi-thread runtime reused across the crate.
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build global runtime")
});

/// Run a future to completion on the shared runtime.
pub fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    RUNTIME.block_on(future)
}
