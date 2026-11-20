mod repo;
pub use repo::{PackageEntry, RepoIndex, RepoUpdateRequest, RepositoryClient};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid package name: {0}")]
    InvalidName(String),
    #[error("Repository error: {0}")]
    Repository(String),
    #[error("Lockfile conflict: {0}")]
    LockConflict(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub provides: Vec<String>,
    pub depends: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LockFile {
    pub resolved: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct FsBackend {
    pub root: PathBuf,
}

impl FsBackend {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub async fn ensure_repos_dir(&self) -> Result<()> {
        tokio::fs::create_dir_all(self.repos_dir()).await?;
        Ok(())
    }

    pub fn repos_dir(&self) -> PathBuf { self.root.join("repos") }
    pub fn packages_dir(&self) -> PathBuf { self.root.join("packages") }
    pub fn lockfile_path(&self) -> PathBuf { self.root.join("mpkg.lock") }
    pub fn repo_path(&self, name: &str) -> PathBuf { self.repos_dir().join(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_fs_backend_dirs() {
        let td = tempdir().unwrap();
        let fb = FsBackend::new(td.path());
        fb.ensure_repos_dir().await.unwrap();
        assert!(fb.repos_dir().exists());
        assert!(fb.packages_dir().exists());
    }
}