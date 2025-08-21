use crate::{engine::*, store::*, types::*, error::*};
use url::Url;
use crate::impls::{ReqwestFetcher, DefaultScraper};

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

pub fn make_engine<'a, PS: PolicyStore>(store: &'a PS, components: &'a Components) -> Engine<'a, PS> {
    Engine::new(store, &*components.fetcher, &*components.scraper, components.opts.clone())
}

/* ------------ policy helpers ------------ */

pub fn policy_create<PS: PolicyStore>(store: &PS, domain: Domain) -> Result<Policy> {
    let p = crate::policy::new_policy(domain);
    store.set(&p)?;
    Ok(p)
}
pub fn policy_read<PS: PolicyStore>(store: &PS, target: &str) -> Result<Option<Policy>> {
    if target == "all" { return Err(QrawlError::Other("use policy_list for 'all'".into())); }
    store.get(&Domain(target.to_string()))
}
pub fn policy_list<PS: PolicyStore>(store: &PS) -> Result<Vec<Policy>> {
    store.list()
}
pub fn policy_update<PS: PolicyStore>(store: &PS, policy: &Policy) -> Result<()> {
    crate::policy::validate_policy(policy)?;
    store.set(policy)
}
pub fn policy_delete<PS: PolicyStore>(store: &PS, target: &str) -> Result<()> {
    if target == "all" { return store.delete_all(); }
    store.delete(&Domain(target.to_string()))
}

/* ------------ extraction entrypoints ------------ */

pub fn extract_url<PS: PolicyStore>(store: &PS, url: &str, unknown: bool, components: &Components) -> Result<ExtractionBundle> {
    let engine = make_engine(store, components);
    if unknown { engine.extract_unknown(url) } else { engine.extract_known(url) }
}

pub fn extract_url_auto<PS: PolicyStore>(store: &PS, url: &str, components: &Components) -> Result<ExtractionBundle> {
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
