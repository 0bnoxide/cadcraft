//! Survey point commands (RTK import, control check). Owner: Claude (see
//! `docs/agents/slice-1-survey.md`).
//!
//! RTKIMPORT reads an RTK CSV, checks it and draws each shot as a POINT (easting X, northing Y,
//! elevation Z, in the file's own units). CONTROLCHECK scores control shots against published
//! benchmarks and changes nothing in the drawing.

use std::io::{Cursor, Read};

use cadcraft_doc::{EntityKind, Point};
use cadcraft_geom::Vec3;
use cadcraft_survey::{Finding, Severity, control, rtk};
use serde_json::{Value, json};

use super::{CommandSpec, bad, size_param, str_param};

use crate::{Result, Session};

const RTK: &str = "rtkimport";
const CONTROL: &str = "controlcheck";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(RTK, "Import RTK Points", run_rtk).params(
            "{path | text (CSV), format?: auto|pnezd|penzd, extra_columns?: [name], columns?: {point_id, northing, \
             easting, elevation, feature_code, description, hrms, vrms, fix_type, collected_at, operator: header}, \
             hrms?, vrms?} → {points, handles, flagged: [{id, flags}], findings: [{severity, message}]}",
        ),
        CommandSpec::new(CONTROL, "RTK Control Check", run_control).params(
            "{path | text (CSV), horizontal_tolerance?, vertical_tolerance?, output?: results CSV path} → \
             {overall_pass, n_points, n_pass, n_fail, rmse_horizontal, rmse_vertical, results, findings}",
        ),
    ]
}

/// The CSV named by `path` (streamed from disk) or given as `text`.
fn source(cmd: &str, p: &Value) -> Result<Box<dyn Read>> {
    if let Some(path) = str_param(p, "path") {
        #[cfg(not(target_arch = "wasm32"))]
        return std::fs::File::open(path).map(|f| Box::new(f) as Box<dyn Read>).map_err(|e| bad(cmd, format!("{path}: {e}")));
        #[cfg(target_arch = "wasm32")]
        return Err(bad(cmd, format!("{path}: paths are not available on the web; pass `text`")));
    }
    let text = str_param(p, "text").ok_or_else(|| bad(cmd, "`path` or `text` (CSV) is required"))?;
    Ok(Box::new(Cursor::new(text.as_bytes().to_vec())))
}

fn findings_json(f: &[Finding]) -> Value {
    let sev = |s: Severity| match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    f.iter().map(|f| json!({ "severity": sev(f.severity), "message": f.message })).collect()
}

fn options(p: &Value) -> Result<rtk::ImportOptions> {
    let order = match str_param(p, "format").map(str::to_ascii_lowercase).as_deref() {
        None | Some("auto") => rtk::CoordOrder::Auto,
        Some("pnezd") => rtk::CoordOrder::Pnezd,
        Some("penzd") => rtk::CoordOrder::Penzd,
        Some(other) => return Err(bad(RTK, format!("`format` must be auto, pnezd or penzd, not '{other}'"))),
    };
    let mut o = rtk::ImportOptions { order, ..Default::default() };
    if let Some(v) = p.get("extra_columns").filter(|v| !v.is_null()) {
        let names = v.as_array().and_then(|a| a.iter().map(|n| n.as_str().map(String::from)).collect::<Option<Vec<_>>>());
        o.extra_columns = Some(names.ok_or_else(|| bad(RTK, "`extra_columns` must be a list of names"))?);
    }
    if let Some(map) = p.get("columns").and_then(Value::as_object) {
        let c = &mut o.columns;
        for (key, v) in map {
            let name = v.as_str().ok_or_else(|| bad(RTK, format!("`columns.{key}` must be a header name")))?.to_string();
            let slot = match key.as_str() {
                "point_id" => &mut c.point_id,
                "northing" => &mut c.northing,
                "easting" => &mut c.easting,
                "elevation" => &mut c.elevation,
                "feature_code" => &mut c.feature_code,
                "description" => &mut c.description,
                "hrms" => &mut c.hrms,
                "vrms" => &mut c.vrms,
                "fix_type" => &mut c.fix_type,
                "collected_at" => &mut c.collected_at,
                "operator" => &mut c.operator,
                _ => return Err(bad(RTK, format!("unknown column `{key}`"))),
            };
            *slot = name;
        }
    }
    Ok(o)
}

