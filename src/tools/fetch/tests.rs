#[cfg(test)]
mod tests {
    use crate::tools::fetch::headers::headers_for_profile;
    use crate::tools::fetch::profile::FetchProfile;
    use crate::tools::fetch::utils::{jitter_ms, validate_response};
    use reqwest::StatusCode;

    fn padded_html(marker: &str) -> String {
        let filler = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
        let mut body = String::new();
        while body.len() < 600 {
            body.push_str(filler);
        }
        format!("<!DOCTYPE html><html><body>{marker} {}</body></html>", body)
    }

    #[test]
    fn minimal_has_only_user_agent() {
        let headers = headers_for_profile(FetchProfile::Minimal);
        assert!(headers.contains_key("user-agent"));
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn windows_has_chrome_headers() {
        let headers = headers_for_profile(FetchProfile::Windows);
        assert!(headers.contains_key("sec-ch-ua"));
        assert!(headers.contains_key("sec-ch-ua-platform"));
        assert_eq!(
            headers
                .get("sec-ch-ua-platform")
                .and_then(|v| v.to_str().ok()),
            Some("\"Windows\"")
        );
        assert_eq!(
            headers
                .get("sec-ch-ua-mobile")
                .and_then(|v| v.to_str().ok()),
            Some("?0")
        );
    }

    #[test]
    fn ios_has_safari_headers() {
        let headers = headers_for_profile(FetchProfile::IOS);
        assert!(headers.contains_key("accept"));
        assert!(!headers.contains_key("sec-ch-ua"));
    }

    #[test]
    fn android_has_mobile_chrome_headers() {
        let headers = headers_for_profile(FetchProfile::Android);
        assert!(headers.contains_key("sec-ch-ua-mobile"));
        assert_eq!(
            headers
                .get("sec-ch-ua-mobile")
                .and_then(|v| v.to_str().ok()),
            Some("?1")
        );
        assert_eq!(
            headers
                .get("sec-ch-ua-platform")
                .and_then(|v| v.to_str().ok()),
            Some("\"Android\"")
        );
    }

    #[test]
    fn jitter_returns_within_range() {
        for _ in 0..100 {
            let result = jitter_ms(100);
            assert!(result < 100, "jitter_ms returned {}", result);
        }
    }

    #[test]
    fn jitter_zero_range_returns_zero() {
        assert_eq!(jitter_ms(0), 0);
    }

    #[test]
    fn is_suspicious_cloudflare_challenge() {
        let html = padded_html("Checking your browser before accessing... cf-browser-verification");
        let err = validate_response(StatusCode::OK, &html).unwrap_err();
        assert!(err.contains("suspicious"));
        assert!(err.contains("cf-browser-verification"));
    }

    #[test]
    fn is_suspicious_cloudflare_captcha() {
        let html = padded_html("Please complete the captcha to continue. cf-captcha-container");
        let err = validate_response(StatusCode::OK, &html).unwrap_err();
        assert!(err.contains("suspicious"));
        assert!(err.contains("please complete the captcha"));
    }

    #[test]
    fn is_suspicious_perimeter_x() {
        let html = padded_html("PerimeterX robot detection blocking this request");
        let err = validate_response(StatusCode::OK, &html).unwrap_err();
        assert!(err.contains("suspicious"));
        assert!(err.contains("bot detection"));
    }

    #[test]
    fn is_suspicious_generic_captcha() {
        let html = padded_html("Please solve this captcha to verify you are a human");
        let err = validate_response(StatusCode::OK, &html).unwrap_err();
        assert!(err.contains("suspicious"));
        assert!(err.contains("verify you are a human"));
    }

    #[test]
    fn is_unauthorized_access_denied() {
        let html =
            padded_html("<h1>Access Denied</h1><p>Permission denied to access this resource</p>");
        let err = validate_response(StatusCode::OK, &html).unwrap_err();
        assert!(err.contains("unauthorized"));
        assert!(err.contains("access denied"));
    }

    #[test]
    fn validate_response_normal_content() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Welcome</h1><p>This is normal content with lots of text to meet the minimum length requirement. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.</p></body></html>"#;
        assert!(validate_response(StatusCode::OK, html).is_ok());
    }

    #[test]
    fn validate_response_non_success_status() {
        let html = r#"<!DOCTYPE html><html><body><h1>Page content</h1></body></html>"#;
        assert!(validate_response(StatusCode::NOT_FOUND, html).is_err());
        assert!(validate_response(StatusCode::INTERNAL_SERVER_ERROR, html).is_err());
        assert!(validate_response(StatusCode::FORBIDDEN, html).is_err());
    }

    #[test]
    fn detect_body_too_short() {
        let html = r#"<html><body>Short</body></html>"#;
        let err = validate_response(StatusCode::OK, html).unwrap_err();
        assert!(err.contains("body is too short"));
    }

    #[test]
    fn detect_non_html_content() {
        let json = r#"{"status": "ok", "data": "This is JSON not HTML but has enough length to pass the minimum length check so we need more text here to make it realistic. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore. And some additional text to ensure we exceed 500 bytes."}"#;
        let err = validate_response(StatusCode::OK, json).unwrap_err();
        assert!(err.contains("missing HTML markers"));
    }
}
