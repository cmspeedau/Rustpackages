// Repository index format and operations
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::manifest::Dependency;

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
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub desktop: Option<String>,
}

impl RepoIndex {
    pub fn from_toml(text: &str) -> Result<Self, crate::MpkgError> {
        toml::from_str(text).map_err(|e| crate::MpkgError::Parse(e.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), crate::MpkgError> {
        let raw = toml::to_string_pretty(self)
            .map_err(|e| crate::MpkgError::Parse(e.to_string()))?;
        std::fs::write(path, raw).map_err(crate::MpkgError::Io)
    }

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

    pub fn find(&self, name: &str, req: &VersionReq) -> Option<RepoEntry> {
        self.packages
            .iter()
            .filter(|p| p.name == name)
            .filter_map(|p| Version::parse(&p.version).ok().map(|v| (v, p)))
            .filter(|(v, _)| req.matches(v))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, p)| p.clone())
    }

    pub fn load(path: &Path) -> Result<Self, crate::MpkgError> {
        let raw = std::fs::read_to_string(path).map_err(crate::MpkgError::Io)?;
        Self::from_toml(&raw)
    }
}