use crate::types::*;
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
        let proj = ProjectDirs::from("io", "qrawl", "qrawl").ok_or_else(|| {
            QrawlError::storage_error("initialization", "could not resolve data dir")
        })?;
        let root = proj.data_local_dir().join("policies");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path_for(&self, d: &Domain) -> PathBuf {
        self.root.join(format!("{}.json", d.0))
    }
}

#[derive(Serialize, Deserialize)]
struct PolicyConfigDoc {
    fetch: FetchConfig,
    scrape: ScrapeConfig,
    #[serde(default = "default_performance_profile")]
    performance_profile: PerformanceProfile,
}

fn default_performance_profile() -> PerformanceProfile {
    PerformanceProfile {
        optimal_timeout_ms: 20_000,
        working_strategy: BotEvadeStrategy::default(),
        avg_response_size_bytes: 0,
        strategies_tried: vec![],
        strategies_failed: vec![],
        last_tested_at: chrono::Utc::now(),
        success_rate: 0.0,
    }
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

        // Only return policy if the exact domain key is found
        // Remove dangerous fallback that could return wrong policy
        if let Some(doc) = map.get(&domain.0) {
            Ok(Some(Policy {
                domain: domain.clone(),
                fetch: doc.config.fetch.clone(),
                scrape: doc.config.scrape.clone(),
                performance_profile: doc.config.performance_profile.clone(),
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
                    fetch: policy.fetch.clone(),
                    scrape: policy.scrape.clone(),
                    performance_profile: policy.performance_profile.clone(),
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
            let fname = match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => continue, // Skip files with invalid filenames
            };
            let domain = Domain::from_raw(fname);

            let file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let map: BTreeMap<String, PolicyDoc> = match serde_json::from_reader(file) {
                Ok(m) => m,
                Err(_) => continue, // skip corrupt files
            };

            // Only use exact domain key matches to avoid confusion
            if let Some(doc) = map.get(&domain.0) {
                out.push(Policy {
                    domain: domain.clone(),
                    fetch: doc.config.fetch.clone(),
                    scrape: doc.config.scrape.clone(),
                    performance_profile: doc.config.performance_profile.clone(),
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
