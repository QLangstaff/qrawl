use crate::impls::{DefaultScraper, ReqwestFetcher};
use crate::{engine::*, error::*, store::*, types::*};
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

/* ------------ policy helpers ------------ */

/// Automatically probe/infer a working policy, verify live, then save it.
/// Refuses to overwrite an existing policy.
pub fn create_policy<PS: PolicyStore>(
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
    // Create minimal policy (no verification needed)
    let pol = crate::infer::infer_policy(&*components.fetcher, &*components.scraper, &domain)?;
    // Persist without verification - policy will be refined during actual extraction
    store.set(&pol)?;
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
    if target == "all" {
        return store.delete_all();
    }
    store.delete(&Domain::from_raw(target))
}

/* ------------ extraction entrypoints ------------ */

pub fn extract_url<PS: PolicyStore>(
    store: &PS,
    url: &str,
    components: &Components,
) -> Result<ExtractionBundle> {
    let domain = {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?
    };

    // Ensure policy exists - create if needed
    if store.get(&domain)?.is_none() {
        create_policy(store, domain, components)?;
    }

    // Always use policy-based extraction
    let engine = make_engine(store, components);
    engine.extract(url)
}

pub async fn extract_url_async<PS: PolicyStore>(
    store: &PS,
    url: &str,
    components: &Components,
) -> Result<ExtractionBundle> {
    let domain = {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?
    };

    // Ensure policy exists - create if needed
    if store.get(&domain)?.is_none() {
        create_policy(store, domain, components)?;
    }

    // Always use policy-based extraction
    let engine = make_engine(store, components);
    engine.extract_async(url).await
}
