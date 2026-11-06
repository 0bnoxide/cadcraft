//! Path safety: safe name components, relative artifact paths, collisions, length limits.
//! Owner: Codex (see `docs/agents/slice-7-proposal.md`). Behaviour reference: AutoGIS-Civil3D
//! `ProposalFiles.cs` and the planner's path checks.

use crate::Issue;
use std::collections::BTreeSet;

/// One portable comparison key for every path collision check.
pub fn case_fold(value: &str) -> String {
    value.to_lowercase()
}

fn unsafe_path(value: &str) -> Issue {
    Issue::new("unsafe_path", "Expected a portable safe path or name").at(value)
}

pub fn component(value: &str) -> Result<(), Issue> {
    let stem = value.split('.').next().unwrap_or_default().trim_end().to_uppercase();
    let numbered = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT"));
    if value.is_empty()
        || value.trim() != value
        || value.ends_with('.')
        || value.encode_utf16().count() > 255
        || value.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || numbered.is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
    {
        return Err(unsafe_path(value));
    }
    Ok(())
}

pub fn relative(value: &str) -> Result<String, Issue> {
    if value.encode_utf16().count() >= 32767 {
        return Err(unsafe_path(value));
    }
    let path = value.replace('\\', "/");
    for part in path.split('/') {
        component(part)?;
    }
    Ok(path)
}

pub fn absolute(value: &str) -> Result<String, Issue> {
    if value.encode_utf16().count() >= 32767 {
        return Err(unsafe_path(value));
    }
    let path = value.replace('\\', "/");
    if let Some(tail) = path.strip_prefix("//") {
        if tail.split('/').count() < 2 {
            return Err(unsafe_path(value));
        }
        relative(tail)?;
    } else if let Some(tail) = path.strip_prefix('/') {
        if !tail.is_empty() {
            relative(tail)?;
        }
    } else if path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && path.get(1..3) == Some(":/") {
        if let Some(tail) = path.get(3..).filter(|s| !s.is_empty()) {
            relative(tail)?;
        }
    } else {
        return Err(unsafe_path(value));
    }
    Ok(path)
}

/// Entries are `(relative_path, is_file)`; folders may contain other entries.
pub fn collisions(entries: &[(String, bool)]) -> Vec<Issue> {
    let mut seen = BTreeSet::new();
    let files: BTreeSet<_> = entries.iter().filter(|(_, is_file)| *is_file).map(|(p, _)| case_fold(&p.replace('\\', "/"))).collect();
    let mut issues = Vec::new();
    for (path, _) in entries {
        let key = case_fold(&path.replace('\\', "/"));
        let file_parent = key.match_indices('/').any(|(at, _)| key.get(..at).is_some_and(|p| files.contains(p)));
        if !seen.insert(key) || file_parent {
            issues.push(Issue::new("output_collision", "Output paths collide").at(path));
        }
    }
    issues
}
