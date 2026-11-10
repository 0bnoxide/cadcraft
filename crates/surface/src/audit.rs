//! Survey Preflight (point checks before building a surface) and Audit EG Surface (TIN checks).
//! Reference: AutoGIS-Civil3D `docs/superpowers/specs/2026-09-11-follow-on-workflow-candidates-design.md`
//! candidates 4 and 5. Owner: Claude (see `docs/agents/slice-5-analysis.md`).
//!
//! Both audits are pure: they read their inputs and return issues plus a [`Receipt`], and never
//! change anything. Issue codes and the receipt shape were agreed with Codex by Paseo message
//! (Claude 6201272a, Codex 4530efb7); `tinvolume` reuses [`Receipt`] for its report header.

use std::collections::{BTreeMap, HashMap, HashSet};

use spade::{DelaunayTriangulation, Point2, Triangulation};

use crate::triangulate::signed_area2;
use crate::{Region, Tin};

/// Version of the check set; bump when a check's meaning changes.
pub const VERSION: u32 = 2;

/// No points to check.
pub const PF_EMPTY: &str = "PF-EMPTY";
/// A point with a non-finite coordinate (skipped by every geometric check).
pub const PF_BAD_COORD: &str = "PF-BAD-COORD";
/// A point number used more than once.
pub const PF_DUP_NUMBER: &str = "PF-DUP-NUMBER";
/// A description code that matches no description key.
pub const PF_CODE_UNKEYED: &str = "PF-CODE-UNKEYED";
/// An elevation outside the configured band.
pub const PF_ELEV_BAND: &str = "PF-ELEV-BAND";
/// An elevation too far from the median of its Delaunay neighbours.
pub const PF_ELEV_NEIGHBOR: &str = "PF-ELEV-NEIGHBOR";
/// A point outside the site extent.
pub const PF_OUTSIDE_EXTENT: &str = "PF-OUTSIDE-EXTENT";
/// The point file's coordinate system differs from the expected one.
pub const PF_CRS_MISMATCH: &str = "PF-CRS-MISMATCH";
/// A TIN with no triangles.
pub const TIN_EMPTY: &str = "TIN-EMPTY";
/// A triangle with an out-of-range or repeated index, a non-finite vertex, or no plan area.
pub const TIN_BAD_TRIANGLE: &str = "TIN-BAD-TRIANGLE";
/// No (valid) outer boundary.
pub const TIN_NO_BOUNDARY: &str = "TIN-NO-BOUNDARY";
/// Two breakline segments cross away from a shared vertex.
pub const TIN_BREAKLINE_CROSS: &str = "TIN-BREAKLINE-CROSS";
/// A triangle steeper than the slope threshold with a plan edge shorter than the edge threshold.
pub const TIN_SPIKE: &str = "TIN-SPIKE";

/// Above this many breakline segments the crossing check is skipped (and the receipt says so).
pub const MAX_BREAKLINE_SEGMENTS: usize = 20_000;
/// Issues listed per code; past this they are only counted, so hostile input can't exhaust memory.
pub const MAX_ISSUES_PER_CODE: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One finding. `refs` names what it is about: point numbers, or triangle / breakline indices.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub at: Option<[f64; 3]>,
    pub refs: Vec<String>,
}

/// What was checked, with which thresholds, and how much was found. Deterministic for the same
/// inputs (sorted maps).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Receipt {
    pub check: String,
    pub version: u32,
    pub inputs: BTreeMap<String, String>,
    pub thresholds: BTreeMap<String, f64>,
    pub counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Audit {
    pub receipt: Receipt,
    pub issues: Vec<Issue>,
}

/// A survey point: its number, position and raw description (the code is its first word).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SurveyPoint {
    pub number: String,
    pub xyz: [f64; 3],
    pub description: String,
}

/// Preflight settings; a `None` check is not run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreflightOptions {
    /// Description keys, matched case-insensitively; a trailing `*` matches any suffix.
    pub keys: Option<Vec<String>>,
    /// Inclusive `(min, max)` elevation band.
    pub elevation_band: Option<(f64, f64)>,
    /// Largest allowed |z − median z of the point's Delaunay neighbours|.
    pub neighbor_tolerance: Option<f64>,
    pub extent: Option<Region>,
    pub points_crs: Option<String>,
    pub expected_crs: Option<String>,
}

