use crate::{error::*, types::*, policy::validate_policy};
use directories::ProjectDirs;
use std::{fs, path::{Path, PathBuf}};

pub trait PolicyStore: Send + Sync {
    fn get(&self, domain: &Domain) -> Result<Option<Policy>>;
    fn set(&self, policy: &Policy) -> Result<()>;
    fn list(&self) -> Result<Vec<Policy>>;
    fn delete(&self, domain: &Domain) -> Result<()>;
    fn delete_all(&self) -> Result<()>;
}

pub struct LocalFsStore { root: PathBuf }

impl LocalFsStore {
    pub fn new() -> Result<Self> {
        let root = policy_dir()?;
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    fn path_for(&self, domain: &Domain) -> PathBuf {
        self.root.join(format!("{}.json", domain.0))
    }
}

fn policy_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("QRAWL_HOME") {
        return Ok(Path::new(&home).join("policies"));
    }
    // author: QLangstaff, app: qrawl
    let dirs = ProjectDirs::from("", "QLangstaff", "qrawl")
        .ok_or_else(|| QrawlError::Other("cannot resolve data dir".into()))?;
    Ok(PathBuf::from(dirs.data_dir()).join("policies"))
}

impl PolicyStore for LocalFsStore {
    fn get(&self, domain: &Domain) -> Result<Option<Policy>> {
        let p = self.path_for(domain);
        if !p.exists() { return Ok(None); }
        let bytes = fs::read(p)?;
        let policy: Policy = serde_json::from_slice(&bytes)?;
        Ok(Some(policy))
    }
    fn set(&self, policy: &Policy) -> Result<()> {
        validate_policy(policy)?;
        let p = self.path_for(&policy.domain);
        let pretty = serde_json::to_string_pretty(policy)?;
        fs::write(p, pretty)?;
        Ok(())
    }
    fn list(&self) -> Result<Vec<Policy>> {
        let mut out = vec![];
        for e in fs::read_dir(&self.root)? {
            let e = e?;
            if e.file_type()?.is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("json") {
                let bytes = fs::read(e.path())?;
                let pol: Policy = serde_json::from_slice(&bytes)?;
                out.push(pol);
            }
        }
        Ok(out)
    }
    fn delete(&self, domain: &Domain) -> Result<()> {
        let p = self.path_for(domain);
        if p.exists() { fs::remove_file(p)?; }
        Ok(())
    }
    fn delete_all(&self) -> Result<()> {
        if self.root.exists() {
            for e in fs::read_dir(&self.root)? {
                let e = e?;
                if e.file_type()?.is_file() { fs::remove_file(e.path())?; }
            }
        }
        Ok(())
    }
}
