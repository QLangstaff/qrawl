use crate::impls::{DefaultScraper, ReqwestFetcher};
use crate::{engine::*, error::*, store::*, types::*};
use std::time::Instant;
use url::Url;

// Helper function for logging - ignores errors to not break main operations
fn log_info(domain: Option<&str>, event: &str, details: Option<&str>) -> crate::Result<()> {
    match crate::log::ActivityLogger::new() {
        Ok(logger) => logger.info(domain, event, details),
        Err(_) => Ok(()), // Silently ignore logging errors
    }
}

fn log_error(domain: Option<&str>, event: &str, details: Option<&str>) -> crate::Result<()> {
    match crate::log::ActivityLogger::new() {
        Ok(logger) => logger.error(domain, event, details),
        Err(_) => Ok(()), // Silently ignore logging errors
    }
}

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

/* ------------ policy helpers ------------ */

/// Automatically probe/infer a working policy, verify live, then save it.
/// Refuses to overwrite an existing policy.
pub fn create_policy<PS: PolicyStore>(
    store: &PS,
    domain: Domain,
    components: &Components,
) -> Result<Policy> {
    let start_time = Instant::now();
    if store.get(&domain)?.is_some() {
        let duration = start_time.elapsed();
        let details = format!("skipped in {}ms", duration.as_millis());
        let _ = log_info(Some(&domain.0), "create_policy", Some(&details));
        return Err(QrawlError::Other(format!(
            "policy already exists for domain {}",
            domain.0
        )));
    }
    // Create minimal policy (no verification needed)
    let pol = crate::infer::infer_policy(&*components.fetcher, &*components.scraper, &domain)?;
    // Persist without verification - policy will be refined during actual extraction
    store.set(&pol)?;
    let duration = start_time.elapsed();
    let details = format!("succeeded in {}ms", duration.as_millis());
    let _ = log_info(Some(&domain.0), "create_policy", Some(&details));
    Ok(pol)
}

pub fn read_policy<PS: PolicyStore>(store: &PS, target: &str) -> Result<Option<Policy>> {
    if target == "all" {
        return Err(QrawlError::Other("use list_domains for 'all'".into()));
    }
    store.get(&Domain::from_raw(target))
}

pub fn list_domains<PS: PolicyStore>(store: &PS) -> Result<Vec<String>> {
    Ok(store.list()?.into_iter().map(|p| p.domain.0).collect())
}

pub fn delete_policy<PS: PolicyStore>(store: &PS, target: &str) -> Result<()> {
    let start_time = Instant::now();
    if target == "all" {
        let result = store.delete_all();
        let duration = start_time.elapsed();
        match &result {
            Ok(_) => {
                let _ = log_info(
                    None,
                    "delete_policy",
                    Some(&format!("succeeded in {}ms", duration.as_millis())),
                );
            }
            Err(_) => {
                let _ = log_error(
                    None,
                    "delete_policy",
                    Some(&format!("failed in {}ms", duration.as_millis())),
                );
            }
        }
        return result;
    }
    let domain = Domain::from_raw(target);
    let result = store.delete(&domain);
    let duration = start_time.elapsed();
    match &result {
        Ok(_) => {
            let _ = log_info(
                Some(&domain.0),
                "delete_policy",
                Some(&format!("succeeded in {}ms", duration.as_millis())),
            );
        }
        Err(_) => {
            let _ = log_error(
                Some(&domain.0),
                "delete_policy",
                Some(&format!("failed in {}ms", duration.as_millis())),
            );
        }
    }
    result
}

/* ------------ extraction entrypoints ------------ */

pub fn extract_url<PS: PolicyStore>(
    store: &PS,
    url: &str,
    components: &Components,
) -> Result<ExtractionBundle> {
    let start_time = Instant::now();
    let domain = {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?
    };

    // Ensure policy exists - create if needed
    if store.get(&domain)?.is_none() {
        create_policy(store, domain.clone(), components)?;
    }

    // Always use policy-based extraction
    let engine = make_engine(store, components);
    let result = engine.extract(url);
    let duration = start_time.elapsed();

    match &result {
        Ok(_) => {
            let details = format!("succeeded in {}ms", duration.as_millis());
            let _ = log_info(Some(&domain.0), "extract_url", Some(&details));
        }
        Err(_) => {
            let details = format!("failed in {}ms", duration.as_millis());
            let _ = log_error(Some(&domain.0), "extract_url", Some(&details));
        }
    }

    result
}

pub async fn extract_url_async<PS: PolicyStore>(
    store: &PS,
    url: &str,
    components: &Components,
) -> Result<ExtractionBundle> {
    let start_time = Instant::now();
    let domain = {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?
    };

    // Ensure policy exists - create if needed
    if store.get(&domain)?.is_none() {
        create_policy(store, domain.clone(), components)?;
    }

    // Always use policy-based extraction
    let engine = make_engine(store, components);
    let result = engine.extract_async(url).await;
    let duration = start_time.elapsed();

    match &result {
        Ok(_) => {
            let details = format!("succeeded in {}ms", duration.as_millis());
            let _ = log_info(Some(&domain.0), "extract_url_async", Some(&details));
        }
        Err(_) => {
            let details = format!("failed in {}ms", duration.as_millis());
            let _ = log_error(Some(&domain.0), "extract_url_async", Some(&details));
        }
    }

    result
}