/// Survey Preflight: is this point set fit to build an existing-ground surface from?
pub fn preflight(points: &[SurveyPoint], opts: &PreflightOptions) -> Audit {
    let mut issues = Sink::default();
    if points.is_empty() {
        issues.push(PF_EMPTY, Severity::Error, "the point set is empty".into(), None, vec![]);
    }
    if let (Some(have), Some(want)) = (&opts.points_crs, &opts.expected_crs)
        && !have.trim().eq_ignore_ascii_case(want.trim())
    {
        issues.push(PF_CRS_MISMATCH, Severity::Error, format!("points are in {have:?} but {want:?} is expected"), None, vec![]);
    }

    // Duplicate numbers, reported once per number in first-seen order.
    let mut seen: HashMap<&str, (usize, [f64; 3])> = HashMap::new();
    let mut order = Vec::new();
    for p in points {
        let n = p.number.trim();
        if n.is_empty() {
            continue;
        }
        let c = seen.entry(n).or_insert((0, p.xyz));
        if c.0 == 1 {
            order.push(n);
        }
        c.0 += 1;
    }
    for n in order {
        let (count, at) = seen.get(n).map_or((0, None), |&(c, at)| (c, Some(at).filter(|p| p.iter().all(|v| v.is_finite()))));
        issues.push(PF_DUP_NUMBER, Severity::Error, format!("point number {n} is used {count} times"), at, vec![n.to_owned()]);
    }

    let keys = opts.keys.as_deref().map(KeySet::new);
    let finite = |p: &SurveyPoint| p.xyz.iter().all(|v| v.is_finite());
    let mut good = Vec::with_capacity(points.len());
    for p in points {
        let refs = vec![p.number.clone()];
        if !finite(p) {
            issues.push(PF_BAD_COORD, Severity::Error, format!("point {} has a non-finite coordinate", p.number), None, refs);
            continue;
        }
        good.push(p);
        if let Some(keys) = &keys {
            let code = p.description.split_whitespace().next().unwrap_or("");
            if !keys.matches(code) {
                let what = if code.is_empty() { "has no description code".to_owned() } else { format!("code {code:?} matches no description key") };
                issues.push(PF_CODE_UNKEYED, Severity::Warning, format!("point {} {what}", p.number), Some(p.xyz), refs.clone());
            }
        }
        if let Some((lo, hi)) = opts.elevation_band
            && !(lo..=hi).contains(&p.xyz[2])
        {
            let msg = format!("point {} elevation {} is outside {lo}..{hi}", p.number, p.xyz[2]);
            issues.push(PF_ELEV_BAND, Severity::Error, msg, Some(p.xyz), refs.clone());
        }
        if let Some(r) = &opts.extent
            && !r.contains([p.xyz[0], p.xyz[1]])
        {
            issues.push(PF_OUTSIDE_EXTENT, Severity::Warning, format!("point {} is outside the site extent", p.number), Some(p.xyz), refs);
        }
    }
    if let Some(tol) = opts.neighbor_tolerance {
        for (p, median) in neighbor_outliers(&good, tol) {
            let msg = format!("point {} elevation {} is {:.3} from its neighbours' median {median}", p.number, p.xyz[2], (p.xyz[2] - median).abs());
            issues.push(PF_ELEV_NEIGHBOR, Severity::Warning, msg, Some(p.xyz), vec![p.number.clone()]);
        }
    }

    let mut inputs = BTreeMap::new();
    inputs.insert("keys".into(), opts.keys.as_ref().map_or("none".into(), |k| k.len().to_string()));
    inputs.insert("extent".into(), if opts.extent.is_some() { "given" } else { "none" }.into());
    inputs.insert("points_crs".into(), opts.points_crs.clone().unwrap_or_else(|| "none".into()));
    inputs.insert("expected_crs".into(), opts.expected_crs.clone().unwrap_or_else(|| "none".into()));
    let mut thresholds = BTreeMap::new();
    if let Some((lo, hi)) = opts.elevation_band {
        // An open side of the band is left out rather than written as an infinity.
        thresholds.extend([("elevation_min", lo), ("elevation_max", hi)].into_iter().filter(|(_, v)| v.is_finite()).map(|(k, v)| (k.to_owned(), v)));
    }
    if let Some(t) = opts.neighbor_tolerance {
        thresholds.insert("neighbor_tolerance".into(), t);
    }
    finish("preflight", inputs, thresholds, [("points", points.len())], issues)
}

