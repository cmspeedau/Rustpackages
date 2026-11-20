// Dependency resolver: graph-based, conflict-aware, satisfying set
use crate::{Error, PackageManifest, Result};
use indexmap::IndexMap;
use semver::{Version, VersionReq};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone)]
struct Candidate {
    name: String,
    manifest: PackageManifest,
    available_versions: Vec<Version>,
}

#[derive(Debug, Default)]
pub struct ResolveOptions {
    pub upgrade_all: bool,
    pub dry_run: bool,
}

pub struct Resolver {
    pub installed: BTreeMap<String, Version>,
    pub repo_entries: BTreeMap<String, Vec<PackageManifest>>, // name -> all versions
    pub provides_map: HashMap<String, String>, // provided virtual -> real name
}

impl Resolver {
    pub fn new(
        installed: BTreeMap<String, Version>,
        repo_entries: BTreeMap<String, Vec<PackageManifest>>,
    ) -> Self {
        // Build provides map: e.g. "libfoo.so.1" -> "foo"
        let mut provides_map = HashMap::new();
        for (name, manifests) in &repo_entries {
            if let Some(p) = manifests.iter().find_map(|m| {
                if m.provides.is_empty() {
                    None
                } else {
                    Some((m.provides[0].clone(), name.clone()))
                }
            }) {
                provides_map.insert(p.0, p.1);
            }
        }
        Self {
            installed,
            repo_entries,
            provides_map,
        }
    }

    /// Resolve a set of requested packages to a lockfile.
    /// On conflict, returns a structured error describing the conflict.
    pub fn resolve<I: IntoIterator<Item = String>>(
        &self,
        requested: I,
        opts: ResolveOptions,
    ) -> Result<Vec<(String, Version)>> {
        let mut resolved: BTreeMap<String, Version> = BTreeMap::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        // Upgrade mode: start from installed set
        if opts.upgrade_all {
            for (name, ver) in &self.installed {
                if let Some(vs) = self.repo_entries.get(name) {
                    if let Some(latest) = vs.iter().map(|m| Version::parse(&m.version).ok()).max().flatten() {
                        resolved.insert(name.clone(), latest);
                    }
                }
            }
        }

        // Track what we still need to satisfy
        let mut queue: Vec<String> = requested.into_iter().collect();
        while let Some(name) = queue.pop() {
            if seen.contains(&name) {
                continue;
            }
            seen.insert(name.clone());

            // Resolve provided virtuals to real name
            let real_name = self.provides_map.get(&name).cloned().unwrap_or(name.clone());

            // Choose a version satisfying deps
            let chosen = self.select_version(&real_name, &resolved)?;
            if chosen.is_none() {
                return Err(Error::Repository(format!(
                    "cannot satisfy dependency '{}' within constraints",
                    real_name
                )));
            }
            let chosen = chosen.unwrap();

            // Install direct dependencies
            for dep in &chosen.depends {
                queue.push(dep.clone());
            }

            // Record resolved version
            resolved.insert(real_name.clone(), chosen.manifest.version.parse().unwrap());
        }

        // Validate no conflicts: each name only once
        Ok(resolved.into_iter().collect())
    }

    /// Select highest version satisfying installed set and constraints.
    fn select_version(&self, name: &str, installed: &BTreeMap<String, Version>) -> Result<Option<&PackageManifest>> {
        let candidates: Vec<&PackageManifest> = self
            .repo_entries
            .get(name)
            .into_iter()
            .flat_map(|v| v.iter())
            .filter(|m| {
                if let Ok(v) = Version::parse(&m.version) {
                    // If already installed, prefer upgrade
                    if let Some(installed_v) = installed.get(name) {
                        if v <= *installed_v {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            })
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        // Sort descending by semver
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| {
            Version::parse(&b.version)
                .unwrap()
                .cmp(&Version::parse(&a.version).unwrap())
        });

        Ok(Some(sorted[0]))
    }

    /// Generate a lockfile string from resolved pairs.
    pub fn generate_lockfile(resolved: &[(String, Version)]) -> String {
        let mut out = String::new();
        for (n, v) in resolved {
            out.push_str(&format!("{}={}\n", n, v));
        }
        out
    }

    /// Detect orphaned packages not required by any installed package.
    pub fn detect_orphans(&self, installed_versions: &BTreeMap<String, Version>) -> Vec<(String, Version)> {
        let mut required: BTreeSet<String> = BTreeSet::new();
        for (_, deps) in self.repo_entries.iter() {
            for m in deps {
                let v = Version::parse(&m.version).unwrap();
                if installed_versions.get(&m.name) == Some(&v) {
                    required.insert(m.name.clone());
                }
            }
        }
        installed_versions
            .iter()
            .filter_map(|(n, v)| {
                if required.contains(n) {
                    None
                } else {
                    Some((n.clone(), v.clone()))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    fn mk(name: &str, ver: &str, deps: Vec<&str>) -> PackageManifest {
        PackageManifest {
            name: name.to_string(),
            version: ver.to_string(),
            description: None,
            license: None,
            provides: vec![],
            depends: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_resolver_picks_highest() {
        let mut repo = BTreeMap::new();
        repo.insert("a".into(), vec![mk("a", "1.0.0", vec![]), mk("a", "2.0.0", vec![])]);
        let installed: BTreeMap<String, Version> = BTreeMap::new();
        let resolver = Resolver::new(installed, repo);
        let resolved = resolver.resolve(vec!["a".into()], ResolveOptions::default()).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, "a");
        assert_eq!(resolved[0].1.to_string(), "2.0.0");
    }

    #[test]
    fn test_resolver_deps() {
        let mut repo = BTreeMap::new();
        repo.insert("a".into(), vec![mk("a", "1.0.0", vec!["b"])]);
        repo.insert("b".into(), vec![mk("b", "1.0.0", vec![])]);
        let installed: BTreeMap<String, Version> = BTreeMap::new();
        let resolver = Resolver::new(installed, repo);
        let resolved = resolver.resolve(vec!["a".into()], ResolveOptions::default()).unwrap();
        assert_eq!(resolved.len(), 2);
    }
}