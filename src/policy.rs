use crate::{types::*, error::*};
use chrono::Utc;

pub const POLICY_VERSION: u32 = 1;

pub fn default_crawl_config() -> CrawlConfig {
    CrawlConfig {
        user_agents: vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36".into(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15".into(),
        ],
        default_headers: HeaderSet::empty()
            .with("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .with("Accept-Language", "en-US,en;q=0.9")
            .with("Accept-Encoding", "gzip, deflate, br")
            .with("Connection", "keep-alive"),
        respect_robots_txt: true,
        timeout_ms: 20_000,
    }
}

pub fn default_main_area() -> AreaPolicy {
    AreaPolicy {
        roots: vec![Sel("article".into()), Sel("main".into()), Sel(".post".into())],
        exclude_within: vec![Sel("nav".into()), Sel("footer".into()), Sel(".ads".into()), Sel("#cookie".into())],
        role: AreaRole::Main,
        fields: FieldSelectors {
            title: vec![Sel("h1".into()), Sel("header h1".into()), Sel(".post-title".into())],
            headings: vec![Sel("h2".into()), Sel("h3".into())],
            paragraphs: vec![Sel("p".into())],
            images: vec![Sel("img".into())],
            links: vec![Sel("a[href]".into())],
            lists: vec![],
            tables: vec![],
        },
        is_repeating: false,
        follow_links: FollowLinks::default(),
    }
}

pub fn default_scrape_config() -> ScrapeConfig {
    ScrapeConfig { extract_json_ld: true, areas: vec![default_main_area()] }
}

pub fn new_policy(domain: Domain) -> Policy {
    let now = Utc::now();
    Policy {
        domain,
        crawl: default_crawl_config(),
        scrape: default_scrape_config(),
        version: POLICY_VERSION,
        created_at: now,
        updated_at: now,
    }
}

pub fn touch_updated(p: &mut Policy) { p.updated_at = Utc::now(); }

pub fn validate_policy(p: &Policy) -> Result<()> {
    if p.domain.0.is_empty() { return Err(QrawlError::Other("domain cannot be empty".into())); }
    Ok(())
}
