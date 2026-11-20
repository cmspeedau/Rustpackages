use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::Path;

use crate::manifest::PkgManifest;

pub const MANIFEST_NAME: &str = "pkg.toml";

/// Build a `.mpk` (tar + zstd) archive from a source directory containing
/// a `pkg.toml` at its root.
pub fn build_package(src_dir: &Path, out_path: &Path) -> io::Result<PkgManifest> {
    let manifest_path = src_dir.join(MANIFEST_NAME);
    if !manifest_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} not found in {}", MANIFEST_NAME, src_dir.display()),
        ));
    }
    let manifest: PkgManifest = toml::from_str(&fs::read_to_string(&manifest_path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    manifest
        .validate()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(out_path)?;
    let encoder = zstd::Encoder::new(file, 3)?;
    let mut tar = tar::Builder::new(encoder);
    tar.append_path_with_name(&manifest_path, MANIFEST_NAME)?;
    for entry in walkdir::WalkDir::new(src_dir).min_depth(1) {
        let entry = entry.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let path = entry.path();
        if path == manifest_path {
            continue;
        }
        let rel = path
            .strip_prefix(src_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if entry.file_type().is_dir() {
            tar.append_dir(rel, path)?;
        } else if entry.file_type().is_file() {
            tar.append_path_with_name(path, rel)?;
        }
    }
    let encoder = tar.into_inner()?;
    encoder.finish()?;
    Ok(manifest)
}

/// Extract a `.mpk` archive into `dest` and return its parsed manifest.
pub fn extract_package(pkg_path: &Path, dest: &Path) -> io::Result<PkgManifest> {
    let file = File::open(pkg_path)?;
    let decoder = zstd::Decoder::new(BufReader::new(file))?;
    let mut archive = tar::Archive::new(decoder);
    fs::create_dir_all(dest)?;
    archive.unpack(dest)?;
    let raw = fs::read_to_string(dest.join(MANIFEST_NAME)).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("archive is missing {}: {}", MANIFEST_NAME, e),
        )
    })?;
    let manifest: PkgManifest = toml::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    manifest
        .validate()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(manifest)
}
