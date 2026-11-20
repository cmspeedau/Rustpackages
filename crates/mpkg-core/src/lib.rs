pub mod repo;
pub use repo::RepoIndex;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub provides: Vec<String>,
    pub depends: Vec<String>,
}

pub async fn local_cache_dir() -> PathBuf {
    // TODO: respect XDG on Unix, fallback to /var/cache/mpkg
    PathBuf::from("/var/cache/mpkg")
}