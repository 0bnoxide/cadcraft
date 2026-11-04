//! RTK control check against published benchmarks. Reference: AutoGIS
//! `core/envmon/rtk_control_check.py`. Owner: Claude (see `docs/agents/slice-1-survey.md`).
//!
//! A control network is only as good as its worst point, so the check passes only when every
//! point is within both tolerances; the RMSE is reported for information.

use std::io::{self, Read, Write};

use crate::rtk::{ParseError, cell, err, number, read_rows};
use crate::{Finding, Severity};

/// Default per-point tolerances, in the input's units.
pub const DEFAULT_HORIZONTAL_TOLERANCE: f64 = 0.05;
pub const DEFAULT_VERTICAL_TOLERANCE: f64 = 0.10;

/// The numeric CSV columns, in [`ControlPoint`] field order.
const NUMBER_COLUMNS: [&str; 6] = ["published_x", "published_y", "published_z", "surveyed_x", "surveyed_y", "surveyed_z"];

/// One control point: its published (known) and surveyed position.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlPoint {
    pub control_id: String,
    pub published_x: f64,
    pub published_y: f64,
    pub published_z: f64,
    pub surveyed_x: f64,
    pub surveyed_y: f64,
    pub surveyed_z: f64,
}

/// One point's residuals (surveyed minus published).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlResult {
    pub control_id: String,
    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_z: f64,
    pub horizontal_distance: f64,
    pub vertical_distance: f64,
    pub point_pass: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSummary {
    pub n_points: usize,
    pub rmse_horizontal: f64,
    pub rmse_vertical: f64,
    pub horizontal_tolerance: f64,
    pub vertical_tolerance: f64,
    pub n_pass: usize,
    pub n_fail: usize,
    /// True only when there are points and every one passes.
    pub overall_pass: bool,
    pub results: Vec<ControlResult>,
}

/// Reads a control-check CSV whose header row names `control_id`, `published_x`, `published_y`,
/// `published_z`, `surveyed_x`, `surveyed_y` and `surveyed_z` (in any order). A bad record is an
/// error naming its line.
pub fn read_control_csv(r: impl Read) -> Result<Vec<ControlPoint>, ParseError> {
    let rows = read_rows(r)?;
    let Some(((_, header), data)) = rows.split_first() else {
        return Ok(Vec::new());
    };
    let col = |name: &str| header.iter().rposition(|h| h.trim() == name);
    let id_col = col("control_id");
    let num_cols = NUMBER_COLUMNS.map(col);
    let mut out = Vec::with_capacity(data.len());
    for (line, row) in data {
        let bad = |name: &str, why: String| err(format!("line {line}: invalid control-check record ({name}: {why})"));
        let Some(id) = id_col.map(|i| cell(row, i)) else {
            return bad("control_id", "missing column".into());
        };
        let mut nums = [0.0; 6];
        for ((slot, name), i) in nums.iter_mut().zip(NUMBER_COLUMNS).zip(num_cols) {
            let Some(raw) = i.map(|i| cell(row, i)) else {
                return bad(name, "missing column".into());
            };
            let Some(v) = number(raw) else {
                return bad(name, format!("'{raw}' is not a number"));
            };
            *slot = v;
        }
        let [published_x, published_y, published_z, surveyed_x, surveyed_y, surveyed_z] = nums;
        out.push(ControlPoint { control_id: id.into(), published_x, published_y, published_z, surveyed_x, surveyed_y, surveyed_z });
    }
    Ok(out)
}

/// Scores each point against the tolerances. Findings: an error per failing point, then an info
/// summary; or one error when there are no points.
pub fn evaluate(points: &[ControlPoint], horizontal_tolerance: f64, vertical_tolerance: f64) -> (ControlSummary, Vec<Finding>) {
    let mut findings = Vec::new();
    let mut results = Vec::with_capacity(points.len());
    for p in points {
        let (dx, dy, dz) = (p.surveyed_x - p.published_x, p.surveyed_y - p.published_y, p.surveyed_z - p.published_z);
        let (h, v) = (dx.hypot(dy), dz.abs());
        let point_pass = h <= horizontal_tolerance && v <= vertical_tolerance;
        if !point_pass {
            findings.push(Finding::error(format!(
                "control_point_fails_tolerance: Control point '{}': horizontal={h:.4} (tol {horizontal_tolerance}), \
                 vertical={v:.4} (tol {vertical_tolerance})",
                p.control_id
            )));
        }
        results.push(ControlResult {
            control_id: p.control_id.clone(),
            delta_x: dx,
            delta_y: dy,
            delta_z: dz,
            horizontal_distance: h,
            vertical_distance: v,
            point_pass,
        });
    }
    let n = results.len();
    let rms = |f: fn(&ControlResult) -> f64| {
        if n == 0 { 0.0 } else { (results.iter().map(|r| f(r).powi(2)).sum::<f64>() / n as f64).sqrt() }
    };
    let (rmse_h, rmse_v) = (rms(|r| r.horizontal_distance), rms(|r| r.vertical_distance));
    let n_pass = results.iter().filter(|r| r.point_pass).count();
    let n_fail = n - n_pass;
    let overall_pass = n > 0 && n_fail == 0;
    if n == 0 {
        findings.push(Finding::error("no_control_points: No control-check records provided."));
    } else {
        findings.push(Finding {
            severity: Severity::Info,
            message: format!(
                "control_check_complete: RTK control check: {n} point(s), {n_pass} pass / {n_fail} fail, RMSE \
                 horizontal={rmse_h:.4}, RMSE vertical={rmse_v:.4}: {}",
                if overall_pass { "PASS" } else { "FAIL" }
            ),
        });
    }
    let summary = ControlSummary {
        n_points: n,
        rmse_horizontal: rmse_h,
        rmse_vertical: rmse_v,
        horizontal_tolerance,
        vertical_tolerance,
        n_pass,
        n_fail,
        overall_pass,
        results,
    };
    (summary, findings)
}

/// A CSV field, quoted when it holds a comma, quote or line break.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.into() }
}

/// Writes the per-point results as CSV with a header row.
pub fn write_results_csv(summary: &ControlSummary, mut w: impl Write) -> io::Result<()> {
    writeln!(w, "control_id,delta_x,delta_y,delta_z,horizontal_distance,vertical_distance,point_pass")?;
    for r in &summary.results {
        writeln!(
            w,
            "{},{},{},{},{},{},{}",
            field(&r.control_id),
            r.delta_x,
            r.delta_y,
            r.delta_z,
            r.horizontal_distance,
            r.vertical_distance,
            if r.point_pass { "True" } else { "False" }
        )?;
    }
    Ok(())
}
