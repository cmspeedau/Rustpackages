// Repository index format and operations
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoIndex {
    /// Schema version for forward/backward compatibility
    pub version: u32,
    /// Packages keyed by name
    pub packages: BTreeMap<String, PackageEntry>,
    /// Ed25519 signature over the JSON bytes (base64)
    pub signature: String,
    /// Public key fingerprint (hex) that produced the signature
    pub key_fingerprint: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageEntry {
    pub name: String,
    pub version: String,
    pub archive: String, // "pkg_name-version.mpk"
    pub manifest: String, // serialized pkg.toml
    pub size: u64,
    pub sha256: String, // hex of the archive file
    pub deps: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoUpdateRequest {
    pub repo_url: String,
}

impl RepoIndex {
    /// Load an index from a JSON file at `path` and verify its signature using `trusted_keys`.
    /// Returns an error if the file is missing, malformed, or the signature is invalid.
    pub async fn load_and_verify<P: AsRef<Path>>(
        path: P,
        trusted_keys: &[ed25519_dalek::VerifyingKey],
    ) -> Result<Self, String> {
        let bytes = tokio::fs::read(&path).await.map_err(|e| format!("read index: {e}"))?;
        let candidate: Self = serde_json::from_slice(&bytes).map_err(|e| format!("parse index: {e}"))?;

        // Verify signature
        let sig_bytes = base64::decode(&candidate.signature).map_err(|e| format!("decode signature: {e}"))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        let key_fp = hex::decode(&candidate.key_fingerprint).map_err(|e| format!("decode fingerprint: {e}"))?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&key_fp.try_into().map_err(|_| "bad key length")?)
            .map_err(|e| format!("bad key: {e}"))?;

        key.verify(&bytes, &sig).map_err(|_| "signature verification failed".to_string())?
            .then_some(candidate)
            .ok_or_else(|| "signature invalid".to_string())
    }

    /// Save the index to `path` and sign it using `signing_key`.
    pub async fn save_and_sign<P: AsRef<Path>>(
        &self,
        path: P,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| format!("serialize index: {e}"))?;
        let sig = signing_key.sign(&json);
        let mut out = self.clone();
        out.signature = base64::encode(sig.to_bytes());
        out.key_fingerprint = hex::encode(signing_key.verifying_key().to_bytes());

        tokio::fs::write(&path, serde_json::to_vec_pretty(&out).map_err(|e| format!("reserialize: {e}"))?)
            .await
            .map_err(|e| format!("write index: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn temp_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join("mpkg_test_repo");
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn keys() -> (SigningKey, ed25519_dalek::VerifyingKey) {
        let mut csprng = rand_core::OsRng;
        let sk = SigningKey::generate(&mut csprng);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    #[tokio::test]
    async fn test_repo_index_sign_verify() {
        let tmp = temp_dir();
        let (sk, vk) = keys();

        let mut idx = RepoIndex {
            version: 1,
            packages: BTreeMap::new(),
            signature: String::new(),
            key_fingerprint: String::new(),
        };
        idx.packages.insert("foo".into(), PackageEntry {
            name: "foo".into(),
            version: "1.0.0".into(),
            archive: "foo-1.0.0.mpk".into(),
            manifest: "name=\"foo\"\nversion=\"1.0.0\"\n".into(),
            size: 123,
            sha256: "deadbeef".into(),
            deps: vec![],
        });

        let idx_path = tmp.join("index.json");
        idx.save_and_sign(&idx_path, &sk).await.unwrap();

        let loaded = RepoIndex::load_and_verify(&idx_path, &[vk]).await.unwrap();
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages["foo"].version, "1.0.0");
    }
}