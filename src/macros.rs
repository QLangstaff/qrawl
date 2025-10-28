//! Shared Macros

/// Chain tools
#[macro_export]
macro_rules! chain {
    // Base case: no more stages, return items
    (@process $items:expr, $ctx:expr $(,)?) => { $items };

    // Helper: List dedupe functions (&[String] -> Vec<String>, operates on whole list)
    (@process_list_dedupe $items:expr, $ctx:expr, $fn:expr $(, $rest:ident)*) => {{
        let data: Vec<String> = $items.iter().map(|(_, d)| d.clone()).collect();
        let cleaned = $fn(&data).await;
        let items: Vec<(String, String)> = cleaned.into_iter().map(|d| (d.clone(), d)).collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Helper: Per-URL list clean (Vec<(url, Vec<String>)> -> Vec<(url, Vec<String>)>)
    (@process_per_url_list $items:expr, $ctx:expr, $fn:expr $(, $rest:ident)*) => {{
        let mut cleaned_items = Vec::new();
        for (url, list) in $items {
            let cleaned = $fn(&list).await;
            cleaned_items.push((url, cleaned));
        }
        let items: Vec<(String, Vec<String>)> = cleaned_items;
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Helper: Flatten and clean globally (Vec<(url, Vec<String>)> -> Vec<(url, String)>)
    // Takes extracted lists (e.g., emails, phones), flattens all into one list,
    // applies clean function globally, returns deduplicated flat tuples
    (@process_flatten_and_clean $items:expr, $ctx:expr, $fn:expr $(, $rest:ident)*) => {{
        let data: Vec<String> = $items.into_iter()
            .flat_map(|(_, list): (String, Vec<String>)| list)
            .collect();
        let cleaned = $fn(&data).await;
        let items: Vec<(String, String)> = cleaned.into_iter().map(|d| (d.clone(), d)).collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Helper: Extract functions (Vec<(url, String)> -> Vec<(url, Vec<String>)>)
    (@process_extract $items:expr, $ctx:expr, $fn:expr $(, $rest:ident)*) => {{
        let concurrency = $ctx.concurrency;
        let items: Vec<(String, Vec<String>)> = $crate::tools::batch::batch(
            $items,
            concurrency,
            |(url, data): (String, String)| async move {
                let result = $fn(&data).await;
                Some((url, result))
            }
        ).await
        .into_iter()
        .flatten()
        .collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Dispatch: clean_urls
    (@process $items:expr, $ctx:expr, clean_urls $(, $rest:ident)*) => {{
        $crate::chain!(@process_list_dedupe $items, $ctx, $crate::tools::clean::clean_urls $(, $rest)*)
    }};

    // Dispatch: clean_emails (flattens and deduplicates globally)
    (@process $items:expr, $ctx:expr, clean_emails $(, $rest:ident)*) => {{
        $crate::chain!(@process_flatten_and_clean $items, $ctx, $crate::tools::clean::clean_emails $(, $rest)*)
    }};

    // Dispatch: clean_phones (flattens and deduplicates globally)
    (@process $items:expr, $ctx:expr, clean_phones $(, $rest:ident)*) => {{
        $crate::chain!(@process_flatten_and_clean $items, $ctx, $crate::tools::clean::clean_phones $(, $rest)*)
    }};

    // Dispatch: extract_emails
    (@process $items:expr, $ctx:expr, extract_emails $(, $rest:ident)*) => {{
        $crate::chain!(@process_extract $items, $ctx, $crate::tools::extract::extract_emails $(, $rest)*)
    }};

    // Dispatch: extract_phones
    (@process $items:expr, $ctx:expr, extract_phones $(, $rest:ident)*) => {{
        $crate::chain!(@process_extract $items, $ctx, $crate::tools::extract::extract_phones $(, $rest)*)
    }};

    // map_children: batched per-item, needs URL from tuple, flattens Vec<String> results
    (@process $items:expr, $ctx:expr, map_children $(, $rest:ident)*) => {{
        let concurrency = $ctx.concurrency;
        let items: Vec<(String, String)> = $crate::tools::batch::batch(
            $items,
            concurrency,
            |(url, html): (String, String)| async move {
                let children = $crate::tools::map::map_children(&html, &url).await;
                children.into_iter()
                    .map(|child| (child.clone(), child))
                    .collect::<Vec<(String, String)>>()
            }
        ).await
        .into_iter()
        .flatten()
        .collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // map_page: batched per-item, needs URL from tuple, flattens Vec<String> results
    (@process $items:expr, $ctx:expr, map_page $(, $rest:ident)*) => {{
        let concurrency = $ctx.concurrency;
        let items: Vec<(String, String)> = $crate::tools::batch::batch(
            $items,
            concurrency,
            |(url, html): (String, String)| async move {
                let links = $crate::tools::map::map_page(&html, &url).await;
                links.into_iter()
                    .map(|link| (link.clone(), link))
                    .collect::<Vec<(String, String)>>()
            }
        ).await
        .into_iter()
        .flatten()
        .collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // clean_html: per-item batched, returns String (infallible)
    (@process $items:expr, $ctx:expr, clean_html $(, $rest:ident)*) => {{
        let concurrency = $ctx.concurrency;
        let items: Vec<(String, String)> = $crate::tools::batch::batch(
            $items,
            concurrency,
            |(url, data): (String, String)| async move {
                let result = $crate::tools::clean::clean_html(&data).await;
                Some((url, result))
            }
        ).await
        .into_iter()
        .flatten()
        .collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Default: per-item batched function returning Result (fetch_*, etc.)
    (@process $items:expr, $ctx:expr, $fn:ident $(, $rest:ident)*) => {{
        let concurrency = $ctx.concurrency;
        let items: Vec<(String, String)> = $crate::tools::batch::batch(
            $items,
            concurrency,
            |(url, data): (String, String)| async move {
                $fn(&data).await.ok().map(|result| (url, result))
            }
        ).await
        .into_iter()
        .flatten()
        .collect();
        $crate::chain!(@process items, $ctx $(, $rest)*)
    }};

    // Entry point: initialize tuples and start processing
    ($urls:expr, $ctx:expr => $first:ident $(-> $rest:ident)*) => {{
        async move {
            use std::sync::Arc;
            let ctx = Arc::new($ctx);
            let items: Vec<(String, String)> = $urls.into_iter().map(|u| (u.clone(), u)).collect();

            $crate::types::CTX.scope(ctx.clone(), async move {
                $crate::chain!(@process items, ctx, $first $(, $rest)*)
            }).await
        }
    }};
}

/// Merge multiple vectors into one.
#[macro_export]
macro_rules! merge {
    ($($vec:expr),+ $(,)?) => {{
        let mut result = Vec::new();
        $(result.extend($vec);)+
        result
    }};
}

/// Run any processor function (handles both sync and async).
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
    (@template $input:expr, $processor:expr $(,)?) => {{
        let url = $input;
        let result = $crate::runtime::block_on($processor(
            vec![url.to_string()],
            $crate::types::Context::default()
        ));
        $crate::cli::print_json(&result);
    }};
    // For String input with two-step async -> async processor chain
    (@async_chain $input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let data = $crate::cli::read_input(&$input);
        let result = $crate::runtime::block_on(async move {
            let intermediate = $first(&data).await;
            $second(&intermediate).await  // Both async
        });
        $crate::cli::print_json(&result);
    }};
    // For String input with two-step async -> sync processor chain
    (@async $input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let data = $crate::cli::read_input(&$input);
        let result = $crate::runtime::block_on(async move {
            let intermediate = $first(&data).await;
            $second(&intermediate)  // Second is sync
        });
        $crate::cli::print_json(&result);
    }};
    // For String input with async processor
    (@async $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let data = $crate::cli::read_input(&$input);
        let result = $crate::runtime::block_on($processor(&data $(, $arg)*));
        $crate::cli::print_json(&result);
    }};
    // For String input with two-step sync processor chain
    ($input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let data = $crate::cli::read_input(&$input);
        let intermediate = $first(&data);
        let result = $second(&intermediate);
        $crate::cli::print_json(&result);
    }};
    // For String input with sync processor
    ($input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let data = $crate::cli::read_input(&$input);
        let result = $processor(&data $(, $arg)*);
        $crate::cli::print_json(&result);
    }};
}

/// Deduplicate a collection while preserving order.
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
