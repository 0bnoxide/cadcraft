//! Closed figure specifications and deterministic, host-free map-series planning.
use crate::{Issue, paths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_JOBS: usize = 1000;
pub const MAX_SPEC_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigureSpec {
    pub figure_id: String,
    pub source_drawing: String,
    pub layout: String,
    #[serde(default)]
    pub required_layers: Vec<String>,
    #[serde(default)]
    pub visible_layers: Vec<String>,
    #[serde(default)]
    pub hidden_layers: Vec<String>,
    #[serde(default)]
    pub text: BTreeMap<String, String>,
    pub extent_boundary_layer: Option<String>,
    #[serde(default = "default_buffer")]
    pub extent_buffer_pct: f64,
}
fn default_buffer() -> f64 {
    5.0
}
fn invalid(message: impl Into<String>) -> Issue {
    Issue::new("invalid_spec", message)
}

impl FigureSpec {
    pub fn parse(bytes: &[u8]) -> Result<Self, Issue> {
        if bytes.len() > MAX_SPEC_BYTES {
            return Err(invalid("Figure spec exceeds 1 MiB"));
        }
        let s: Self = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        for value in [&s.figure_id, &s.source_drawing, &s.layout].into_iter().chain(s.extent_boundary_layer.iter()) {
            if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
                return Err(invalid("Expected a nonempty figure id, source, layout and boundary name"));
            }
        }
        if !s.extent_buffer_pct.is_finite() || !(0.0..=1000.0).contains(&s.extent_buffer_pct) {
            return Err(invalid("Extent buffer must be 0..1000 percent"));
        }
        for list in [&s.required_layers, &s.visible_layers, &s.hidden_layers] {
            if list.len() > MAX_JOBS || list.iter().any(|v| v.trim().is_empty() || v.len() > 255 || v.chars().any(char::is_control)) {
                return Err(invalid("Invalid or excessive layer list"));
            }
        }
        if s.visible_layers.iter().any(|v| s.hidden_layers.iter().any(|h| h.eq_ignore_ascii_case(v))) {
            return Err(invalid("A layer cannot be both visible and hidden"));
        }
        if s.text.len() > MAX_JOBS
            || s.text.iter().any(|(k, v)| {
                k.is_empty() || k.len() > 255 || v.len() > 65536 || k.contains(['{', '}']) || ["Site", "Event", "FigureId"].contains(&k.as_str())
            })
        {
            return Err(invalid("Invalid text map or reserved Site, Event or FigureId key"));
        }
        Ok(s)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub spec_index: usize,
    pub site: String,
    pub event: String,
    pub filename: String,
}

pub fn safe_token(value: &str) -> String {
    let mut out = String::new();
    let mut invalid_run = false;
    for c in value.trim().chars() {
        if c.is_ascii_alphanumeric() || "_.-".contains(c) {
            out.push(c);
            invalid_run = false;
        } else if !invalid_run {
            out.push('_');
            invalid_run = true;
        }
    }
    out
}
fn labels(values: &[String]) -> Result<Vec<String>, Issue> {
    if values.is_empty() || values.len() > MAX_JOBS || values.iter().any(|v| v.trim().is_empty() || v.len() > 255 || v.chars().any(char::is_control))
    {
        return Err(Issue::new("invalid_input", "Expected 1..1000 nonempty labels, each at most 255 bytes"));
    }
    let mut seen = BTreeSet::new();
    Ok(values.iter().filter(|v| seen.insert((*v).clone())).cloned().collect())
}

pub fn series(specs: &[FigureSpec], sites: &[String], events: &[String], mode: &str, format: &str) -> Result<Vec<Job>, Issue> {
    if !["per_site", "per_map_type", "historical", "combined_appendix"].contains(&mode) || !["pdf", "png"].contains(&format) {
        return Err(Issue::new("invalid_input", "Unknown series mode or output format"));
    }
    let sites = labels(sites)?;
    let events = labels(events)?;
    let count = sites.len().checked_mul(events.len()).and_then(|n| n.checked_mul(specs.len()));
    if specs.is_empty() || count.is_none_or(|n| n > MAX_JOBS) {
        return Err(Issue::new("job_limit", "Expected 1..1000 jobs"));
    }
    let mut indices = Vec::with_capacity(count.unwrap_or(0));
    for (si, _) in sites.iter().enumerate() {
        for (ei, _) in events.iter().enumerate() {
            for (fi, _) in specs.iter().enumerate() {
                indices.push((si, ei, fi));
            }
        }
    }
    match mode {
        "per_map_type" => indices.sort_by_key(|(s, e, f)| (*f, *s, *e)),
        "historical" => indices.sort_by_key(|(s, e, f)| (*e, *s, *f)),
        _ => {}
    }
    let mut jobs = Vec::with_capacity(indices.len());
    let mut names = BTreeSet::new();
    for (si, ei, fi) in indices {
        let site = sites.get(si).ok_or_else(|| invalid("Site missing"))?;
        let event = events.get(ei).ok_or_else(|| invalid("Event missing"))?;
        let spec = specs.get(fi).ok_or_else(|| invalid("Spec missing"))?;
        let (s, e, f) = (safe_token(site), safe_token(event), safe_token(&spec.figure_id));
        let stem = match mode {
            "per_map_type" => format!("{f}_{s}_{e}"),
            "historical" => format!("{s}_{f}_{e}"),
            "combined_appendix" => format!("Appendix_{:03}_{s}_{e}_{f}", jobs.len() + 1),
            _ => format!("{s}_{e}_{f}"),
        };
        let filename = format!("{stem}.{format}");
        paths::component(&filename)?;
        if !names.insert(paths::case_fold(&filename)) {
            return Err(Issue::new("output_collision", "Jobs produce the same filename").at(&filename));
        }
        jobs.push(Job { spec_index: fi, site: site.clone(), event: event.clone(), filename });
    }
    Ok(jobs)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Clone, Debug, Serialize)]
pub struct Qa {
    pub severity: Severity,
    pub code: String,
    pub subject: String,
    pub message: String,
}
impl Qa {
    pub fn new(severity: Severity, code: &str, subject: impl Into<String>, message: &str) -> Self {
        Self { severity, code: code.into(), subject: subject.into(), message: message.into() }
    }
}
