//! Shared Macros

/// Compose tools into one crawl. `chain! { urls, ctx => a -> b -> c }` threads a
/// collection through each step, fanning per-item work out with bounded
/// concurrency and feeding each step's output into the next. Each `a -> b` is
/// type-checked by the compiler (feeding a URL list where pages are expected
/// won't compile). Runs inside a fresh `CTX` + `FETCH_CACHE` scope; evaluates to
/// the final step's value — use it inside an `async fn`.
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
/// A "page" is a `(String, Html)` — its source URL and body. `map`/`extract`/
/// `filter` fan their tool out across the pages with bounded concurrency; a
/// capturing closure is cloned per page, so it must be `Clone + Send + Sync`.
///
/// ```ignore
/// use qrawl::tools::normalize::{normalize_emails, normalize_urls};
/// use qrawl::tools::extract::extract_emails;
/// use qrawl::tools::map::map_children;
///
/// let emails = chain! {
///     urls, ctx =>
///     all(normalize_urls) -> fetch -> map(map_children) ->
///     all(normalize_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
/// };
/// ```
#[macro_export]
macro_rules! chain {
    ($urls:expr, $ctx:expr => $($rest:tt)+) => {{
        let ctx_arc = ::std::sync::Arc::new($ctx);
        let cache = $crate::types::fetch_cache_new();
        $crate::types::CTX
            .scope(ctx_arc, async move {
                $crate::types::FETCH_CACHE
                    .scope(cache, async move { $crate::chain!(@run $urls, $($rest)+) })
                    .await
            })
            .await
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
            $crate::tools::fetch::fetch_strategy(&url).await.ok().map(|html| (url, html))
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

/// Merge Vectors
#[macro_export]
macro_rules! merge {
    ($($vec:expr),+ $(,)?) => {{
        let mut result = Vec::new();
        $(result.extend($vec);)+
        result
    }};
}

/// Run Processor
#[macro_export]
macro_rules! run {
    // For Vec<String> input with async processor
    (@vec_async $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let result = $crate::runtime::block_on($processor(&$input $(, $arg)*));
        $crate::cli::print_json(&result);
    }};
    // For Vec<String> input with sync processor
    (@vec $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let result = $processor(&$input $(, $arg)*);
        $crate::cli::print_json(&result);
    }};
    // For template functions that take Vec<String> and Context
    (@template $input:expr, $processor:expr, $ctx:expr $(,)?) => {{
        let url = $input;
        let result = $crate::runtime::block_on($processor(vec![url.to_string()], $ctx));
        $crate::cli::print_json(&result);
    }};
    (@template $input:expr, $processor:expr $(,)?) => {{
        $crate::run!(@template $input, $processor, $crate::types::Context::auto())
    }};
    // For String input with two-step async -> sync processor chain
    (@async $ctx:expr, $input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let data = $crate::cli::read_input(&$input, $ctx);
        let result = $crate::runtime::block_on(async move {
            let intermediate = $first(&$crate::types::Html::new(data)).await;
            $second(&intermediate)  // Second is sync
        });
        $crate::cli::print_json(&result);
    }};
    // For String input with async processor
    (@async $ctx:expr, $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let data = $crate::cli::read_input(&$input, $ctx);
        let result = $crate::runtime::block_on($processor(&$crate::types::Html::new(data) $(, $arg)*));
        $crate::cli::print_json(&result);
    }};
}

/// Deduplicate Items
#[macro_export]
macro_rules! dedupe {
    // Deduplication
    ($list:expr) => {{
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for item in $list {
            if seen.insert(item.clone()) {
                result.push(item);
            }
        }
        result
    }};
    // Deduplication + function application
    ($list:expr, $fn:expr) => {{
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for item in $list {
            let processed = $fn(item);
            if !processed.is_empty() && seen.insert(processed.clone()) {
                result.push(processed);
            }
        }
        result
    }};
}
