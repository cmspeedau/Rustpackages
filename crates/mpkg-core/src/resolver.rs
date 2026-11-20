// Dependency resolver for mpkg
//
// - Supports semver ranges for dependencies
// - Handles virtual packages via `provides`
// - Detects conflicts and chooses a satisfying set
// - Detects orphaned packages

use crate::{PackageManifest, Result};
use indexmap::IndexMap;
use semver::{Version, VersionReq};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub upgrade_all: bool,
}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    version: Version,
    manifest: PackageManifest,
}

#[derive(Debug)]
struct ResolverState {
    repo_map: BTreeMap<String, Vec<PackageManifest>>, // name -> all versions
    installed: BTreeMap<String, Version>,             // name -> installed version
    plan: BTreeMap<String, Version>,                   // name -> resolved version
    provides_map: BTreeMap<String, Vec<(String, Version)>>, // virtual -> real
    queue: VecDeque<String>,                           // install queue
    conflicts: BTreeSet<(String, Version)>,            // explicit conflicts
}

pub struct Resolver {
    state: ResolverState,
}

impl Resolver {
    pub fn new(
        installed: BTreeMap<String, Version>,
        repo_map: BTreeMap<String, Vec<PackageManifest>>,
    ) -> Self {
        // Build provides map across repo
        let mut provides_map: BTreeMap<String, Vec<(String, Version)>> = BTreeMap::new();
        for (name, manifests) in &repo_map {
            for mf in manifests {
                for prov in &mf.provides {
                    provides_map
                        .entry(prov.clone())
                        .or_default()
                        .push((name.clone(), Version::parse(&mf.version).unwrap()));
                }
            }
        }

        Self {
            state: ResolverState {
                repo_map,
                installed,
                plan: BTreeMap::new(),
                provides_map,
                queue: VecDeque::new(),
                conflicts: BTreeSet::new(),
            },
        }
    }

    /// Resolve a list of package specs (with optional semver constraints).
    /// An empty list upgrades everything.
    pub fn resolve(
        mut self,
        specs: impl IntoIterator<Item = String>,
        opts: ResolveOptions,
    ) -> Result<Vec<(String, String)>> {
        // Seed queue
        if opts.upgrade_all {
            for (name, ver) in &self.state.installed {
                self.state.queue.push_back(name.clone());
                self.state.plan.insert(name.clone(), ver.clone());
            }
        } else {
            for spec in specs {
                let (name, req) = parse_spec(&spec);
                // If already installed and satisfies req, keep it; else upgrade
                if let Some(v) = self.state.installed.get(&name) {
                    if req.matches(v) {
                        // keep current
                        self.state.plan.insert(name.clone(), v.clone());
                        continue;
                    }
                }
                self.state.queue.push_back(name.clone());
            }
        }

        // Resolve graph
        while let Some(name) = self.state.queue.pop_front() {
            if self.state.plan.contains_key(&name) {
                // already resolved
                continue;
            }

            // Find the best version in repo for this name
            let best = if let Some(candidates) = self.state.repo_map.get(&name) {
                if let Some(ver) = self.pick_best_for_spec(&name, None, candidates) {
                    ver
                } else {
                    // No candidate satisfies constraints → conflict
                    self.state.conflicts.insert((name.clone(), Version::new(0, 0, 0)));
                    continue;
                }
            } else if let Some(provs) = self.state.provides_map.get(&name) {
                // Resolve virtual package to a real one
                let mut best_real: Option<(String, Version)> = None;
                for (real_name, real_ver) in provs {
                    if let Some(candidates) = self.state.repo_map.get(real_name) {
                        if self.pick_best_for_spec(real_name, None, candidates).is_some() {
                            if let Some((_, cur_ver)) = &best_real {
                                if real_ver > cur_ver {
                                    best_real = Some((real_name.clone(), real_ver.clone()));
                                }
                            } else {
                                best_real = Some((real_name.clone(), real_ver.clone()));
                            }
                        }
                    }
                }
                if let Some((real_name, real_ver)) = best_real {
                    self.state.queue.push_back(real_name.clone());
                    self.state.plan.insert(real_name.clone(), real_ver.clone());
                    continue;
                } else {
                    self.state.conflicts.insert((name.clone(), Version::new(0, 0, 0)));
                    continue;
                }
            } else {
                // Package not found anywhere
                self.state.conflicts.insert((name.clone(), Version::new(0, 0, 0)));
                continue;
            };

            // Node for this name/version
            let node = Node {
                name: name.clone(),
                version: best.clone(),
                manifest: self
                    .state
                    .repo_map
                    .get(&name)
                    .and_then(|v| {
                        v.iter()
                            .find(|m| Version::parse(&m.version).unwrap() == best)
                            .cloned()
                    })
                    .ok_or_else(|| crate::Error::Resolver(format!("manifest missing for {} {}", name, best)))?,
            };

            // Record resolution
            self.state.plan.insert(node.name.clone(), node.version.clone());

            // Enqueue dependencies
            for dep in &node.manifest.depends {
                let (dep_name, dep_req) = parse_spec(dep);
                // If already planned or installed, skip
                if self.state.plan.contains_key(&dep_name) {
                    continue;
                }
                if let Some(installed_v) = self.state.installed.get(&dep_name) {
                    if dep_req.matches(installed_v) {
                        continue;
                    }
                }
                self.state.queue.push_back(dep_name);
            }
        }

        if !self.state.conflicts.is_empty() {
            let mut c = self.state.conflicts.iter().collect::<Vec<_>>();
            c.sort();
            return Err(crate::Error::Resolver(format!(
                "conflicts detected: {:?}",
                c.into_iter().map(|(n, v)| format!("{}@{}", n, v)).collect::<Vec<_>>()
            )));
        }

        Ok(self
            .state
            .plan
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect())
    }

