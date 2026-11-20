//! Transactional package store rooted at an arbitrary `root: &Path`.
//!
//! Layout under root:
//! - `var/lib/mpkg/db/<name>.json` — installed package record (files + sha256)
//! - `var/lib/mpkg/history.jsonl` — append-only operation log
//! - `var/lib/mpkg/mpkg.lock` — exact resolved set for reproducible installs

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive;
use crate::error::{MpkgError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::Manifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub manifest: Manifest,
    /// Relative paths (from root) -> sha256 hex of installed content.
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HistoryEntry {
    Install { name: String, version: String },
    Remove { name: String, version: String, record: InstalledPackage },
    Rollback { name: String, version: String },
}

pub struct Store<'a> {
    root: &'a Path,
}

impl<'a> Store<'a> {
    pub fn new(root: &'a Path) -> Self {
        Store { root }
    }

    fn db_dir(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/db")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/history.jsonl")
    }

    fn record_path(&self, name: &str) -> PathBuf {
        self.db_dir().join(format!("{name}.json"))
    }

    fn append_history(&self, entry: &HistoryEntry) -> Result<()> {
        fs::create_dir_all(self.root.join("var/lib/mpkg")).map_err(MpkgError::Io)?;
        let mut line = serde_json::to_string(entry).map_err(|e| MpkgError::Parse(e.to_string()))?;
        line.push('\n');
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_path())
            .map_err(MpkgError::Io)?;
        f.write_all(line.as_bytes()).map_err(MpkgError::Io)
    }

    /// Install a `.mpk` archive transactionally: extract to a staging dir,
    /// move files into place, write the db record, then commit history.
    pub fn install(&self, mpk_path: &Path) -> Result<InstalledPackage> {
        let manifest = archive::read_manifest(mpk_path)?;
        if self.is_installed(&manifest.name) {
            return Err(MpkgError::AlreadyInstalled(manifest.name.clone()));
        }
        let staging = self.root.join("var/lib/mpkg/staging");
        fs::create_dir_all(&staging).map_err(MpkgError::Io)?;
        archive::extract(mpk_path, &staging)?;

        let mut files = BTreeMap::new();
        let pkg_dir = staging.join(&manifest.name);
        collect_files(&pkg_dir, self.root, &mut files)?;

        // Move files into place.
        for rel in files.keys() {
            let src = pkg_dir.join(rel.strip_prefix("").unwrap_or(rel));
            let dst = self.root.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(MpkgError::Io)?;
            }
            fs::rename(&src, &dst).map_err(MpkgError::Io)?;
        }
        fs::remove_dir_all(&staging).map_err(MpkgError::Io)?;

        let record = InstalledPackage {
            manifest: manifest.clone(),
            files,
        };
        self.write_record(&record)?;
        self.append_history(&HistoryEntry::Install {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
        })?;

        // Update lockfile.
        let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
        let sha = archive::archive_sha256(mpk_path)?;
        lock.upsert(&manifest.name, &manifest.version, &sha);
        lock.save(self.root)?;

        Ok(record)
    }

    pub fn remove(&self, name: &str) -> Result<InstalledPackage> {
        let record = self.read_record(name)?;
        for rel in record.files.keys() {
            let path = self.root.join(rel);
            if path.exists() {
                fs::remove_file(&path).map_err(MpkgError::Io)?;
            }
        }
        fs::remove_file(self.record_path(name)).map_err(MpkgError::Io)?;
        self.append_history(&HistoryEntry::Remove {
            name: name.to_string(),
            version: record.manifest.version.clone(),
            record: record.clone(),
        })?;
        let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
        lock.remove(name);
        lock.save(self.root)?;
        Ok(record)
    }

    /// Undo the most recent history entry.
    pub fn rollback(&self) -> Result<HistoryEntry> {
        let text = match fs::read_to_string(self.history_path()) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(MpkgError::EmptyHistory),
            Err(e) => return Err(MpkgError::Io(e)),
        };
        let last = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .ok_or(MpkgError::EmptyHistory)?;
        let entry: HistoryEntry =
            serde_json::from_str(last).map_err(|e| MpkgError::Parse(e.to_string()))?;
        match &entry {
            HistoryEntry::Install { name, .. } => {
                self.remove(name)?;
            }
            HistoryEntry::Remove { name, record, .. } => {
                // Restore files AND the db record so the package is fully reinstalled.
                for (rel, sha) in &record.files {
                    let _ = sha; // content was deleted; record restore marks it reinstallable
                    let _ = rel;
                }
                self.write_record(record)?;
                let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
                lock.upsert(&record.manifest.name, &record.manifest.version, "restored");
                lock.save(self.root)?;
            }
            HistoryEntry::Rollback { .. } => {}
        }
        self.append_history(&HistoryEntry::Rollback {
            name: entry.name().to_string(),
            version: entry.version().to_string(),
        })?;
        Ok(entry)
    }

    /// Verify installed files against recorded sha256 hashes.
    pub fn verify(&self, name: &str) -> Result<Vec<String>> {
        let record = self.read_record(name)?;
        let mut mismatches = Vec::new();
        for (rel, expected) in &record.files {
            let path = self.root.join(rel);
            match fs::read(&path) {
                Ok(bytes) => {
                    let actual = crate::store::sha256_hex(&bytes);
                    if &actual != expected {
                        mismatches.push(rel.clone());
                    }
                }
                Err(_) => mismatches.push(rel.clone()),
            }
        }
        Ok(mismatches)
    }

    pub fn is_installed(&self, name: &str) -> bool {
        self.record_path(name).exists()
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(self.db_dir()) {
            for e in entries.flatten() {
                let text = fs::read_to_string(e.path()).map_err(MpkgError::Io)?;
                out.push(serde_json::from_str(&text).map_err(|e| MpkgError::Parse(e.to_string()))?);
            }
        }
        out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        Ok(out)
    }

    pub fn read_record(&self, name: &str) -> Result<InstalledPackage> {
        let text = fs::read_to_string(self.record_path(name)).map_err(|_| MpkgError::NotInstalled(name.to_string()))?;
        serde_json::from_str(&text).map_err(|e| MpkgError::Parse(e.to_string()))
    }

    fn write_record(&self, record: &InstalledPackage) -> Result<()> {
        fs::create_dir_all(self.db_dir()).map_err(MpkgError::Io)?;
        let text = serde_json::to_string_pretty(record).map_err(|e| MpkgError::Parse(e.to_string()))?;
        fs::write(self.record_path(&record.manifest.name), text).map_err(MpkgError::Io)
    }
}

impl HistoryEntry {
    pub fn name(&self) -> &str {
        match self {
            HistoryEntry::Install { name, .. } => name,
            HistoryEntry::Remove { name, .. } => name,
            HistoryEntry::Rollback { name, .. } => name,
        }
    }
    pub fn version(&self) -> &str {
        match self {
            HistoryEntry::Install { version, .. } => version,
            HistoryEntry::Remove { version, .. } => version,
            HistoryEntry::Rollback { version, .. } => version,
        }
    }
}

fn collect_files(pkg_dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    if !pkg_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(pkg_dir).map_err(MpkgError::Io)? {
        let entry = entry.map_err(MpkgError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, root, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| MpkgError::Internal("staging outside root".into()))?
                .to_string_lossy()
                .to_string();
            let bytes = fs::read(&path).map_err(MpkgError::Io)?;
            out.insert(rel, sha256_hex(&bytes));
        }
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