/// Civil 3D style description keys: case-insensitive, `*` at the end matches any suffix. Exact
/// keys are a set lookup; only wildcard keys are scanned per point.
struct KeySet {
    exact: HashSet<String>,
    prefixes: Vec<String>,
}

impl KeySet {
    fn new(keys: &[String]) -> Self {
        let mut set = KeySet { exact: HashSet::new(), prefixes: Vec::new() };
        for k in keys {
            let k = k.trim().to_ascii_uppercase();
            match k.strip_suffix('*') {
                Some(prefix) => set.prefixes.push(prefix.to_owned()),
                None => {
                    set.exact.insert(k);
                }
            }
        }
        set
    }

    fn matches(&self, code: &str) -> bool {
        let code = code.to_ascii_uppercase();
        (!code.is_empty() && self.exact.contains(&code)) || self.prefixes.iter().any(|p| code.starts_with(p.as_str()))
    }
}

/// How many wildcard keys `keys` holds: the per-point cost of the key check.
pub fn wildcard_keys(keys: &[String]) -> usize {
    keys.iter().filter(|k| k.trim().ends_with('*')).count()
}

/// Points whose elevation is more than `tol` from the median of their Delaunay neighbours, with
/// that median. Points sharing an XY count as one site (the first one's elevation).
fn neighbor_outliers<'a>(points: &[&'a SurveyPoint], tol: f64) -> Vec<(&'a SurveyPoint, f64)> {
    let mut dt = DelaunayTriangulation::<Point2<f64>>::new();
    let mut site = Vec::with_capacity(points.len());
    let mut z_of: HashMap<usize, f64> = HashMap::new();
    for p in points {
        // spade rejects coordinates it cannot represent; such a point just gets no neighbours.
        let h = dt.insert(Point2::new(p.xyz[0], p.xyz[1])).ok();
        if let Some(h) = h {
            z_of.entry(h.index()).or_insert(p.xyz[2]);
        }
        site.push(h);
    }
    let mut out = Vec::new();
    for (p, h) in points.iter().zip(site) {
        let Some(h) = h else { continue };
        let mut zs: Vec<f64> = dt.vertex(h).out_edges().filter_map(|e| z_of.get(&e.to().fix().index()).copied()).collect();
        if zs.is_empty() {
            continue;
        }
        zs.sort_by(f64::total_cmp);
        let mid = zs.len() / 2;
        let median = match (zs.get(mid), mid.checked_sub(1).and_then(|i| zs.get(i))) {
            (Some(&b), Some(&a)) if zs.len().is_multiple_of(2) => (a + b) / 2.0,
            (Some(&b), _) => b,
            _ => continue,
        };
        if (p.xyz[2] - median).abs() > tol {
            out.push((*p, median));
        }
    }
    out
}

/// TIN audit thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TinAuditOptions {
    /// Triangles steeper than this (degrees from horizontal) ...
    pub max_slope_deg: f64,
    /// ... with a plan edge shorter than this are spikes.
    pub min_edge: f64,
}

impl Default for TinAuditOptions {
    fn default() -> Self {
        Self { max_slope_deg: 60.0, min_edge: 1.0 }
    }
}

