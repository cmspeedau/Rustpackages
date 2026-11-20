//! Transactional package store rooted at an arbitrary `root: &Path`.
//!
//! Layout under root:
//! - `var/lib/mpkg/db/<name>.json` — installed package record (files + sha256)
//! - `var/lib/mpkg/history.jsonl` — append-only operation log
//! - `var/lib/mpkg/mpkg.lock` — exact resolved set for reproducible installs
//! - `var/lib/mpkg/trash/<name>/…` — removed file contents, for rollback

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::archive;
use crate::error::{MpkgError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::PkgManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub manifest: PkgManifest,
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

    fn state_dir(&self) -> PathBuf {
        self.root.join("var/lib/mpkg")
    }

    fn db_dir(&self) -> PathBuf {
        self.state_dir().join("db")
    }

    fn history_path(&self) -> PathBuf {
        self.state_dir().join("history.jsonl")
    }

    fn trash_dir(&self, name: &str) -> PathBuf {
        self.state_dir().join("trash").join(name)
    }

    fn record_path(&self, name: &str) -> PathBuf {
        self.db_dir().join(format!("{name}.json"))
    }

    fn append_history(&self, entry: &HistoryEntry) -> Result<()> {
        fs::create_dir_all(self.state_dir()).map_err(MpkgError::Io)?;
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

    /// Full operation history, oldest first.
    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let text = match fs::read_to_string(self.history_path()) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(MpkgError::Io(e)),
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| MpkgError::Parse(e.to_string())))
            .collect()
    }

    /// Install a `.mpk` archive transactionally: extract to a staging dir,
    /// move files into place, write the db record, then commit history.
    pub fn install(&self, mpk_path: &Path) -> Result<InstalledPackage> {
        let manifest = archive::read_manifest(mpk_path)?;
        manifest
            .validate()
            .map_err(MpkgError::Internal)?;
        if self.is_installed(&manifest.package.name) {
            return Err(MpkgError::AlreadyInstalled(manifest.package.name.clone()));
        }
        let staging = self.state_dir().join("staging");
        fs::create_dir_all(&staging).map_err(MpkgError::Io)?;
        archive::extract(mpk_path, &staging)?;

        // Record paths relative to the extracted package root.
        let pkg_dir = staging.join(&manifest.package.name);
        let mut files = BTreeMap::new();
        collect_files(&pkg_dir, &pkg_dir, &mut files)?;

        // Move files into place.
        for rel in files.keys() {
            let src = pkg_dir.join(rel);
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
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
        })?;

        // Update lockfile.
        let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
        let sha = archive::archive_sha256(mpk_path)?;
        lock.upsert(&manifest.package.name, &manifest.package.version, &sha);
        lock.save(self.root)?;

        Ok(record)
    }

    /// Remove a package: move its files to the trash (so rollback can restore
    /// them), delete the db record, and log the operation.
    pub fn remove(&self, name: &str) -> Result<InstalledPackage> {
        let record = self.read_record(name)?;
        let trash = self.trash_dir(name);
        for rel in record.files.keys() {
            let path = self.root.join(rel);
            if path.exists() {
                let dst = trash.join(rel);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent).map_err(MpkgError::Io)?;
                }
                fs::rename(&path, &dst).map_err(MpkgError::Io)?;
            }
        }
        fs::remove_file(self.record_path(name)).map_err(MpkgError::Io)?;
        self.append_history(&HistoryEntry::Remove {
            name: name.to_string(),
            version: record.manifest.package.version.clone(),
            record: record.clone(),
        })?;
        let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
        lock.remove(name);
        lock.save(self.root)?;
        Ok(record)
    }

    /// Undo the most recent history entry.
    pub fn rollback(&self) -> Result<HistoryEntry> {
        let entries = self.history()?;
        let entry = entries.last().cloned().ok_or(MpkgError::EmptyHistory)?;
        match &entry {
            HistoryEntry::Install { name, .. } => {
                self.remove(name)?;
            }
            HistoryEntry::Remove { name, record, .. } => {
                // Restore file contents from the trash, then the db record.
                let trash = self.trash_dir(name);
                for rel in record.files.keys() {
                    let src = trash.join(rel);
                    let dst = self.root.join(rel);
                    if src.exists() {
                        if let Some(parent) = dst.parent() {
                            fs::create_dir_all(parent).map_err(MpkgError::Io)?;
                        }
                        fs::rename(&src, &dst).map_err(MpkgError::Io)?;
                    }
                }
                fs::remove_dir_all(&trash).ok();
                self.write_record(record)?;
                let mut lock = Lockfile::load(self.root)?.unwrap_or_default();
                lock.upsert(&record.manifest.package.name, &record.manifest.package.version, "restored");
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
    /// Returns the list of mismatched/missing files (empty = OK).
    pub fn verify(&self, name: &str) -> Result<Vec<String>> {
        let record = self.read_record(name)?;
        let mut mismatches = Vec::new();
        for (rel, expected) in &record.files {
            let path = self.root.join(rel);
            match fs::read(&path) {
                Ok(bytes) => {
                    let actual = sha256_hex(&bytes);
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
        out.sort_by(|a, b| a.manifest.package.name.cmp(&b.manifest.package.name));
        Ok(out)
    }

    pub fn read_record(&self, name: &str) -> Result<InstalledPackage> {
        let text = fs::read_to_string(self.record_path(name))
            .map_err(|_| MpkgError::NotInstalled(name.to_string()))?;
        serde_json::from_str(&text).map_err(|e| MpkgError::Parse(e.to_string()))
    }

    fn write_record(&self, record: &InstalledPackage) -> Result<()> {
        fs::create_dir_all(self.db_dir()).map_err(MpkgError::Io)?;
        let text =
            serde_json::to_string_pretty(record).map_err(|e| MpkgError::Parse(e.to_string()))?;
        fs::write(self.record_path(&record.manifest.package.name), text).map_err(MpkgError::Io)
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

/// Recursively collect `dir` files into `out` as (path relative to `base`, sha256).
fn collect_files(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(MpkgError::Io)? {
        let entry = entry.map_err(MpkgError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .map_err(|_| MpkgError::Internal("file outside package dir".into()))?
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
