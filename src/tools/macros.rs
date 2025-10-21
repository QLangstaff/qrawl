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

/// Run any processor function.
#[macro_export]
macro_rules! run {
    // For Vec<String> input
    (@vec $input:expr, $processor:expr $(, $arg:expr)* $(,)?) => {{
        let result = $processor(&$input $(, $arg)*);
        $crate::tools::cli_utils::print_json(&result);
    }};
    // For String input
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