fn run_rtk(s: &mut Session, p: &Value) -> Result<Value> {
    let opts = options(p)?;
    let hrms = size_param(RTK, p, "hrms", true)?.unwrap_or(rtk::DEFAULT_HRMS);
    let vrms = size_param(RTK, p, "vrms", true)?.unwrap_or(rtk::DEFAULT_VRMS);
    let import = rtk::parse_rtk_csv(source(RTK, p)?, &opts).map_err(|e| bad(RTK, e.to_string()))?;
    let mut findings = import.findings;
    findings.extend(rtk::validate(&import.points, hrms, vrms));
    let mut handles = Vec::with_capacity(import.points.len());
    let mut flagged = Vec::new();
    for pt in &import.points {
        let sp = &pt.point;
        let p = Vec3 { x: sp.easting, y: sp.northing, z: sp.elevation };
        handles.push(s.add_entity(EntityKind::Point(Point { p, angle: 0.0 }))?);
        let flags = rtk::qa_flags(pt, hrms, vrms);
        if !flags.is_empty() {
            flagged.push(json!({ "id": sp.id, "flags": flags }));
        }
    }
    let hex: Vec<String> = handles.iter().map(|h| h.hex()).collect();
    s.set_selection(handles);
    Ok(json!({
        "points": hex.len(),
        "handles": hex,
        "flagged": flagged,
        "findings": findings_json(&findings),
    }))
}