    /// Detect packages installed but not required by any dependency or explicit install
    pub fn detect_orphans(&self, installed_map: &BTreeMap<String, Version>) -> Vec<(String, Version)> {
        let mut required: BTreeSet<String> = BTreeSet::new();
        // Required by plan
        for (k, _) in &self.state.plan {
            required.insert(k.clone());
        }
        // Required by installed packages' dependencies
        for (name, ver) in installed_map {
            if let Some(manifests) = self.state.repo_map.get(name) {
                if let Some(mf) = manifests.iter().find(|m| Version::parse(&m.version).unwrap() == *ver) {
                    for dep in &mf.depends {
                        let (dep_name, _) = parse_spec(dep);
                        required.insert(dep_name);
                    }
                }
            }
        }
        installed_map
            .iter()
            .filter(|(k, _)| !required.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Choose the highest version satisfying no constraints (used by upgrade)
    fn pick_best_for_spec(
        &self,
        name: &str,
        _req: Option<VersionReq>,
        candidates: &[PackageManifest],
    ) -> Option<Version> {
        let mut best: Option<Version> = None;
        for mf in candidates {
            let v = Version::parse(&mf.version).ok()?;
            if let Some(b) = &best {
                if v > *b {
                    best = Some(v);
                }
            } else {
                best = Some(v);
            }
        }
        best
    }

    /// Generate a lockfile string from resolved set
    pub fn generate_lockfile(resolved: &[(String, String)]) -> String {
        let mut out = String::new();
        for (name, ver) in resolved {
            out.push_str(&format!("{} {}\n", name, ver));
        }
        out
    }
}

/// Parse "name" or "name@>=1.2.0" into (name, req)
fn parse_spec(spec: &str) -> (String, VersionReq) {
    if let Some(idx) = spec.find('@') {
        let name = spec[..idx].to_string();
        let req_str = &spec[idx + 1..];
        let req = VersionReq::parse(req_str).unwrap_or_else(|_| VersionReq::any());
        (name, req)
    } else {
        (spec.to_string(), VersionReq::any())
    }
}
