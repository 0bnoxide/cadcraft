//! BORINGIMPORT: import a boring-log package and draw boring locations. Owner: Claude (see
//! `docs/agents/slice-6-subsurface.md`).
//!
//! Stateless by agreement: nothing is stored in the drawing, and every subsurface command
//! reads its package from `path` on each run.

use cadcraft_doc::{Common, EntityKind, HAlign, Point, Text, VAlign};
use cadcraft_geom::Vec3;
use cadcraft_subsurface::{BoringSet, Finding, Severity};
use serde_json::{Value, json};

use super::blocks::valid_block_name;
use super::{CommandSpec, bad, size_param, str_param};
use crate::{Result, Session};

const IMPORT: &str = "boringimport";
/// Default layer for boring locations (agreed with Codex by message, 21:40Z).
pub(crate) const LAYER: &str = "BORING-LOC";
const TEXT_HEIGHT: f64 = 0.25;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(IMPORT, "Import Boring Logs", run_import).params(
        "{path: folder of CSV files or one XLSX/CSV workbook, layer?: (default BORING-LOC), text_height?: (default 0.25)} → \
         {borings, handles, unlocated: [boring_id], errors, warnings, findings: [{severity, code, message, boring_id, source: {sheet, row}}]}",
    )]
}

/// Findings as JSON, shared with the other subsurface commands.
pub(crate) fn findings_json(f: &[Finding]) -> Value {
    let sev = |s: Severity| match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    f.iter()
        .map(|f| {
            json!({
                "severity": sev(f.severity),
                "code": f.code,
                "message": f.message,
                "boring_id": f.boring_id,
                "source": f.source.as_ref().map(|(sheet, row)| json!({ "sheet": sheet, "row": row })),
            })
        })
        .collect()
}

/// The package at `path`, for every subsurface command (stateless by agreement).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load(path: &str) -> Result<(BoringSet, Vec<Finding>)> {
    cadcraft_subsurface::import::load(std::path::Path::new(path)).map_err(|e| bad(IMPORT, e.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn load(path: &str) -> Result<(BoringSet, Vec<Finding>)> {
    Err(bad(IMPORT, format!("{path}: paths are not available on the web")))
}

fn run_import(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad(IMPORT, "`path` (a package folder or workbook) is required"))?;
    let layer = str_param(p, "layer").unwrap_or(LAYER);
    if !valid_block_name(layer) {
        return Err(bad(IMPORT, format!("`layer`: '{layer}' is not a valid layer name")));
    }
    let height = size_param(IMPORT, p, "text_height", false)?.unwrap_or(TEXT_HEIGHT);
    let (set, findings) = load(path)?;

    let mut handles = Vec::new();
    let mut unlocated = Vec::new();
    // Stage the drawing so a failed add can't leave half the borings behind.
    let mut drawing = s.doc()?.clone();
    let space = s.space();
    drawing.ensure_layer(layer);
    let common = || Common { layer: layer.into(), ..Common::default() };
    for b in &set.borings {
        let (Some(x), Some(y)) = (b.easting, b.northing) else {
            unlocated.push(b.boring_id.clone());
            continue;
        };
        let p = Vec3 { x, y, z: b.ground_elevation.unwrap_or(0.0) };
        handles.push(drawing.add(&space, common(), EntityKind::Point(Point { p, angle: 0.0 }))?);
        let label = Text {
            insert: Vec3 { x: x + height * 0.5, y: y + height * 0.5, z: p.z },
            align_pt: None,
            height,
            value: b.boring_id.clone(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::default(),
            valign: VAlign::default(),
        };
        handles.push(drawing.add(&space, common(), EntityKind::Text(label))?);
    }
    *s.doc_mut()? = drawing;
    let count = |sev| findings.iter().filter(|f| f.severity == sev).count();
    let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
    s.echo(format!("Imported {} borings ({} drawn): {errors} errors, {warnings} warnings.", set.borings.len(), handles.len() / 2));
    let hex: Vec<String> = handles.iter().map(|h| h.hex()).collect();
    s.set_selection(handles);
    Ok(json!({
        "borings": set.borings.len(),
        "handles": hex,
        "unlocated": unlocated,
        "errors": errors,
        "warnings": warnings,
        "findings": findings_json(&findings),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Space;

    fn package(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cadcraft-boringimport-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("boring_locations.csv"), "BoringID,Northing,Easting,GroundElevation_ft\nB-1,200,100,50\nB-2,,,\n").unwrap();
        std::fs::write(d.join("lithology.csv"), "BoringID,TopDepth_ft,BottomDepth_ft,USCS\nB-1,0,5,SM\nB-1,4,9,CL\n").unwrap();
        d
    }

    #[test]
    fn boringimport_draws_located_borings_and_reports_findings() {
        let dir = package("ok");
        let mut s = Session::new();
        let v = s.execute(IMPORT, &json!({ "path": dir.to_string_lossy() })).unwrap();
        assert_eq!(v["borings"], 2);
        assert_eq!(v["unlocated"], json!(["B-2"]));
        assert_eq!(v["handles"].as_array().unwrap().len(), 2);
        assert_eq!(v["errors"], 1, "{v}");
        let codes: Vec<&str> = v["findings"].as_array().unwrap().iter().filter_map(|f| f["code"].as_str()).collect();
        assert!(codes.contains(&"overlapping_intervals") && codes.contains(&"missing_coordinates"), "{codes:?}");
        let d = s.doc().unwrap();
        let ents: Vec<_> = d.space(&Space::Model).unwrap().iter().filter(|e| e.common.layer == LAYER).collect();
        assert!(ents.iter().any(|e| matches!(&e.kind, EntityKind::Point(p) if p.p == Vec3 { x: 100.0, y: 200.0, z: 50.0 })));
        assert!(ents.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "B-1")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn boringimport_refuses_bad_params() {
        let dir = package("bad");
        let mut s = Session::new();
        let path = dir.to_string_lossy().to_string();
        assert!(s.execute(IMPORT, &json!({})).is_err());
        assert!(s.execute(IMPORT, &json!({ "path": "no/such/package" })).is_err());
        assert!(s.execute(IMPORT, &json!({ "path": path, "layer": "a/b" })).is_err());
        assert!(s.execute(IMPORT, &json!({ "path": path, "text_height": -1 })).is_err());
        let v = s.execute(IMPORT, &json!({ "path": path, "layer": "WELLS" })).unwrap();
        assert_eq!(v["handles"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
