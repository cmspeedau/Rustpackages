//! Lockfile support (`mpkg.lock`).
//!
//! Records the exact resolved set of packages so installs are reproducible
//! and `upgrade` can compute a diff.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{MpkgError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lockfile {
    pub packages: BTreeMap<String, LockedPackage>,
}

impl Lockfile {
    pub fn load(root: &Path) -> Result<Option<Lockfile>> {
        let path = root.join("var/lib/mpkg/mpkg.lock");
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(toml::from_str(&text).map_err(|e| MpkgError::Parse(format!("mpkg.lock: {e}")))?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(MpkgError::Io(e)),
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join("var/lib/mpkg");
        std::fs::create_dir_all(&dir).map_err(MpkgError::Io)?;
        let text = toml::to_string_pretty(self).map_err(|e| MpkgError::Parse(format!("mpkg.lock: {e}")))?;
        std::fs::write(dir.join("mpkg.lock"), text).map_err(MpkgError::Io)
    }

    pub fn upsert(&mut self, name: &str, version: &str, sha256: &str) {
        self.packages.insert(
            name.to_string(),
            LockedPackage {
                name: name.to_string(),
                version: version.to_string(),
                sha256: sha256.to_string(),
            },
        );
    }

    pub fn remove(&mut self, name: &str) {
        self.packages.remove(name);
    }
}
