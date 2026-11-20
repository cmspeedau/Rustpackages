use semver::VersionReq;

use crate::repo::{RepoEntry, RepoIndex};

/// Parse a request like `foo` or `foo@^1.2` into (name, version req).
pub fn parse_request(s: &str) -> Result<(String, VersionReq), String> {
    match s.split_once('@') {
        Some((name, req)) => Ok((
            name.to_string(),
            VersionReq::parse(req).map_err(|e| format!("invalid req '{}': {}", req, e))?,
        )),
        None => Ok((s.to_string(), VersionReq::STAR)),
    }
}

/// Resolve requests against a repo index, returning entries in
/// dependency-first (topological) install order.
pub fn resolve(index: &RepoIndex, requests: &[String]) -> Result<Vec<RepoEntry>, String> {
    let mut order: Vec<RepoEntry> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    for req in requests {
        let (name, req) = parse_request(req)?;
        visit(index, &name, &req, &mut order, &mut done, &mut stack)?;
    }
    Ok(order)
}

fn visit(
    index: &RepoIndex,
    name: &str,
    req: &VersionReq,
    order: &mut Vec<RepoEntry>,
    done: &mut Vec<String>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    if done.contains(&name.to_string()) {
        return Ok(());
    }
    if stack.iter().any(|s| s == name) {
        let mut cycle = stack.clone();
        cycle.push(name.to_string());
        return Err(format!("dependency cycle detected: {}", cycle.join(" -> ")));
    }
    let entry = index
        .find(name, req)
        .ok_or_else(|| format!("no package '{}' matching '{}' in index", name, req))?;
    stack.push(name.to_string());
    for dep in &entry.dependencies {
        let dreq = VersionReq::parse(&dep.version)
            .map_err(|e| format!("invalid dep req '{}': {}", dep.version, e))?;
        visit(index, &dep.name, &dreq, order, done, stack)?;
    }
    stack.pop();
    done.push(name.to_string());
    order.push(entry);
    Ok(())
}
