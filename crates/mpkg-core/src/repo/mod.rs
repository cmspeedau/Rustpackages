mod index;
pub use index::{PackageEntry, RepoIndex, RepoUpdateRequest, RepositoryClient};

use std::path::PathBuf;

/// Client for interacting with a signed repository.
pub struct RepositoryClient {
    base_url: String,
    local_index_path: PathBuf,
}

impl RepositoryClient {
    pub fn new(base_url: impl Into<String>, local_cache: impl AsRef<std::path::Path>) -> Self {
        Self {
            base_url: base_url.into(),
            local_index_path: local_cache.as_ref().join("index.json"),
        }
    }

    /// Fetch the latest index from the repository, verify it, and write it locally.
    pub async fn update(&self, trusted_keys: &[ed25519_dalek::VerifyingKey]) -> Result<RepoIndex, String> {
        let remote_index_url = format!("{}/index.json", self.base_url);
        let bytes = reqwest::get(&remote_index_url)
            .await
            .map_err(|e| format!("fetch index: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("read remote index bytes: {e}"))?;

        tokio::fs::create_dir_all(self.local_index_path.parent().unwrap())
            .await
            .map_err(|e| format!("create cache dir: {e}"))?;
        tokio::fs::write(&self.local_index_path, &bytes)
            .await
            .map_err(|e| format!("write local index: {e}"))?;

        RepoIndex::load_and_verify(&self.local_index_path, trusted_keys).await
    }
}