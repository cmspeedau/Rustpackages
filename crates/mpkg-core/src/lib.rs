//! mpkg-core: rootfs-agnostic core library for the Rustpackages package manager.

pub mod archive;
pub mod error;
pub mod lockfile;
pub mod manifest;
pub mod remote;
pub mod repo;
pub mod resolve;
pub mod sign;
pub mod store;
pub mod resolver;

pub use error::{MpkgError, Result};
// Simple package manifest used by the CLI resolver.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub depends: Vec<String>,
}

// Simple repo index used by tests/repo.rs and the CLI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct RepoIndex {
    #[serde(default)]
    pub packages: Vec<PackageManifest>,
}

// Simple repository wrapper used by tests/repo.rs.
#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub index: RepoIndex,
}

impl Repository {
    pub fn write_index(&self, repo_path: &std::path::Path) -> Result<()> {
        let idx_path = repo_path.join("index.json");
        let f = std::fs::File::create(&idx_path).map_err(MpkgError::Io)?;
        serde_json::to_writer_pretty(f, &self.index).map_err(|e| MpkgError::Parse(e.to_string()))?;
        Ok(())
    }

    pub fn load(repo_path: &std::path::Path, repo_name: &str) -> Result<Self> {
        let idx_path = repo_path.join("index.json");
        let text = std::fs::read_to_string(&idx_path).map_err(MpkgError::Io)?;
        let idx: RepoIndex = serde_json::from_str(&text).map_err(|e| MpkgError::Parse(e.to_string()))?;
        Ok(Self { name: repo_name.to_string(), index: idx })
    }

    pub fn find(&self, name: &str) -> Option<PackageManifest> {
        self.index.packages.iter().find(|p| p.name == name).cloned()
    }
}

pub use resolver::{ResolveOptions, Resolver};