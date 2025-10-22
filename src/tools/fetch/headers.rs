use super::profile::FetchProfile;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};

/// Build complete header map for the given profile, including User-Agent.
pub(crate) fn headers_for_profile(profile: FetchProfile) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Add profile-specific headers
    for (k, v) in header_pairs_for_profile(profile) {
        let name = HeaderName::from_lowercase(k.to_ascii_lowercase().as_bytes())
            .unwrap_or_else(|_| HeaderName::from_static("accept"));
        if let Ok(val) = HeaderValue::from_str(v) {
            headers.insert(name, val);
        }
    }

    // Add User-Agent
    let ua = user_agent_for_profile(profile);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(ua).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
    );

    headers
}

/// Get User-Agent string for the given profile (private, only used internally).
fn user_agent_for_profile(profile: FetchProfile) -> &'static str {
    match profile {
        FetchProfile::Minimal => {
            // Minimal UA - simple but identifies as browser
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
        }
        FetchProfile::Windows => {
            // Chrome on Windows 10 (most popular desktop)
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        }
        FetchProfile::MacOS => {
            // Safari on macOS Sonoma (native browser)
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15"
        }
        FetchProfile::IOS => {
            // Safari on iPhone (most popular mobile browser)
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
        }
        FetchProfile::Android => {
            // Chrome on Android 14
            "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.6778.200 Mobile Safari/537.36"
        }
    }
}

/// Get header pairs for the given profile (without User-Agent).
fn header_pairs_for_profile(profile: FetchProfile) -> Vec<(&'static str, &'static str)> {
    match profile {
        FetchProfile::Minimal => {
            // Truly minimal - no headers at all (just User-Agent)
            vec![]
        }
        FetchProfile::Windows => {
            // Chrome on Windows - full desktop headers
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
                ("Sec-Ch-Ua-Platform", "\"Windows\""),
            ]
        }
        FetchProfile::MacOS => {
            // Safari on macOS - native browser headers
            vec![
                (
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                ),
                ("Accept-Language", "en-US,en;q=0.9"),
                ("Accept-Encoding", "gzip, deflate, br"),
                ("Connection", "keep-alive"),
            ]
        }
        FetchProfile::IOS => {
            // Safari on iPhone - mobile Safari headers
            vec![
                (
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                ),
                ("Accept-Language", "en-US,en;q=0.9"),
                ("Accept-Encoding", "gzip, deflate, br"),
                ("Connection", "keep-alive"),
            ]
        }
        FetchProfile::Android => {
            // Chrome on Android - mobile Chrome headers
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
                ("Sec-Ch-Ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""),
                ("Sec-Ch-Ua-Mobile", "?1"),
                ("Sec-Ch-Ua-Platform", "\"Android\""),
            ]
        }
    }
}
