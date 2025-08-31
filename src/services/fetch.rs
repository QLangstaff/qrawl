use crate::services::infer::is_valid_response;
use crate::{engine::Fetcher as FetcherT, types::*};
use async_trait::async_trait;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL,
    CONNECTION, REFERER, UPGRADE_INSECURE_REQUESTS, USER_AGENT,
};
use reqwest::Client as AsyncClient;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct ReqwestFetcher;

impl ReqwestFetcher {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn build_client_for_policy(&self, cfg: &FetchConfig) -> Result<Client> {
        if matches!(cfg.bot_evasion_strategy, BotEvadeStrategy::UltraMinimal) {
            return Ok(Client::builder().timeout(Duration::from_secs(30)).build()?);
        }

        let mut builder = Client::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(10));

        match cfg.http_version {
            HttpVersion::Http1Only => {
                builder = builder.http1_only();
            }
            HttpVersion::Http2Only => {
                builder = builder.http2_prior_knowledge();
            }
            HttpVersion::Http2WithHttp1Fallback => {
                // Default reqwest behavior - try HTTP/2, fallback to HTTP/1.1
            }
        }

        Ok(builder.build()?)
    }

    fn build_async_client_for_policy(&self, cfg: &FetchConfig) -> Result<AsyncClient> {
        if matches!(cfg.bot_evasion_strategy, BotEvadeStrategy::UltraMinimal) {
            return Ok(AsyncClient::builder()
                .timeout(Duration::from_secs(30))
                .build()?);
        }

        let mut builder = AsyncClient::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(10));

        match cfg.http_version {
            HttpVersion::Http1Only => {
                builder = builder.http1_only();
            }
            HttpVersion::Http2Only => {
                builder = builder.http2_prior_knowledge();
            }
            HttpVersion::Http2WithHttp1Fallback => {
                // Default reqwest behavior - try HTTP/2, fallback to HTTP/1.1
            }
        }

        Ok(builder.build()?)
    }

    fn try_once(
        &self,
        client: &Client,
        url: &str,
        mut headers: HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) -> Result<String> {
        self.apply_evasion_strategy(&mut headers, ua, referer, strategy);

        let resp = client.get(url).headers(headers).send()?;
        let status = resp.status();
        let text = resp.text()?;

        if is_valid_response(Some(status), &text) {
            return Ok(text);
        }
        Err(QrawlError::fetch_error(
            url,
            &format!("HTTP status {}", status),
        ))
    }

    fn apply_evasion_strategy(
        &self,
        headers: &mut HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) {
        match strategy {
            BotEvadeStrategy::UltraMinimal => {
                // Ultra minimal: ONLY User-Agent header
            }
            BotEvadeStrategy::Minimal => {
                headers
                    .entry(ACCEPT)
                    .or_insert(HeaderValue::from_static("text/html;q=0.9,*/*;q=0.8"));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.8"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br"));
            }
            BotEvadeStrategy::Standard => {
                headers.entry(ACCEPT).or_insert(HeaderValue::from_static(
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
                ));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.5"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br"));
                headers
                    .entry(CONNECTION)
                    .or_insert(HeaderValue::from_static("keep-alive"));
                headers.insert(
                    HeaderName::from_static("upgrade-insecure-requests"),
                    HeaderValue::from_static("1"),
                );
                headers.insert(
                    HeaderName::from_static("dnt"),
                    HeaderValue::from_static("1"),
                );
            }
            BotEvadeStrategy::Advanced => {
                headers.entry(ACCEPT).or_insert(HeaderValue::from_static(
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"
                ));
                headers
                    .entry(ACCEPT_LANGUAGE)
                    .or_insert(HeaderValue::from_static("en-US,en;q=0.9"));
                headers
                    .entry(ACCEPT_ENCODING)
                    .or_insert(HeaderValue::from_static("gzip, deflate, br, zstd"));
                headers
                    .entry(CONNECTION)
                    .or_insert(HeaderValue::from_static("keep-alive"));
                headers.insert(
                    HeaderName::from_static("upgrade-insecure-requests"),
                    HeaderValue::from_static("1"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-dest"),
                    HeaderValue::from_static("document"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-mode"),
                    HeaderValue::from_static("navigate"),
                );
                headers.insert(
                    HeaderName::from_static("sec-fetch-site"),
                    HeaderValue::from_static("none"),
                );
            }
            BotEvadeStrategy::Adaptive => {
                headers.insert(
                    ACCEPT,
                    HeaderValue::from_static(
                        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    ),
                );
                headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
                headers.insert(
                    ACCEPT_ENCODING,
                    HeaderValue::from_static("gzip, deflate, br"),
                );
                headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
                headers.insert(UPGRADE_INSECURE_REQUESTS, HeaderValue::from_static("1"));
                headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=0"));
                if let Some(ref_url) = referer {
                    if let Ok(ref_value) = HeaderValue::from_str(ref_url) {
                        headers.insert(REFERER, ref_value);
                    }
                }
            }
        }

        // Add User-Agent - use different UA for UltraMinimal
        let user_agent = match strategy {
            BotEvadeStrategy::UltraMinimal => "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
            _ => ua,
        };
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(user_agent).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
        );
        if let Some(r) = referer {
            if let Ok(referer_value) = HeaderValue::from_str(r) {
                headers.insert(REFERER, referer_value);
            }
            // Skip invalid referer values silently - this is evasion strategy, not critical
        }
    }

    async fn try_once_async(
        &self,
        client: &AsyncClient,
        url: &str,
        mut headers: HeaderMap,
        ua: &str,
        referer: Option<&str>,
        strategy: &BotEvadeStrategy,
    ) -> Result<String> {
        self.apply_evasion_strategy(&mut headers, ua, referer, strategy);

        let resp = client.get(url).headers(headers).send().await?;
        let status = resp.status();
        let text = resp.text().await?;

        if is_valid_response(Some(status), &text) {
            return Ok(text);
        }
        Err(QrawlError::fetch_error(
            url,
            &format!("HTTP status {}", status),
        ))
    }
}