/// Audit EG Surface: checks a TIN, its outer boundary and the breaklines it was built from.
pub fn audit_tin(tin: &Tin, boundary: Option<&Region>, breaklines: &[Vec<[f64; 3]>], opts: &TinAuditOptions) -> Audit {
    let mut issues = Sink::default();
    if tin.triangles.is_empty() {
        issues.push(TIN_EMPTY, Severity::Error, "the surface has no triangles".into(), None, vec![]);
    }
    if !boundary.is_some_and(Region::is_valid) {
        let message = if boundary.is_some() { "the outer boundary is not a valid ring" } else { "no outer boundary was given for the surface" };
        issues.push(TIN_NO_BOUNDARY, Severity::Warning, message.into(), None, vec![]);
    }

    for (i, t) in tin.triangles.iter().enumerate() {
        let refs = vec![i.to_string()];
        let v = t.map(|k| tin.vertices.get(k as usize).copied());
        let [Some(a), Some(b), Some(c)] = v else {
            issues.push(TIN_BAD_TRIANGLE, Severity::Error, format!("triangle {i} names a missing vertex"), None, refs);
            continue;
        };
        let at = Some([(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]);
        // Use the same relative degeneracy tolerance as surface creation and strict export.
        let (area2, degenerate) = signed_area2(a, b, c);
        let bad = if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            Some("repeats a vertex")
        } else if ![a, b, c].iter().flatten().all(|v| v.is_finite()) {
            Some("has a non-finite vertex")
        } else if degenerate || !area2.is_finite() {
            Some("has no plan area")
        } else {
            None
        };
        if let Some(why) = bad {
            let at = at.filter(|p| p.iter().all(|v| v.is_finite()));
            issues.push(TIN_BAD_TRIANGLE, Severity::Error, format!("triangle {i} {why}"), at, refs);
            continue;
        }
        // Slope of the triangle's plane: atan(|horizontal normal| / |vertical normal|).
        let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], area2];
        let slope = n[0].hypot(n[1]).atan2(n[2].abs()).to_degrees();
        let edge = [(a, b), (b, c), (c, a)].iter().map(|(p, q)| (q[0] - p[0]).hypot(q[1] - p[1])).fold(f64::INFINITY, f64::min);
        if slope > opts.max_slope_deg && edge < opts.min_edge {
            let msg = format!("triangle {i} slopes {slope:.1}° with a {edge:.3} plan edge");
            issues.push(TIN_SPIKE, Severity::Warning, msg, at, refs);
        }
    }

    let segments: Vec<Segment> = breaklines
        .iter()
        .enumerate()
        .flat_map(|(line, pts)| pts.windows(2).enumerate().filter_map(move |(k, w)| Some(Segment { line, k, a: *w.first()?, b: *w.get(1)? })))
        .filter(|s| s.a.iter().chain(&s.b).all(|v| v.is_finite()))
        .collect();
    let mut inputs = BTreeMap::new();
    inputs.insert("boundary".into(), if boundary.is_some() { "given" } else { "none" }.into());
    if segments.len() > MAX_BREAKLINE_SEGMENTS {
        inputs.insert("crossing_check".into(), format!("skipped: more than {MAX_BREAKLINE_SEGMENTS} breakline segments"));
    } else {
        let found = crossings(segments.clone(), MAX_ISSUES_PER_CODE + 1);
        if found.len() > MAX_ISSUES_PER_CODE {
            inputs.insert("crossing_check".into(), format!("stopped after {MAX_ISSUES_PER_CODE} crossings"));
        }
        for (s, t, p) in found.into_iter().take(MAX_ISSUES_PER_CODE) {
            let msg = format!("breakline {} segment {} crosses breakline {} segment {} (z {:.3} vs {:.3})", s.line, s.k, t.line, t.k, p[2], p[3]);
            let refs = vec![s.line.to_string(), t.line.to_string()];
            issues.push(TIN_BREAKLINE_CROSS, Severity::Error, msg, Some([p[0], p[1], p[2]]), refs);
        }
    }
    let thresholds = BTreeMap::from([("max_slope_deg".into(), opts.max_slope_deg), ("min_edge".into(), opts.min_edge)]);
    let counts = [
        ("vertices", tin.vertices.len()),
        ("triangles", tin.triangles.len()),
        ("breaklines", breaklines.len()),
        ("breakline_segments", segments.len()),
    ];
    finish("tin", inputs, thresholds, counts, issues)
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    line: usize,
    k: usize,
    a: [f64; 3],
    b: [f64; 3],
}

