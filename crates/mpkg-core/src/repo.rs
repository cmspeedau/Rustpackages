use crate::{Error, PackageManifest, RepoIndex, Repository, Result};
use std::path::Path;

impl Repository {
    /// Load repository index from a JSON file on disk.
    pub fn load<P: AsRef<Path>>(repo_path: P, repo_name: &str) -> Result<Self> {
        let idx_path = repo_path.as_ref().join("index.json");
        let f = std::fs::File::open(&idx_path).map_err(|e| {
            Error::Repository(format!(
                "failed to open index.json at {}: {}",
                idx_path.display(),
                e
            ))
        })?;
        let mut repo = Self::from_reader(f)?;
        repo.name = repo_name.to_string();
        Ok(repo)
    }

    /// Write repository index to disk.
    pub fn write_index<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let idx_path = repo_path.as_ref().join("index.json");
        let f = std::fs::File::create(&idx_path)?;
        serde_json::to_writer_pretty(f, &self.index)?;
        Ok(())
    }
}