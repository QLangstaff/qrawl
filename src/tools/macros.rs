//! Shared Macros

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
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");
        let result = runtime.block_on($processor(&$input $(, $arg)*));
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For Vec<String> input with sync processor
    (@vec $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let result = $processor(&$input $(, $arg)*);
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input with two-step async -> async processor chain
    (@async_chain $input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");
        let data = $crate::tools::cli_utils::read_input(&$input);
        let result = runtime.block_on(async {
            let intermediate = $first(&data).await;
            $second(&intermediate).await  // Both async
        });
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input with two-step async -> sync processor chain
    (@async $input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");
        let data = $crate::tools::cli_utils::read_input(&$input);
        let result = runtime.block_on(async {
            let intermediate = $first(&data).await;
            $second(&intermediate)  // Second is sync
        });
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input with async processor
    (@async $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");
        let data = $crate::tools::cli_utils::read_input(&$input);
        let result = runtime.block_on($processor(&data $(, $arg)*));
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input with two-step sync processor chain
    ($input:expr, [$first:expr, $second:expr] $(,)?) => {{
        let data = $crate::tools::cli_utils::read_input(&$input);
        let intermediate = $first(&data);
        let result = $second(&intermediate);
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input with sync processor
    ($input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let data = $crate::tools::cli_utils::read_input(&$input);
        let result = $processor(&data $(, $arg)*);
        $crate::tools::cli_utils::print_json(&result);
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