/// Pairs of segments that meet away from a shared vertex, with `[x, y, z on first, z on second]`.
/// Sweeps in x so only overlapping x ranges are compared.
fn crossings(mut segs: Vec<Segment>, limit: usize) -> Vec<(Segment, Segment, [f64; 4])> {
    let lo = |s: &Segment| s.a[0].min(s.b[0]);
    segs.sort_by(|s, t| lo(s).total_cmp(&lo(t)));
    let mut out = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let hi = s.a[0].max(s.b[0]);
        for t in segs.iter().skip(i + 1).take_while(|t| lo(t) <= hi) {
            if s.line == t.line && s.k.abs_diff(t.k) <= 1 {
                continue; // neighbours on one line share a vertex by construction
            }
            let (s, t) = if (s.line, s.k) <= (t.line, t.k) { (s, t) } else { (t, s) };
            if let Some(p) = meet(s, t) {
                out.push((*s, *t, p));
                if out.len() >= limit {
                    out.sort_by_key(|(s, t, _)| (s.line, s.k, t.line, t.k));
                    return out;
                }
            }
        }
    }
    out.sort_by_key(|(s, t, _)| (s.line, s.k, t.line, t.k));
    out
}

/// Where segments `s` and `t` meet in plan, unless only at an endpoint they share.
/// shortcut: collinear overlaps are not reported; add them if real data shows them.
fn meet(s: &Segment, t: &Segment) -> Option<[f64; 4]> {
    let (d, e) = ([s.b[0] - s.a[0], s.b[1] - s.a[1]], [t.b[0] - t.a[0], t.b[1] - t.a[1]]);
    let den = d[0] * e[1] - d[1] * e[0];
    if den == 0.0 || !den.is_finite() {
        return None;
    }
    let f = [t.a[0] - s.a[0], t.a[1] - s.a[1]];
    let (u, v) = ((f[0] * e[1] - f[1] * e[0]) / den, (f[0] * d[1] - f[1] * d[0]) / den);
    if !((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)) {
        return None;
    }
    let shared = [s.a, s.b].iter().any(|p| [t.a, t.b].iter().any(|q| p[0] == q[0] && p[1] == q[1]));
    let at_end = |x: f64| x == 0.0 || x == 1.0;
    if shared && at_end(u) && at_end(v) {
        return None;
    }
    let lerp = |a: [f64; 3], b: [f64; 3], r: f64| a[2] + r * (b[2] - a[2]);
    Some([s.a[0] + u * d[0], s.a[1] + u * d[1], lerp(s.a, s.b, u), lerp(t.a, t.b, v)])
}

/// Collects issues, listing at most [`MAX_ISSUES_PER_CODE`] per code but counting them all.
#[derive(Default)]
struct Sink {
    issues: Vec<Issue>,
    per_code: BTreeMap<&'static str, usize>,
    errors: usize,
    warnings: usize,
}

impl Sink {
    fn push(&mut self, code: &'static str, severity: Severity, message: String, at: Option<[f64; 3]>, refs: Vec<String>) {
        let n = self.per_code.entry(code).or_insert(0);
        *n += 1;
        match severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
        }
        if *n <= MAX_ISSUES_PER_CODE {
            self.issues.push(Issue { code, severity, message, at, refs });
        }
    }
}

/// Builds the audit with the shared counts: `issues`, `errors`, `warnings` and one per code (all
/// true totals), plus `listed`, the issues actually returned.
fn finish<const N: usize>(
    check: &str,
    inputs: BTreeMap<String, String>,
    thresholds: BTreeMap<String, f64>,
    sizes: [(&str, usize); N],
    sink: Sink,
) -> Audit {
    let mut counts: BTreeMap<String, usize> = sizes.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
    counts.insert("issues".into(), sink.errors + sink.warnings);
    counts.insert("errors".into(), sink.errors);
    counts.insert("warnings".into(), sink.warnings);
    counts.insert("listed".into(), sink.issues.len());
    counts.extend(sink.per_code.iter().map(|(k, v)| ((*k).to_owned(), *v)));
    Audit { receipt: Receipt { check: check.into(), version: VERSION, inputs, thresholds, counts }, issues: sink.issues }
}