#[async_trait]
impl FetcherT for ReqwestFetcher {
    fn name(&self) -> &'static str {
        "reqwest-blocking"
    }

    fn fetch_blocking(&self, url: &str, cfg: &FetchConfig) -> Result<String> {
        let (parsed, _domain) = Domain::parse_from_url(url)?;
        let origin = format!("{}://{}/", parsed.scheme(), parsed.host_str().unwrap_or(""));

        let client = self.build_client_for_policy(cfg)?;

        let uas: Vec<&str> = if cfg.user_agents.is_empty() {
            vec!["Mozilla/5.0"]
        } else {
            cfg.user_agents.iter().map(|s| s.as_str()).collect()
        };

        let base = to_headermap(&cfg.default_headers, None)?;

        let strategies = match &cfg.bot_evasion_strategy {
            BotEvadeStrategy::Adaptive => {
                vec![
                    BotEvadeStrategy::UltraMinimal,
                    BotEvadeStrategy::Minimal,
                    BotEvadeStrategy::Standard,
                    BotEvadeStrategy::Advanced,
                ]
            }
            other => vec![other.clone()],
        };

        for (strategy_idx, strategy) in strategies.iter().enumerate() {
            for (ua_idx, ua) in uas.iter().enumerate() {
                if let Ok(text) = self.try_once(&client, url, base.clone(), ua, None, strategy) {
                    return Ok(text);
                }

                if strategy_idx == 0 && ua_idx == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(80 + jitter_ms(120)));
                }

                match self.try_once(&client, url, base.clone(), ua, Some(&origin), strategy) {
                    Ok(text) => return Ok(text),
                    Err(e) => {
                        if strategy_idx == strategies.len() - 1 && ua_idx == uas.len() - 1 {
                            return Err(e);
                        }
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(120 + jitter_ms(160)));
            }

            if strategy_idx < strategies.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(300 + jitter_ms(200)));
            }
        }

        Err(QrawlError::fetch_error(
            url,
            "request failed after all evasion strategies",
        ))
    }

    async fn fetch_async(&self, url: &str, cfg: &FetchConfig) -> Result<String> {
        let (parsed, _domain) = Domain::parse_from_url(url)?;
        let origin = format!("{}://{}/", parsed.scheme(), parsed.host_str().unwrap_or(""));

        let client = self.build_async_client_for_policy(cfg)?;

        let uas: Vec<&str> = if cfg.user_agents.is_empty() {
            vec!["Mozilla/5.0"]
        } else {
            cfg.user_agents.iter().map(|s| s.as_str()).collect()
        };

        let base = to_headermap(&cfg.default_headers, None)?;

        let strategies = match &cfg.bot_evasion_strategy {
            BotEvadeStrategy::Adaptive => {
                vec![
                    BotEvadeStrategy::UltraMinimal,
                    BotEvadeStrategy::Minimal,
                    BotEvadeStrategy::Standard,
                    BotEvadeStrategy::Advanced,
                ]
            }
            other => vec![other.clone()],
        };

        for (strategy_idx, strategy) in strategies.iter().enumerate() {
            for (ua_idx, ua) in uas.iter().enumerate() {
                if let Ok(text) = self
                    .try_once_async(&client, url, base.clone(), ua, None, strategy)
                    .await
                {
                    return Ok(text);
                }

                if strategy_idx == 0 && ua_idx == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(80 + jitter_ms(120)))
                        .await;
                }

                match self
                    .try_once_async(&client, url, base.clone(), ua, Some(&origin), strategy)
                    .await
                {
                    Ok(text) => return Ok(text),
                    Err(e) => {
                        if strategy_idx == strategies.len() - 1 && ua_idx == uas.len() - 1 {
                            return Err(e);
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(120 + jitter_ms(160))).await;
            }

            if strategy_idx < strategies.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(300 + jitter_ms(200))).await;
            }
        }

        Err(QrawlError::fetch_error(
            url,
            "request failed after all evasion strategies",
        ))
    }
}

fn to_headermap(hs: &HeaderSet, ua: Option<&str>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (k, v) in &hs.0 {
        let kn = HeaderName::from_bytes(k.as_bytes()).map_err(|e| {
            QrawlError::validation_error(
                &format!("header_name_{}", k),
                &format!("invalid header name: {}", e),
            )
        })?;
        let vv = HeaderValue::from_str(v).map_err(|e| {
            QrawlError::validation_error(
                &format!("header_value_{}", k),
                &format!("invalid header value: {}", e),
            )
        })?;
        headers.insert(kn, vv);
    }
    if let Some(ua_str) = ua {
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(ua_str).unwrap_or(HeaderValue::from_static("Mozilla/5.0")),
        );
    }
    Ok(headers)
}

fn jitter_ms(range: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_nanos(0));
    let nanos = now.subsec_nanos() as u64;
    let micros = (now.as_micros() & 0xFFFF) as u64;
    (nanos ^ (micros << 5)) % range
}
