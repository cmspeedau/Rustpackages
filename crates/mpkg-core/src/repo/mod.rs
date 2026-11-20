mod index;
pub use index::{PackageEntry, RepoIndex, RepoUpdateRequest};

use std::path::{Path, PathBuf};

/// Client for interacting with a signed repository.
pub struct RepositoryClient {
    base_url: String,
    local_index_path: PathBuf,
}

impl RepositoryClient {
    pub fn new(base_url: impl Into<String>, local_cache: impl AsRef<Path>) -> Self {
        Self {
            base_url: base_url.into(),
            local_index_path: local_cache.as_ref().join("index.json"),
        }
    }

    /// Fetch the latest index from the repository, verify it, and write it locally.
    pub async fn update(&self, trusted_keys: &[ed25519_dalek::VerifyingKey]) -> Result<RepoIndex, String> {
        let client = reqwest::Client::new();
        let remote_index_url = format!("{}/index.json", self.base_url);
        let bytes = client
            .get(&remote_index_url)
            .send()
            .await
            .map_err(|e| format!("fetch index: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("read remote index bytes: {e}"))?;

        // Persist to local cache
        tokio::fs::create_dir_all(self.local_index_path.parent().unwrap())
            .await
            .map_err(|e| format!("create cache dir: {e}"))?;
        tokio::fs::write(&self.local_index_path, &bytes)
            .await
            .map_err(|e| format!("write local index: {e}"))?;

        // Load and verify
        RepoIndex::load_and_verify(&self.local_index_path, trusted_keys).await
    }

    /// List available packages in the repo (from the local cached index).
    pub async fn search(&self) -> Result<Vec<PackageEntry>, String> {
        let idx = RepoIndex::load_and_verify(&self.local_index_path, &[]).await?;
        Ok(idx.packages.into_values().collect())
    }
}