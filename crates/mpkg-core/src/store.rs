use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::archive;
use crate::manifest::PkgManifest;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid package: {0}")]
    Invalid(String),
    #[error("package '{0}' is not installed")]
    NotInstalled(String),
    #[error("no history to roll back")]
    EmptyHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub manifest: PkgManifest,
    pub files: Vec<RecordedFile>,
    pub installed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub op: String, // "install" | "remove"
    pub name: String,
    pub version: String,
    /// Backup slot (relative to var/lib/mpkg/backup) holding removed files.
    pub backup: Option<String>,
    pub files: Vec<String>,
    pub at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut f, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// All operations are relative to `root`, so tests can use a tempdir as a
/// fake rootfs.
pub struct Store<'a> {
    pub root: &'a Path,
}

impl<'a> Store<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self { root }
    }

    fn db_dir(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/installed")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/history.jsonl")
    }

    fn staging_dir(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/staging")
    }

    fn backup_dir(&self) -> PathBuf {
        self.root.join("var/lib/mpkg/backup")
    }

    fn record_path(&self, name: &str) -> PathBuf {
        self.db_dir().join(format!("{}.toml", name))
    }

    pub fn get(&self, name: &str) -> Result<Option<InstalledPackage>, StoreError> {
        let path = self.record_path(name);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path)?;
        let rec: InstalledPackage = toml::from_str(&raw)
            .map_err(|e| StoreError::Invalid(format!("corrupt record for {}: {}", name, e)))?;
        Ok(Some(rec))
    }

    pub fn list(&self) -> Result<Vec<InstalledPackage>, StoreError> {
        let mut out = Vec::new();
        let dir = self.db_dir();
        if dir.is_dir() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.path().extension().map(|e| e == "toml").unwrap_or(false) {
                    let raw = fs::read_to_string(entry.path())?;
                    let rec: InstalledPackage = toml::from_str(&raw).map_err(|e| {
                        StoreError::Invalid(format!("corrupt record: {}", e))
                    })?;
                    out.push(rec);
                }
            }
        }
        out.sort_by(|a, b| a.manifest.package.name.cmp(&b.manifest.package.name));
        Ok(out)
    }

    /// Install a `.mpk` archive into the rootfs. Replaces any existing
    /// version of the same package. Records history for rollback.
    pub fn install(&self, pkg_path: &Path) -> Result<InstalledPackage, StoreError> {
        let staging = self
            .staging_dir()
            .join(format!("{}-{}", std::process::id(), now()));
        let _ = fs::remove_dir_all(&staging);
        let manifest = archive::extract_package(pkg_path, &staging)?;
        let name = manifest.package.name.clone();
        let version = manifest.package.version.clone();

        // Record every regular file with its hash.
        let mut files: Vec<RecordedFile> = Vec::new();
        for entry in walkdir::WalkDir::new(&staging).min_depth(1) {
            let entry =
                entry.map_err(|e| StoreError::Invalid(format!("walk error: {}", e)))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(&staging)
                    .map_err(|e| StoreError::Invalid(e.to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(RecordedFile {
                    sha256: sha256_file(entry.path())?,
                    path: rel,
                });
            }
        }

        // Replace an existing installation (without its own history entry).
        if let Some(old) = self.get(&name)? {
            for f in &old.files {
                let _ = fs::remove_file(self.root.join(&f.path));
            }
        }

        // Move staged files into the rootfs.
        for f in &files {
            let src = staging.join(&f.path);
            let dst = self.root.join(&f.path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            if fs::rename(&src, &dst).is_err() {
                fs::copy(&src, &dst)?;
                fs::remove_file(&src)?;
            }
        }

        let record = InstalledPackage {
            manifest,
            files,
            installed_at: now(),
        };
        fs::create_dir_all(self.db_dir())?;
        fs::write(
            self.record_path(&name),
            toml::to_string_pretty(&record)
                .map_err(|e| StoreError::Invalid(e.to_string()))?,
        )?;
        self.append_history(HistoryEntry {
            op: "install".into(),
            name: name.clone(),
            version,
            backup: None,
            files: record.files.iter().map(|f| f.path.clone()).collect(),
            at: now(),
        })?;
        let _ = fs::remove_dir_all(&staging);
        Ok(record)
    }

    /// Remove an installed package. Files are moved to a backup slot so the
    /// removal can be rolled back.
    pub fn remove(&self, name: &str) -> Result<(), StoreError> {
        let rec = self.get(name)?.ok_or_else(|| StoreError::NotInstalled(name.into()))?;
        let slot = format!("{}-{}", name, now());
        let backup_root = self.backup_dir().join(&slot);
        let mut paths = Vec::new();
        for f in &rec.files {
            let src = self.root.join(&f.path);
            let dst = backup_root.join(&f.path);
            if src.exists() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                if fs::rename(&src, &dst).is_err() {
                    fs::copy(&src, &dst)?;
                    fs::remove_file(&src)?;
                }
            }
            paths.push(f.path.clone());
        }
        fs::remove_file(self.record_path(name))?;
        self.append_history(HistoryEntry {
            op: "remove".into(),
            name: name.into(),
            version: rec.manifest.package.version.clone(),
            backup: Some(slot),
            files: paths,
            at: now(),
        })?;
        Ok(())
    }

    /// Undo the most recent history entry.
    pub fn rollback(&self) -> Result<HistoryEntry, StoreError> {
        let path = self.history_path();
        let file = File::open(&path)?;
        let lines: Vec<String> = BufReader::new(file)
            .lines()
            .collect::<io::Result<_>>()?
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .collect();
        let last = lines
            .last()
            .ok_or(StoreError::EmptyHistory)?
            .clone();
        let entry: HistoryEntry = serde_json::from_str(&last)
            .map_err(|e| StoreError::Invalid(format!("corrupt history: {}", e)))?;

        match entry.op.as_str() {
            "install" => {
                for f in &entry.files {
                    let _ = fs::remove_file(self.root.join(f));
                }
                let _ = fs::remove_file(self.record_path(&entry.name));
            }
            "remove" => {
                if let Some(slot) = &entry.backup {
                    for f in &entry.files {
                        let src = self.backup_dir().join(slot).join(f);
                        let dst = self.root.join(f);
                        if src.exists() {
                            if let Some(parent) = dst.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            let _ = fs::rename(&src, &dst);
                        }
                    }
                    // Restore the db record if we still have it is not
                    // possible (it was deleted); reinstall is the recovery.
                }
            }
            other => return Err(StoreError::Invalid(format!("unknown op '{}'", other))),
        }

        // Rewrite history without the last line.
        let mut out = File::create(&path)?;
        for line in &lines[..lines.len() - 1] {
            writeln!(out, "{}", line)?;
        }
        Ok(entry)
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>, StoreError> {
        let path = self.history_path();
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if !line.trim().is_empty() {
                out.push(
                    serde_json::from_str(&line)
                        .map_err(|e| StoreError::Invalid(format!("corrupt history: {}", e)))?,
                );
            }
        }
        Ok(out)
    }

    /// Check that every recorded file exists and matches its hash.
    /// Returns a list of problems (empty = clean).
    pub fn verify(&self, name: &str) -> Result<Vec<String>, StoreError> {
        let rec = self.get(name)?.ok_or_else(|| StoreError::NotInstalled(name.into()))?;
        let mut problems = Vec::new();
        for f in &rec.files {
            let p = self.root.join(&f.path);
            match fs::read(&p) {
                Ok(data) => {
                    let h = hex::encode(Sha256::digest(&data));
                    if h != f.sha256 {
                        problems.push(format!("modified: {}", f.path));
                    }
                }
                Err(_) => problems.push(format!("missing: {}", f.path)),
            }
        }
        Ok(problems)
    }

    fn append_history(&self, entry: HistoryEntry) -> Result<(), StoreError> {
        if let Some(parent) = self.history_path().parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_path())?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}
