mod repo;
pub use repo::{PackageEntry, RepoIndex, RepoUpdateRequest, RepositoryClient};

mod resolver;
pub use resolver::{ResolveOptions, Resolver};

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
    #[error("Resolver error: {0}")]
    Resolver(String),
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

pub async fn list_installed_packages(backend: &FsBackend) -> Result<Vec<(String, semver::Version)>> {
    use tokio::fs::read_dir;
    let mut pkgs = Vec::new();
    if !backend.packages_dir().exists() {
        return Ok(pkgs);
    }
    let mut entries = read_dir(backend.packages_dir()).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(v) = semver::Version::parse(name) {
                    pkgs.push((name.to_string(), v));
                }
            }
        }
    }
    Ok(pkgs)
}

pub async fn resolve_install(
    backend: &FsBackend,
    names: &[String],
) -> Result<LockFile> {
    // Load repo index from cache
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        return Err(Error::Repository("no local index; run `mpkg update` first".into()));
    }
    let trusted_keys = crate::repo::load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await
        .map_err(|e| Error::Repository(e.to_string()))?;
    let idx = RepoIndex::load_and_verify(&idx_path, &trusted_keys)
        .await
        .map_err(|e| Error::Repository(e.to_string()))?;

    // Build repo map: name -> all versions
    let mut repo_map: std::collections::BTreeMap<String, Vec<PackageManifest>> = std::collections::BTreeMap::new();
    for entry in idx.packages.values() {
        let mf: PackageManifest = serde_json::from_str(&entry.manifest).map_err(|e| Error::Json(e))?;
        repo_map.entry(mf.name.clone()).or_default().push(mf);
    }

    // Current installed versions
    let installed = list_installed_packages(backend)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    let resolver = Resolver::new(installed, repo_map);
    let resolved = resolver.resolve(names.iter().cloned(), ResolveOptions::default())
        .map_err(|e| Error::Resolver(e.to_string()))?;

    Ok(LockFile { resolved })
}

pub async fn resolve_upgrade(backend: &FsBackend) -> Result<LockFile> {
    resolve_install(backend, &[]).await
}

pub async fn install_lock(backend: &FsBackend, lock: &LockFile) -> Result<()> {
    use tokio::fs::{create_dir_all, write};

    for (name, ver) in &lock.resolved {
        let target = backend.packages_dir().join(name);
        create_dir_all(&target).await?;
        write(target.join("package.ok"), "ok").await?;
    }
    Ok(())
}

pub async fn write_lockfile(backend: &FsBackend, lock: &LockFile) -> Result<()> {
    let s = Resolver::generate_lockfile(&lock.resolved);
    tokio::fs::write(backend.lockfile_path(), s).await?;
    Ok(())
}

pub async fn autoremove(backend: &FsBackend) -> Result<Vec<(String, semver::Version)>> {
    let installed = list_installed_packages(backend).await?;
    let idx_path = backend.repos_dir().join("index.json");
    if !idx_path.exists() {
        return Ok(vec![]);
    }
    let trusted_keys = crate::repo::load_trusted_keys(&backend.repos_dir().join("trusted-keys")).await
        .map_err(|e| Error::Repository(e.to_string()))?;
    let idx = RepoIndex::load_and_verify(&idx_path, &trusted_keys)
        .await
        .map_err(|e| Error::Repository(e.to_string()))?;

    let mut repo_map: std::collections::BTreeMap<String, Vec<PackageManifest>> = std::collections::BTreeMap::new();
    for entry in idx.packages.values() {
        let mf: PackageManifest = serde_json::from_str(&entry.manifest).map_err(|e| Error::Json(e))?;
        repo_map.entry(mf.name.clone()).or_default().push(mf);
    }

    let installed_map: std::collections::BTreeMap<String, semver::Version> = installed.into_iter().collect();
    let resolver = Resolver::new(installed_map.clone(), repo_map);
    let orphans = resolver.detect_orphans(&installed_map);

    Ok(orphans)
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