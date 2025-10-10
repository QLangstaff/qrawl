use super::types::FetchStrategy;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};

/// Build complete header map for the given strategy, including User-Agent.
pub(crate) fn headers_for_strategy(strategy: FetchStrategy) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Add strategy-specific headers
    for (k, v) in header_pairs_for_strategy(strategy) {
        let name = HeaderName::from_lowercase(k.to_ascii_lowercase().as_bytes())
            .unwrap_or_else(|_| HeaderName::from_static("accept"));
        if let Ok(val) = HeaderValue::from_str(v) {
            headers.insert(name, val);
        }
    }

    // Add User-Agent
    let ua = user_agent_for_strategy(strategy);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(ua).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
    );

    headers
}

/// Get User-Agent string for the given strategy (private, only used internally).
fn user_agent_for_strategy(strategy: FetchStrategy) -> &'static str {
    match strategy {
        FetchStrategy::Minimal => {
            // Minimal UA - simple but identifies as browser
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
        }
        FetchStrategy::Browser | FetchStrategy::Adaptive => {
            // Standard desktop browser - Chrome on macOS
            // Adaptive uses Browser headers as default
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        }
        FetchStrategy::Mobile => {
            // Mobile browser - Android Chrome
            "Mozilla/5.0 (Linux; Android 6.0; Nexus 5 Build/MRA58N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.7258.127 Mobile Safari/537.36"
        }
        FetchStrategy::Stealth | FetchStrategy::Extreme => {
            // Same as Mobile but with more aggressive headers
            // Extreme uses Stealth headers (difference is in orchestration, not headers)
            "Mozilla/5.0 (Linux; Android 6.0; Nexus 5 Build/MRA58N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.7258.127 Mobile Safari/537.36"
        }
    }
}

/// Get header pairs for the given strategy (without User-Agent).
fn header_pairs_for_strategy(strategy: FetchStrategy) -> Vec<(&'static str, &'static str)> {
    match strategy {
        FetchStrategy::Minimal => {
            // Bare minimum - just like curl
            vec![("Accept", "*/*"), ("Accept-Encoding", "gzip, deflate")]
        }
        FetchStrategy::Browser | FetchStrategy::Adaptive => {
            // Standard desktop browser headers - Chrome on macOS
            // Adaptive uses Browser headers as default
            vec![
                ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"),
                ("Accept-Language", "en-US,en;q=0.9"),
                ("Accept-Encoding", "gzip, deflate, br, zstd"),
                ("Connection", "keep-alive"),
                ("Upgrade-Insecure-Requests", "1"),
                ("Sec-Fetch-Dest", "document"),
                ("Sec-Fetch-Mode", "navigate"),
                ("Sec-Fetch-Site", "none"),
                ("Sec-Ch-Ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""),
                ("Sec-Ch-Ua-Mobile", "?0"),
                ("Sec-Ch-Ua-Platform", "\"macOS\""),
            ]
        }
        FetchStrategy::Mobile => {
            // Mobile browser headers - Android Chrome
            vec![
                ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"),
                ("Accept-Language", "en-US,en;q=0.9"),
                ("Accept-Encoding", "gzip, deflate, br, zstd"),
                ("Connection", "keep-alive"),
                ("Upgrade-Insecure-Requests", "1"),
                ("Sec-Fetch-Dest", "document"),
                ("Sec-Fetch-Mode", "navigate"),
                ("Sec-Fetch-Site", "none"),
                ("Sec-Fetch-User", "?1"),
                ("Sec-Ch-Ua", "\"Not;A=Brand\";v=\"99\", \"Google Chrome\";v=\"139\", \"Chromium\";v=\"139\""),
                ("Sec-Ch-Ua-Mobile", "?1"),
                ("Sec-Ch-Ua-Platform", "\"Android\""),
            ]
        }
        FetchStrategy::Stealth | FetchStrategy::Extreme => {
            // Aggressive anti-bot evasion - full sec-ch-ua suite
            // Extreme uses same headers as Stealth (difference is in orchestration)
            vec![
                ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"),
                ("Accept-Language", "en-US,en;q=0.9"),
                ("Accept-Encoding", "gzip, deflate, br, zstd"),
                ("Cache-Control", "no-cache, no-store, must-revalidate"),
                ("Pragma", "no-cache"),
                ("Connection", "keep-alive"),
                ("Upgrade-Insecure-Requests", "1"),
                ("Sec-Fetch-Dest", "document"),
                ("Sec-Fetch-Mode", "navigate"),
                ("Sec-Fetch-Site", "none"),
                ("Sec-Fetch-User", "?1"),
                ("Sec-Ch-Ua", "\"Not;A=Brand\";v=\"99\", \"Google Chrome\";v=\"139\", \"Chromium\";v=\"139\""),
                ("Sec-Ch-Ua-Mobile", "?1"),
                ("Sec-Ch-Ua-Platform", "\"Android\""),
                ("Sec-Ch-Ua-Platform-Version", "\"6.0\""),
                ("Sec-Ch-Ua-Full-Version", "\"139.0.7258.127\""),
                ("Sec-Ch-Ua-Full-Version-List", "\"Not;A=Brand\";v=\"99.0.0.0\", \"Google Chrome\";v=\"139.0.7258.127\", \"Chromium\";v=\"139.0.7258.127\""),
                ("Sec-Ch-Ua-Model", "\"Nexus 5\""),
                ("Sec-Ch-Ua-Arch", "\"\""),
                ("Sec-Ch-Ua-Bitness", "\"64\""),
                ("Sec-Ch-Ua-Form-Factors", "\"Desktop\""),
                ("Sec-Ch-Ua-Wow64", "?0"),
                ("Sec-Ch-Prefers-Color-Scheme", "dark"),
                ("Dnt", "1"),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_has_basic_headers() {
        let headers = headers_for_strategy(FetchStrategy::Minimal);
        assert!(headers.contains_key("accept"));
        assert!(headers.contains_key("user-agent"));
        // Should have minimal headers
        assert!(headers.len() <= 3);
    }

    #[test]
    fn browser_has_sec_ch_headers() {
        let headers = headers_for_strategy(FetchStrategy::Browser);
        assert!(headers.contains_key("sec-ch-ua"));
        assert!(headers.contains_key("sec-ch-ua-platform"));
        assert_eq!(
            headers
                .get("sec-ch-ua-mobile")
                .and_then(|v| v.to_str().ok()),
            Some("?0")
        );
    }

    #[test]
    fn stealth_has_full_suite() {
        let headers = headers_for_strategy(FetchStrategy::Stealth);
        // Should have many sec-ch-ua headers
        assert!(headers.contains_key("sec-ch-ua-full-version"));
        assert!(headers.contains_key("sec-ch-ua-model"));
        assert!(headers.contains_key("sec-ch-ua-bitness"));
        assert!(headers.contains_key("cache-control"));
    }
}
