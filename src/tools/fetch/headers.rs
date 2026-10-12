use super::types::FetchProfile;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};

/// Build complete header map for the given profile, including User-Agent.
pub(crate) fn headers_for_profile(profile: FetchProfile) -> HeaderMap {
    let mut headers = HeaderMap::new();

    for (name, value) in header_pairs_for_profile(profile) {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }

    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(user_agent_for_profile(profile)),
    );

    headers
}

/// User-Agent string for the given profile.
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
        FetchProfile::Ios => {
            // Safari on iPhone (most popular mobile browser)
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
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
                ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"),
                ("accept-language", "en-US,en;q=0.9"),
                ("accept-encoding", "gzip, deflate, br"),
                ("connection", "keep-alive"),
                ("upgrade-insecure-requests", "1"),
                ("sec-fetch-dest", "document"),
                ("sec-fetch-mode", "navigate"),
                ("sec-fetch-site", "none"),
                ("sec-ch-ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""),
                ("sec-ch-ua-mobile", "?0"),
                ("sec-ch-ua-platform", "\"Windows\""),
            ]
        }
        FetchProfile::Ios => {
            // Safari on iPhone - mobile Safari headers
            vec![
                (
                    "accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                ),
                ("accept-language", "en-US,en;q=0.9"),
                ("accept-encoding", "gzip, deflate, br"),
                ("connection", "keep-alive"),
            ]
        }
    }
}