fn run_control(_: &mut Session, p: &Value) -> Result<Value> {
    let h_tol = size_param(CONTROL, p, "horizontal_tolerance", true)?.unwrap_or(control::DEFAULT_HORIZONTAL_TOLERANCE);
    let v_tol = size_param(CONTROL, p, "vertical_tolerance", true)?.unwrap_or(control::DEFAULT_VERTICAL_TOLERANCE);
    let points = control::read_control_csv(source(CONTROL, p)?).map_err(|e| bad(CONTROL, e.to_string()))?;
    let (sum, findings) = control::evaluate(&points, h_tol, v_tol);
    if let Some(out) = str_param(p, "output") {
        #[cfg(not(target_arch = "wasm32"))]
        std::fs::File::create(out)
            .and_then(|f| control::write_results_csv(&sum, std::io::BufWriter::new(f)))
            .map_err(|e| bad(CONTROL, format!("{out}: {e}")))?;
        #[cfg(target_arch = "wasm32")]
        return Err(bad(CONTROL, format!("{out}: paths are not available on the web")));
    }
    let results: Vec<Value> = sum
        .results
        .iter()
        .map(|r| {
            json!({
                "control_id": r.control_id,
                "delta_x": r.delta_x,
                "delta_y": r.delta_y,
                "delta_z": r.delta_z,
                "horizontal_distance": r.horizontal_distance,
                "vertical_distance": r.vertical_distance,
                "point_pass": r.point_pass,
            })
        })
        .collect();
    Ok(json!({
        "overall_pass": sum.overall_pass,
        "n_points": sum.n_points,
        "n_pass": sum.n_pass,
        "n_fail": sum.n_fail,
        "rmse_horizontal": sum.rmse_horizontal,
        "rmse_vertical": sum.rmse_vertical,
        "results": results,
        "findings": findings_json(&findings),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Space;

    const HEADERED: &str = "PointID,Northing,Easting,Elevation_ft,FixType,HRMS_ft\n\
        MW-01,4527893.12,293847.55,512.34,RTK_FIXED,0.01\n\
        INV001,4527800.00,293870.00,510.00,AUTONOMOUS,0.50\n";

    #[test]
    fn rtkimport_draws_points_and_reports_flags() {
        let mut s = Session::new();
        let v = s.execute(RTK, &json!({ "text": HEADERED })).unwrap();
        assert_eq!(v["points"], 2);
        assert_eq!(v["flagged"][0]["id"], "INV001");
        assert_eq!(v["flagged"][0]["flags"], json!(["hrms_exceeds_threshold", "fix_type_not_rtk"]));
        let last = v["findings"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["severity"], "info");
        let d = s.doc().unwrap();
        let first = d.space(&Space::Model).unwrap().iter().find_map(|e| match &e.kind {
            EntityKind::Point(p) => Some(p.p),
            _ => None,
        });
        assert_eq!(first, Some(Vec3 { x: 293847.55, y: 4527893.12, z: 512.34 }));
    }

    #[test]
    fn rtkimport_refuses_bad_input_and_options() {
        let mut s = Session::new();
        let ambiguous = "1,555000.12,545000.34,510.50,PT-1\n";
        let e = s.execute(RTK, &json!({ "text": ambiguous })).unwrap_err();
        assert!(e.to_string().contains("`format`"), "{e}");
        let v = s.execute(RTK, &json!({ "text": ambiguous, "format": "PENZD" })).unwrap();
        assert_eq!(v["points"], 1);
        assert!(s.execute(RTK, &json!({ "text": ambiguous, "format": "nez" })).is_err());
        assert!(s.execute(RTK, &json!({ "text": HEADERED, "columns": { "bogus": "x" } })).is_err());
        assert!(s.execute(RTK, &json!({ "text": HEADERED, "hrms": -1.0 })).is_err());
        assert!(s.execute(RTK, &json!({ "path": "no/such/file.csv" })).is_err());
        assert!(s.execute(RTK, &json!({})).is_err());
    }

    #[test]
    fn rtkimport_custom_columns() {
        let mut s = Session::new();
        let text = "ID,Y,X,Z\nMW-01,4527893.12,293847.55,512.34\n";
        let cols = json!({ "point_id": "ID", "northing": "Y", "easting": "X", "elevation": "Z" });
        let v = s.execute(RTK, &json!({ "text": text, "columns": cols })).unwrap();
        assert_eq!(v["points"], 1);
    }

    #[test]
    fn controlcheck_scores_without_touching_the_drawing() {
        let mut s = Session::new();
        let text = "control_id,published_x,published_y,published_z,surveyed_x,surveyed_y,surveyed_z\n\
            CP-1,1000,2000,500,1000,2000,500\n\
            CP-2,1000,2000,500,1000.5,2000,500\n";
        let v = s.execute(CONTROL, &json!({ "text": text })).unwrap();
        assert_eq!((v["overall_pass"].as_bool(), v["n_pass"].as_u64(), v["n_fail"].as_u64()), (Some(false), Some(1), Some(1)));
        assert_eq!(v["results"][1]["point_pass"], false);
        assert_eq!(v["findings"][0]["severity"], "error");
        let n = s.doc().unwrap().space(&Space::Model).unwrap().iter().count();
        assert_eq!(n, 0);
        let e = s.execute(CONTROL, &json!({ "text": "control_id,published_x\nCP-1,x\n" })).unwrap_err();
        assert!(e.to_string().contains("line 2"), "{e}");
    }

    #[test]
    fn controlcheck_writes_results_csv() {
        let dir = std::env::temp_dir().join(format!("cadcraft-controlcheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("results.csv");
        let text = "control_id,published_x,published_y,published_z,surveyed_x,surveyed_y,surveyed_z\nCP-1,0,0,0,0,0,0\n";
        let mut s = Session::new();
        s.execute(CONTROL, &json!({ "text": text, "output": out.to_str().unwrap() })).unwrap();
        let csv = std::fs::read_to_string(&out).unwrap();
        assert!(csv.starts_with("control_id,") && csv.contains("CP-1"), "{csv}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
