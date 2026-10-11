//! Shared Macros

/// Compose tools into one crawl. `chain! { urls, context => a -> b -> c }` threads a
/// collection through each step, fanning per-item work out with bounded
/// concurrency and feeding each step's output into the next. Each `a -> b` is
/// type-checked by the compiler (feeding a URL list where pages are expected
/// won't compile). Runs as one crawl ([`Context::scope`](crate::types::Context::scope));
/// evaluates to the final step's value — use it inside an `async fn`.
///
/// The step vocabulary is **open**: every combinator takes *any* tool — a
/// built-in, or your own function/closure with the matching signature — so you
/// can build your own pipelines, not just arrange blessed names. The tool paths
/// resolve in *your* scope, so bring them into scope with `use`.
///
/// | step | tool signature | flows |
/// |------|----------------|-------|
/// | `all(f)` | `fn(&[String]) -> Vec<String>` | urls → urls (whole list) |
/// | `fetch` | — (the built-in I/O primitive) | urls → pages |
/// | `map(f)` | `async fn(&Html, &str) -> Vec<String>` | pages → urls |
/// | `extract(f)` | `async fn(&Html) -> Vec<String>` | pages → list |
/// | `filter(p)` | `async fn(Html) -> bool` | pages → pages |
///
/// `fetch` requests each URL exactly as the step before it produced it, so
/// dedupe with `all(dedupe_urls)`, which keeps URLs as written.
/// `all(normalize_urls)` rewrites each URL to its canonical form (`https`, no
/// `www.`, no trailing slash), which a few servers don't answer.
///
/// A "page" is a `(String, Html)` — the URL that served it and its body. `map`/`extract`/
/// `filter` fan their tool out across the pages with bounded concurrency; a
/// capturing closure is cloned per page, so it must be `Clone + Send + Sync`.
///
/// ```no_run
/// use qrawl::chain;
/// use qrawl::tools::extract::extract_emails;
/// use qrawl::tools::map::map_children;
/// use qrawl::tools::normalize::{dedupe_urls, normalize_emails};
/// use qrawl::types::Context;
///
/// # async fn run(urls: Vec<String>) {
/// let emails = chain! {
///     urls, Context::auto() =>
///     all(dedupe_urls) -> fetch -> map(map_children) ->
///     all(dedupe_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
/// };
/// # }
/// ```
#[macro_export]
macro_rules! chain {
    ($urls:expr, $context:expr => $($rest:tt)+) => {{
        let urls = $urls;
        $context.scope(async move { $crate::chain!(@run urls, $($rest)+) }).await
    }};

    // Thread the collection through each step, left to right. A step is either a
    // combinator `name(tool)` or the bare `fetch` primitive.
    (@run $items:expr, $comb:ident ( $arg:expr ) -> $($rest:tt)+) => {{
        let items = $crate::chain!(@op $items, $comb($arg));
        $crate::chain!(@run items, $($rest)+)
    }};
    (@run $items:expr, $step:ident -> $($rest:tt)+) => {{
        let items = $crate::chain!(@op $items, $step);
        $crate::chain!(@run items, $($rest)+)
    }};
    (@run $items:expr, $comb:ident ( $arg:expr )) => {
        $crate::chain!(@op $items, $comb($arg))
    };
    (@run $items:expr, $step:ident) => { $crate::chain!(@op $items, $step) };

    // The one I/O primitive: fetch each URL, drop failures, pair with its body.
    (@op $items:expr, fetch) => {
        $crate::tools::batch::batch($items, $crate::types::get_concurrency(), |url: String| async move {
            $crate::tools::fetch::fetch_strategy(&url).await.ok().map(|fetched| (fetched.url, fetched.html))
        })
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<(String, $crate::types::Html)>>()
    };

    // Whole-collection transform (normalize/dedupe): `fn(&[String]) -> Vec<String>`.
    (@op $items:expr, all($tool:expr)) => { $tool(&$items) };

    // Pages → urls, fanned: `async fn(&Html, &str) -> Vec<String>` (needs the url).
    (@op $items:expr, map($tool:expr)) => {{
        let tool = $tool;
        $crate::tools::batch::batch($items, $crate::types::get_concurrency(), move |(url, html): (String, $crate::types::Html)| {
            let tool = tool.clone();
            async move { tool(&html, &url).await }
        })
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<String>>()
    }};

    // Pages → flat list, fanned: `async fn(&Html) -> Vec<String>`.
    (@op $items:expr, extract($tool:expr)) => {{
        let tool = $tool;
        $crate::tools::batch::batch($items, $crate::types::get_concurrency(), move |(_url, html): (String, $crate::types::Html)| {
            let tool = tool.clone();
            async move { tool(&html).await }
        })
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<String>>()
    }};

    // Pages → pages, keep those the async predicate accepts: `async fn(Html) -> bool`.
    // The predicate takes the body by value (its future owns it), so a capturing
    // closure sidesteps the borrow-across-await lifetime that `&Html` would impose.
    (@op $items:expr, filter($pred:expr)) => {{
        let pred = $pred;
        $crate::tools::batch::batch($items, $crate::types::get_concurrency(), move |(url, html): (String, $crate::types::Html)| {
            let pred = pred.clone();
            async move { if pred(html.clone()).await { Some((url, html)) } else { None } }
        })
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<(String, $crate::types::Html)>>()
    }};
}
