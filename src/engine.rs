use crate::{error::*, types::*, store::PolicyStore, policy::new_policy};
use url::Url;

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub follow_depth: u32,
    pub max_children: u32,
    pub unknown_allows_probe: bool,
}
impl Default for EngineOptions {
    fn default() -> Self {
        Self { follow_depth: 0, max_children: 20, unknown_allows_probe: true }
    }
}

/// Future-proof hook: decide if links should be followed (this run).
/// Today: just returns the policy flag. Later: add heuristics here.
fn effective_follow_enabled(
    area_pol: &AreaPolicy,
    _area_out: &AreaContent,
    _page_url: &url::Url,
    _opts: &EngineOptions,
) -> bool {
    area_pol.follow_links.enabled
}

pub trait Fetcher: Send + Sync {
    fn name(&self) -> &'static str;
    fn fetch_blocking(&self, _url: &str, _cfg: &CrawlConfig) -> Result<String> {
        Err(QrawlError::Other("Fetcher not implemented (Step-2)".into()))
    }
}
pub trait Scraper: Send + Sync {
    fn name(&self) -> &'static str;
    fn scrape(&self, _url: &str, _html: &str, _cfg: &ScrapeConfig) -> Result<PageExtraction> {
        Err(QrawlError::Other("Scraper not implemented (Step-2)".into()))
    }
}

pub struct Engine<'a, PS: PolicyStore> {
    pub store: &'a PS,
    pub fetcher: &'a dyn Fetcher,
    pub scraper: &'a dyn Scraper,
    pub opts: EngineOptions,
}

impl<'a, PS: PolicyStore> Engine<'a, PS> {
    pub fn new(store: &'a PS, fetcher: &'a dyn Fetcher, scraper: &'a dyn Scraper, opts: EngineOptions) -> Self {
        Self { store, fetcher, scraper, opts }
    }

    pub fn extract_url(&self, url: &str, unknown: bool) -> Result<ExtractionBundle> {
        if unknown { self.extract_unknown(url) } else { self.extract_known(url) }
    }

    pub fn extract_unknown(&self, url: &str) -> Result<ExtractionBundle> {
        if !self.opts.unknown_allows_probe {
            return Err(QrawlError::Other("unknown pipeline disabled by options".into()));
        }
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let domain = Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?;
        let policy = new_policy(domain.clone());
        self.store.set(&policy)?;
        self.extract_known(url)
    }

    pub fn extract_known(&self, url: &str) -> Result<ExtractionBundle> {
        let u = Url::parse(url).map_err(|_| QrawlError::InvalidUrl(url.into()))?;
        let domain = Domain::from_url(&u).ok_or(QrawlError::MissingDomain)?;
        let policy = self.store.get(&domain)?
            .ok_or_else(|| QrawlError::Other(format!("no policy for domain {}", domain.0)))?;

        let html = self.fetcher.fetch_blocking(url, &policy.crawl)?;
        let parent = self.scraper.scrape(url, &html, &policy.scrape)?;

        let mut children: Vec<PageExtraction> = Vec::new();
        if self.opts.follow_depth > 0 {
            let base_domain = domain.0.clone();
            let mut to_visit: Vec<String> = Vec::new();

            for (area_pol, area_out) in policy.scrape.areas.iter().zip(parent.areas.iter()) {
                let effective_follow = effective_follow_enabled(area_pol, area_out, &u, &self.opts);
                if !effective_follow { continue; }

                let mut taken = 0u32;
                for l in &area_out.links {
                    if taken >= area_pol.follow_links.max { break; }
                    if (to_visit.len() as u32) >= self.opts.max_children { break; }

                    if let Ok(abs) = Url::options().base_url(Some(&u)).parse(&l.href) {
                        let ok = match area_pol.follow_links.scope {
                            FollowScope::SameDomain => abs.domain().map(|d| d == base_domain).unwrap_or(false),
                            FollowScope::AllowList => area_pol.follow_links.allow_domains.iter().any(|d| abs.domain()==Some(d.as_str())),
                            FollowScope::AnyDomain => true,
                        };
                        if !ok { continue; }
                        let abs_str = abs.to_string();
                        if area_pol.follow_links.dedupe {
                            if to_visit.contains(&abs_str) { continue; }
                        }
                        to_visit.push(abs_str);
                        taken += 1;
                    }
                }
                if (to_visit.len() as u32) >= self.opts.max_children { break; }
            }

            for link in to_visit {
                if (children.len() as u32) >= self.opts.max_children { break; }
                let lu = Url::parse(&link).map_err(|_| QrawlError::InvalidUrl(link.clone()))?;
                let ldomain = Domain::from_url(&lu).ok_or(QrawlError::MissingDomain)?;
                let lpolicy = match self.store.get(&ldomain)? {
                    Some(p) => p,
                    None => {
                        if ldomain.0 == base_domain {
                            policy.clone()
                        } else {
                            let np = new_policy(ldomain.clone());
                            self.store.set(&np)?;
                            np
                        }
                    }
                };
                let lhtml = self.fetcher.fetch_blocking(&link, &lpolicy.crawl)?;
                let lextract = self.scraper.scrape(&link, &lhtml, &lpolicy.scrape)?;
                children.push(lextract);
            }
        }

        Ok(ExtractionBundle { parent, children })
    }
}
