use crate::impls::{DefaultScraper, ReqwestFetcher};
use crate::{engine::*, error::*, store::*, types::*};
use serde::Serialize;
use std::collections::BTreeMap;
use url::Url;

/* ------------ public facade components ------------ */

pub struct Components {
    pub fetcher: Box<dyn Fetcher>,
    pub scraper: Box<dyn Scraper>,
    pub opts: EngineOptions,
}
impl Default for Components {
    fn default() -> Self {
        let fetcher = ReqwestFetcher::new().expect("failed to init reqwest client");
        let scraper = DefaultScraper;
        Self {
            fetcher: Box::new(fetcher),
            scraper: Box::new(scraper),
            opts: EngineOptions::default(),
        }
    }
}

pub fn make_engine<'a, PS: PolicyStore>(
    store: &'a PS,
    components: &'a Components,
) -> Engine<'a, PS> {
    Engine::new(
        store,
        &*components.fetcher,
        &*components.scraper,
        components.opts,
    )
}

/* ------------ shared runtime verification ------------ */

fn verify_policy_runtime(components: &Components, pol: &Policy) -> Result<()> {
    // Try https://<domain>/, then http://
    let https = format!("https://{}/", pol.domain.0);
    let attempt = components
        .fetcher
        .fetch_blocking(&https, &pol.crawl)
        .and_then(|html| components.scraper.scrape(&https, &html, &pol.scrape))
        .or_else(|_| {
            let http = format!("http://{}/", pol.domain.0);
            components
                .fetcher
                .fetch_blocking(&http, &pol.crawl)
                .and_then(|html| components.scraper.scrape(&http, &html, &pol.scrape))
        });

    let page = attempt.map_err(|e| QrawlError::Other(format!("fetch/scrape failed: {e}")))?;
    let has_any_content = !page.json_ld.is_empty()
        || page.areas.iter().any(|a| {
            a.title
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
                || !a.headings.is_empty()
                || !a.paragraphs.is_empty()
                || !a.images.is_empty()
                || !a.links.is_empty()
        });

    if !has_any_content {
        return Err(QrawlError::Other(
            "scrape succeeded but produced no content — adjust selectors".into(),
        ));
    }
    Ok(())
}

/* ------------ policy helpers ------------ */

/// Automatically probe/infer a working policy, verify live, then save it.
/// Refuses to overwrite an existing policy.
pub fn policy_create_auto<PS: PolicyStore>(
    store: &PS,
    domain: Domain,
    components: &Components,
) -> Result<Policy> {
    if store.get(&domain)?.is_some() {
        return Err(QrawlError::Other(format!(
            "policy already exists for domain {}",
            domain.0
        )));
    }
    // Infer
    let pol = crate::infer::infer_policy(&*components.fetcher, &*components.scraper, &domain)?;
    // Double-check (redundant but safe)
    verify_policy_runtime(components, &pol)?;
    // Persist
    store.set(&pol)?;
    Ok(pol)
}

/// Update (or create-if-missing) ONLY if supplied config works (manual path).
pub fn policy_update_checked<PS: PolicyStore>(
    store: &PS,
    policy: &Policy,
    components: &Components,
) -> Result<()> {
    crate::policy::validate_policy(policy)?;
    verify_policy_runtime(components, policy)?;
    store.set(policy)
}

pub fn policy_read<PS: PolicyStore>(store: &PS, target: &str) -> Result<Option<Policy>> {
    if target == "all" {
        return Err(QrawlError::Other("use policy_list for 'all'".into()));
    }
    store.get(&Domain::from_raw(target))
}

pub fn policy_list<PS: PolicyStore>(store: &PS) -> Result<Vec<Policy>> {
    store.list()
}

pub fn policy_delete<PS: PolicyStore>(store: &PS, target: &str) -> Result<()> {
    if target == "all" {
        return store.delete_all();
    }
    store.delete(&Domain::from_raw(target))
}

/* ------------ extraction entrypoints ------------ */

pub fn extract_url<PS: PolicyStore>(
    store: &PS,
    url: &str,
    unknown: bool,
    components: &Components,
) -> Result<ExtractionBundle> {
    let engine = make_engine(store, components);
    if unknown {
        engine.extract_unknown(url)
    } else {
        engine.extract_known(url)
    }
}

pub fn extract_url_auto<PS: PolicyStore>(
    store: &PS,
    url: &str,
    components: &Components,
) -> Result<ExtractionBundle> {
    let engine = make_engine(store, components);

    let domain = {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?
    };

    if store.get(&domain)?.is_some() {
        engine.extract_known(url)
    } else {
        engine.extract_unknown(url)
    }
}

/* ------------ policy status (audit) ------------ */

#[derive(Serialize)]
pub struct PolicyStatus {
    pub status: String, // "pass" | "fail"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PolicyConfig>, // when verbose
}

pub fn policy_status_all<PS: PolicyStore>(
    store: &PS,
    components: &Components,
    verbose: bool,
) -> Result<BTreeMap<String, PolicyStatus>> {
    let mut out: BTreeMap<String, PolicyStatus> = BTreeMap::new();
    for pol in store.list()? {
        let status = match verify_policy_runtime(components, &pol) {
            Ok(_) => PolicyStatus {
                status: "pass".into(),
                error: None,
                config: if verbose {
                    Some(PolicyConfig {
                        crawl: pol.crawl.clone(),
                        scrape: pol.scrape.clone(),
                    })
                } else {
                    None
                },
            },
            Err(e) => PolicyStatus {
                status: "fail".into(),
                error: Some(e.to_string()),
                config: if verbose {
                    Some(PolicyConfig {
                        crawl: pol.crawl.clone(),
                        scrape: pol.scrape.clone(),
                    })
                } else {
                    None
                },
            },
        };
        out.insert(pol.domain.0.clone(), status);
    }
    Ok(out)
}
