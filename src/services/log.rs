use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub domain: Option<String>,
    pub event: String,
    pub details: Option<String>,
}

pub struct ActivityLogger {
    log_path: PathBuf,
}

impl ActivityLogger {
    pub fn new() -> crate::Result<Self> {
        let user_dirs = directories::UserDirs::new().ok_or_else(|| {
            crate::types::QrawlError::storage_error(
                "initialization",
                "could not determine home directory",
            )
        })?;
        let home = user_dirs.home_dir();
        let qrawl_dir = home.join(".qrawl");
        fs::create_dir_all(&qrawl_dir)?;

        Ok(Self {
            log_path: qrawl_dir.join("activity.log"),
        })
    }

    pub fn log(
        &self,
        level: LogLevel,
        domain: Option<&str>,
        event: &str,
        details: Option<&str>,
    ) -> crate::Result<()> {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            domain: domain.map(|d| d.to_string()),
            event: event.to_string(),
            details: details.map(|d| d.to_string()),
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        let level_str = match entry.level {
            LogLevel::Info => "🟢",
            LogLevel::Error => "🔴",
        };

        let domain_str = entry.domain.as_deref().unwrap_or("*");
        let details_str = entry.details.as_deref().unwrap_or("");

        writeln!(
            file,
            "{} {} {} {} {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            level_str,
            entry.event,
            domain_str,
            details_str
        )?;

        Ok(())
    }

    pub fn read_logs(
        &self,
        domain_filter: Option<&str>,
        errors_only: bool,
    ) -> crate::Result<Vec<String>> {
        if !self.log_path.exists() {
            return Ok(vec![]);
        }

        let file = std::fs::File::open(&self.log_path)?;
        let reader = BufReader::new(file);
        let mut matching_lines = Vec::new();

        for line in reader.lines() {
            let line = line?;

            // Filter by error level if requested
            if errors_only && !line.contains("🔴") {
                continue;
            }

            // Filter by domain if requested
            if let Some(domain) = domain_filter {
                if !line.contains(domain) {
                    continue;
                }
            }

            matching_lines.push(line);
        }

        // Return most recent entries first (reverse chronological)
        matching_lines.reverse();
        Ok(matching_lines)
    }

    pub fn info(
        &self,
        domain: Option<&str>,
        event: &str,
        details: Option<&str>,
    ) -> crate::Result<()> {
        self.log(LogLevel::Info, domain, event, details)
    }

    pub fn error(
        &self,
        domain: Option<&str>,
        event: &str,
        details: Option<&str>,
    ) -> crate::Result<()> {
        self.log(LogLevel::Error, domain, event, details)
    }
}
