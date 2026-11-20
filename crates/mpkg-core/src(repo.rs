use std::fs;
use std::path::Path;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::manifest::Dependency;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid index: {0}")]
    Invalid(String),
}

/// A repository index (`index.toml`) listing all available packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub packages: Vec<RepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub arch: String,
    /// sha256 (hex) of the `.mpk` file.
    pub sha256: String,
    pub url: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub desktop: Option<String>,
}

impl RepoIndex {
    pub fn load(path: &Path) -> Result<Self, RepoError> {
        let raw = fs::read_to_string(path)?;
        toml::from_str(&raw).map_err(|e| RepoError::Invalid(e.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), RepoError> {
        let raw = toml::to_string_pretty(self)
            .map_err(|e| RepoError::Invalid(e.to_string()))?;
        fs::write(path, raw)?;
        Ok(())
    }

    /// Case-insensitive substring search over name and description.
    pub fn search(&self, query: &str) -> Vec<&RepoEntry> {
        let q = query.to_lowercase();
        self.packages
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Highest version of `name` matching `req`.
    pub fn find(&self, name: &str, req: &VersionReq) -> Option<RepoEntry> {
        self.packages
            .iter()
            .filter(|p| p.name == name)
            .filter_map(|p| Version::parse(&p.version).ok().map(|v| (v, p)))
            .filter(|(v, _)| req.matches(v))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, p)| p.clone())
    }
}
