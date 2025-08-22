use crate::{error::*, types::*};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

pub trait PolicyStore {
    fn get(&self, domain: &Domain) -> Result<Option<Policy>>;
    fn set(&self, policy: &Policy) -> Result<()>;
    fn list(&self) -> Result<Vec<Policy>>;
    fn delete(&self, domain: &Domain) -> Result<()>;
    fn delete_all(&self) -> Result<()>;
}

pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    pub fn new() -> Result<Self> {
        let proj = ProjectDirs::from("io", "qrawl", "qrawl")
            .ok_or_else(|| QrawlError::Other("could not resolve data dir".into()))?;
        let root = proj.data_local_dir().join("policies");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path_for(&self, d: &Domain) -> PathBuf {
        self.root.join(format!("{}.json", d.0))
    }
}

/* ---------- On-disk document shape ----------
{
  "<domain>": {
    "config": { "crawl": {...}, "scrape": {...} }
  }
}
---------------------------------------------- */

#[derive(Serialize, Deserialize)]
struct PolicyConfigDoc {
    crawl: CrawlConfig,
    scrape: ScrapeConfig,
}
#[derive(Serialize, Deserialize)]
struct PolicyDoc {
    config: PolicyConfigDoc,
}

impl PolicyStore for LocalFsStore {
    fn get(&self, domain: &Domain) -> Result<Option<Policy>> {
        let p = self.path_for(domain);
        if !p.exists() {
            return Ok(None);
        }
        let file = fs::File::open(&p)?;
        let map: BTreeMap<String, PolicyDoc> = serde_json::from_reader(file)?;
        if let Some(doc) = map.get(&domain.0) {
            Ok(Some(Policy {
                domain: domain.clone(),
                crawl: doc.config.crawl.clone(),
                scrape: doc.config.scrape.clone(),
            }))
        } else if let Some(doc) = map.values().next() {
            Ok(Some(Policy {
                domain: domain.clone(),
                crawl: doc.config.crawl.clone(),
                scrape: doc.config.scrape.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    fn set(&self, policy: &Policy) -> Result<()> {
        let p = self.path_for(&policy.domain);
        let mut map = BTreeMap::<String, PolicyDoc>::new();
        map.insert(
            policy.domain.0.clone(),
            PolicyDoc {
                config: PolicyConfigDoc {
                    crawl: policy.crawl.clone(),
                    scrape: policy.scrape.clone(),
                },
            },
        );
        let file = fs::File::create(&p)?;
        serde_json::to_writer_pretty(file, &map)?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<Policy>> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let fname = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let domain = Domain::from_raw(fname);

            let file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let map: BTreeMap<String, PolicyDoc> = match serde_json::from_reader(file) {
                Ok(m) => m,
                Err(_) => continue, // skip corrupt files
            };

            // Prefer canonical key, else any first value
            if let Some(doc) = map.get(&domain.0) {
                out.push(Policy {
                    domain: domain.clone(),
                    crawl: doc.config.crawl.clone(),
                    scrape: doc.config.scrape.clone(),
                });
            } else if let Some(doc) = map.values().next() {
                out.push(Policy {
                    domain: domain.clone(),
                    crawl: doc.config.crawl.clone(),
                    scrape: doc.config.scrape.clone(),
                });
            }
        }
        out.sort_by(|a, b| a.domain.0.cmp(&b.domain.0));
        Ok(out)
    }

    fn delete(&self, domain: &Domain) -> Result<()> {
        let p = self.path_for(domain);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    fn delete_all(&self) -> Result<()> {
        if !self.root.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}
