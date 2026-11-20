use serde::{Deserialize, Serialize};

/// Parsed `pkg.toml` embedded in every `.mpk` archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgManifest {
    pub package: PackageMeta,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Desktop entry file (relative to package root) for GUI packages.
    #[serde(default)]
    pub desktop: Option<String>,
    /// Critical packages use the offline-finish (stage for next boot) pattern.
    #[serde(default)]
    pub critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_arch")]
    pub arch: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// A dependency: `name` plus a semver requirement string (e.g. `^1.2`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    #[serde(default = "default_req")]
    pub version: String,
}

fn default_arch() -> String {
    std::env::consts::ARCH.to_string()
}

fn default_req() -> String {
    "*".to_string()
}

impl PkgManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.package.name.is_empty() {
            return Err("package name must not be empty".into());
        }
        if !self
            .package
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(format!("invalid package name: {}", self.package.name));
        }
        semver::Version::parse(&self.package.version)
            .map_err(|e| format!("invalid version '{}': {}", self.package.version, e))?;
        for dep in &self.dependencies {
            semver::VersionReq::parse(&dep.version)
                .map_err(|e| format!("invalid dependency req '{}': {}", dep.version, e))?;
        }
        Ok(())
    }
}
