//! Config loading: resolve DB and config-dir paths from env vars or defaults,
//! and parse companies.yaml.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct Companies {
    #[serde(default)]
    pub companies: Vec<Company>,
    #[serde(default)]
    pub default_company: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // some fields present for YAML shape parity with Python, unused in hook
pub struct Company {
    pub name: String,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub github_repos: Vec<String>,
    #[serde(default)]
    pub jira_projects: Vec<String>,
    #[serde(default)]
    pub gcal_calendars: Vec<String>,
    #[serde(default)]
    pub gcal_keywords: Vec<String>,
}

pub struct Paths {
    pub db: PathBuf,
    pub companies_yaml: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        let db = std::env::var_os("WORKLOG_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share/worklog/worklog.db"));

        let config_dir = std::env::var_os("WORKLOG_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config/worklog"));

        Self {
            db,
            companies_yaml: config_dir.join("companies.yaml"),
        }
    }
}

pub fn load_companies(path: &std::path::Path) -> Result<Companies> {
    if !path.exists() {
        return Ok(Companies::default());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_saphyr::from_str(&text).with_context(|| format!("parse {}", path.display()))
}
