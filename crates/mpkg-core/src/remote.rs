//! Remote repository fetching over HTTP(S) or file:// URLs.
//!
//! Only the index and `.mpk` archives are fetched; everything else stays local.

use std::path::{Path, PathBuf};

use crate::error::{MpkgError, Result};
use crate::repo::RepoIndex;

/// Fetch raw bytes from a URL. Supports `http(s)://` and `file://`.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| MpkgError::Io(std::io::Error::new(e.kind(), format!("{path}: {e}"))));
    }
    #[cfg(feature = "http")]
    {
        let resp = ureq::get(url)
            .call()
            .map_err(|e| MpkgError::Network(format!("GET {url}: {e}")))?;
        let mut buf = Vec::new();
        resp.into_reader().read_to_end(&mut buf).map_err(MpkgError::Io)?;
        return Ok(buf);
    }
    #[cfg(not(feature = "http"))]
    Err(MpkgError::Network(format!(
        "HTTP support not compiled in (enable the `http` feature); only file:// URLs are supported: {url}"
    )))
}

/// Download and parse a signed repo index from a base URL.
pub fn fetch_index(base_url: &str) -> Result<RepoIndex> {
    let url = format!("{}/index.toml", base_url.trim_end_matches('/'));
    let bytes = fetch_bytes(&url)?;
    let text = String::from_utf8(bytes).map_err(|e| MpkgError::Parse(format!("index.toml not UTF-8: {e}")))?;
    RepoIndex::from_toml(&text)
}

/// Download a `.mpk` archive for a package into `dest_dir`, returning the file path.
pub fn fetch_package(base_url: &str, name: &str, version: &str, dest_dir: &Path) -> Result<PathBuf> {
    let url = format!(
        "{}/packages/{name}-{version}.mpk",
        base_url.trim_end_matches('/')
    );
    let bytes = fetch_bytes(&url)?;
    std::fs::create_dir_all(dest_dir).map_err(MpkgError::Io)?;
    let dest = dest_dir.join(format!("{name}-{version}.mpk"));
    std::fs::write(&dest, bytes).map_err(MpkgError::Io)?;
    Ok(dest)
}
