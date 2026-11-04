//! Level loop and point export commands. Owner: Codex (see `docs/agents/slice-1-survey.md`).

use cadcraft_survey::export::{self, XmlOptions};
use cadcraft_survey::level_loop::{self, LoopOptions};
use cadcraft_survey::{Finding, Severity};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::{CommandSpec, always, bad};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("levelloop", "Process Level Loop", run_level).enabled(always).noundo().params(
            "{csv: text | path, benchmark_id, known_elevation, run_id?, site_id?, survey_date?, tolerance?, well_ids?: [id]} → {run, observations, update_plan, findings}; input units, equal correction per setup, no database writes",
        ),
        CommandSpec::new("pointexport", "Export Survey Points", run_export).enabled(always).noundo().params(
            "{csv: text | path, crs, mapping?: {feature_code: layer, default: layer}, start_number?: 1, landxml?: false, geojson?: false, linear_unit?: meter|foot|USSurveyFoot, date?: YYYY-MM-DD, time?: HH:MM:SS, output_dir?: new directory} → {files, manifest, findings, written}; XML requires caller-supplied date/time and units; no coordinate conversion",
        ),
    ]
}

fn optional_text<'a>(p: &'a Value, id: &str, key: &str) -> Result<Option<&'a str>> {
    p.get(key).map(|v| v.as_str().ok_or_else(|| bad(id, format!("`{key}` must be a string")))).transpose()
}

fn required_text<'a>(p: &'a Value, id: &str, key: &str) -> Result<&'a str> {
    optional_text(p, id, key)?.filter(|s| !s.trim().is_empty()).ok_or_else(|| bad(id, format!("`{key}` is required")))
}

fn input_csv(p: &Value, id: &str) -> Result<Vec<u8>> {
    match (optional_text(p, id, "csv")?, optional_text(p, id, "path")?) {
        (Some(csv), None) if csv.len() <= 64 * 1024 * 1024 => Ok(csv.as_bytes().to_vec()),
        (None, Some(path)) => {
            #[cfg(not(target_arch = "wasm32"))]
            return crate::capped_read::read_capped(std::path::Path::new(path), 64 * 1024 * 1024).map_err(|e| bad(id, format!("{path}: {e}")));
            #[cfg(target_arch = "wasm32")]
            return Err(bad(id, format!("{path}: paths are unavailable on the web; pass `csv`")));
        }
        _ => Err(bad(id, "pass exactly one of `csv` (maximum 64 MiB) or `path`")),
    }
}

fn findings_json(findings: &[Finding]) -> Value {
    findings.iter().map(|f| json!({"severity": match f.severity { Severity::Info => "Info", Severity::Warning => "Warning", Severity::Error => "Error" }, "message": f.message})).collect()
}

fn run_level(_: &mut Session, p: &Value) -> Result<Value> {
    let id = "levelloop";
    let bytes = input_csv(p, id)?;
    let rows = level_loop::read_observations(bytes.as_slice()).map_err(|e| bad(id, e.to_string()))?;
    let known = p
        .get("known_elevation")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad(id, "`known_elevation` must be a finite number"))?;
    let tolerance =
        p.get("tolerance").filter(|v| !v.is_null()).map(|v| v.as_f64().ok_or_else(|| bad(id, "`tolerance` must be a number"))).transpose()?;
    let mut well_ids = BTreeSet::new();
    if let Some(ids) = p.get("well_ids") {
        for value in ids
            .as_array()
            .filter(|ids| ids.len() <= export::MAX_ROWS)
            .ok_or_else(|| bad(id, "`well_ids` must be an array of at most 1,000,000 strings"))?
        {
            well_ids.insert(value.as_str().ok_or_else(|| bad(id, "well ids must be strings"))?.to_string());
        }
    }
    let options = LoopOptions {
        run_id: optional_text(p, id, "run_id")?.unwrap_or_else(|| rows.first().map(|r| r.run_id.as_str()).unwrap_or_default()).into(),
        site_id: optional_text(p, id, "site_id")?.unwrap_or_default().into(),
        survey_date: optional_text(p, id, "survey_date")?.unwrap_or_default().into(),
        benchmark_id: required_text(p, id, "benchmark_id")?.into(),
        known_elevation: known,
        tolerance,
    };
    let result = level_loop::process_level_loop(&rows, &options).map_err(|e| bad(id, e.to_string()))?;
    let plan = level_loop::select_elevations_for_update(&result.run, &result.observations, &well_ids).map_err(|e| bad(id, e.to_string()))?;
    let observations: Vec<_> = result.observations.iter().map(|r| json!({"run_id":r.run_id,"setup_id":r.setup_id,"point_id":r.point_id,"backsight":r.backsight,"foresight":r.foresight,"intermediate_sight":r.intermediate_sight,"hi":r.hi,"elevation":r.elevation})).collect();
    let run = result.run;
    Ok(json!({
        "run": {"run_id":run.run_id,"site_id":run.site_id,"survey_date":run.survey_date,"benchmark_id":run.benchmark_id,"known_elevation":run.known_elevation,"misclosure":run.misclosure,"closure_tolerance":run.closure_tolerance,"adjusted":run.adjusted,"has_errors":run.has_errors},
        "observations": observations,
        "update_plan": {"run_id":plan.run_id,"survey_date":plan.survey_date,"blocked":plan.block_reason.is_some(),"block_reason":plan.block_reason,"updates":plan.updates,"skipped":plan.skipped},
        "findings": findings_json(&result.findings),
    }))
}

fn flag(p: &Value, key: &str) -> Result<bool> {
    p.get(key).map(|v| v.as_bool().ok_or_else(|| bad("pointexport", format!("`{key}` must be boolean")))).transpose().map(|v| v.unwrap_or(false))
}

fn run_export(_: &mut Session, p: &Value) -> Result<Value> {
    let id = "pointexport";
    let bytes = input_csv(p, id)?;
    let crs = required_text(p, id, "crs")?;
    let output_dir = optional_text(p, id, "output_dir")?;
    let write_xml = flag(p, "landxml")?;
    let write_geojson = flag(p, "geojson")?;
    let start = p
        .get("start_number")
        .map(|v| v.as_u64().filter(|n| *n > 0).ok_or_else(|| bad(id, "`start_number` must be a positive integer")))
        .transpose()?
        .unwrap_or(1);
    let mut mapping = BTreeMap::new();
    if let Some(map) = p.get("mapping") {
        for (code, layer) in map.as_object().ok_or_else(|| bad(id, "`mapping` must be an object of feature code to layer name"))? {
            mapping.insert(code.clone(), layer.as_str().ok_or_else(|| bad(id, "mapping layer names must be strings"))?.to_string());
        }
    }
    let xml = if write_xml {
        Some(XmlOptions {
            crs,
            linear_unit: required_text(p, id, "linear_unit")?,
            date: required_text(p, id, "date")?,
            time: required_text(p, id, "time")?,
        })
    } else {
        None
    };
    if let Some(options) = &xml {
        export::landxml(&[], options).map_err(|e| bad(id, e.to_string()))?;
    }
    let (points, mut findings) = export::read_points_csv(bytes.as_slice()).map_err(|e| bad(id, e.to_string()))?;
    let (layers, layer_findings) = export::group_points_by_layer(&points, &mapping).map_err(|e| bad(id, e.to_string()))?;
    findings.extend(layer_findings);
    let mut files = BTreeMap::<String, String>::new();
    let mut manifest = Vec::new();
    for (layer, group) in &layers {
        if layer.eq_ignore_ascii_case("points_pnezd") || layer.len() > 200 {
            return Err(bad(id, "layer name collides with points_pnezd or exceeds 200 UTF-8 bytes"));
        }
        let csv_name = format!("{layer}.csv");
        files.insert(csv_name.clone(), export::layer_csv(group).map_err(|e| bad(id, e.to_string()))?);
        if let Some(options) = &xml {
            files.insert(format!("{layer}.xml"), export::landxml(group, options).map_err(|e| bad(id, e.to_string()))?);
        }
        if write_geojson {
            let features: Vec<_> = group.iter().map(|point| json!({"type":"Feature","geometry":{"type":"Point","coordinates":[point.easting,point.northing,point.elevation]},"properties":{"point_id":point.id,"feature_code":point.feature_code,"description":point.description}})).collect();
            files.insert(format!("{layer}.geojson"), json!({"type":"FeatureCollection","features":features}).to_string());
        }
        let codes = group
            .iter()
            .filter(|p| !p.feature_code.is_empty())
            .map(|p| p.feature_code.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(";");
        manifest.push(json!({"layer_name":layer,"feature_codes":codes,"point_count":group.len(),"output_path":csv_name}));
    }
    if points.is_empty() {
        findings.push(Finding { severity: Severity::Info, message: "no_points: nothing exported".into() });
    } else {
        files.insert("points_pnezd.csv".into(), export::pnezd_csv(&points, start).map_err(|e| bad(id, e.to_string()))?);
        if let Some(options) = &xml {
            let mut numbered = points.clone();
            for (i, point) in numbered.iter_mut().enumerate() {
                let number = start.checked_add(i as u64).ok_or_else(|| bad(id, "point number overflow"))?;
                if point.description.is_empty() {
                    point.description = point.id.clone();
                }
                point.id = number.to_string();
                point.feature_code.clear();
            }
            files.insert("points_pnezd.xml".into(), export::landxml(&numbered, options).map_err(|e| bad(id, e.to_string()))?);
        }
        files.insert(
            "projection_note.txt".into(),
            format!("Coordinate system: {crs}\nCoordinates pass through unprojected in the input units; assign this coordinate system on import.\n"),
        );
        files.insert("manifest.json".into(), json!(manifest).to_string());
        findings.push(Finding {
            severity: Severity::Info,
            message: format!("export_survey_cad_complete: {} points, {} layers", points.len(), layers.len()),
        });
    }
    let written = if let Some(dir) = output_dir.filter(|_| !files.is_empty()) {
        write_package(dir, &files)?;
        true
    } else {
        false
    };
    Ok(json!({"files":files,"manifest":manifest,"findings":findings_json(&findings),"written":written}))
}

/// Generate everything before writing; a fresh output directory protects existing packages.
fn write_package(dir: &str, files: &BTreeMap<String, String>) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = std::path::Path::new(dir);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| bad("pointexport", e.to_string()))?;
        }
        std::fs::create_dir(path).map_err(|e| bad("pointexport", format!("output directory must be new: {dir}: {e}")))?;
        for (name, contents) in files {
            crate::atomic_write::write_atomic(&path.join(name), contents.as_bytes())
                .map_err(|e| bad("pointexport", format!("{dir}/{name}: {e}; the package may be incomplete")))?;
        }
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (dir, files);
        Err(bad("pointexport", "output_dir is unavailable on the web; use returned files"))
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    const LOOP: &str = "run_id,setup_id,point_id,backsight,foresight,intermediate_sight\nL1,1,BM,2,,\nL1,1,TP1,,3,\nL1,2,TP1,4,,\nL1,2,BM,,2.9,\n";
    const POINTS: &str = "location_id,x,y,z,feature_code\nMW-1,1000,2000,50,MW\nGCP-1,30,40,5,GCP\n";

    #[test]
    fn level_command_reports_adjustments_and_promotion_without_modifying_drawing() {
        let mut s = Session::new();
        let value = s.execute("levelloop", &json!({"csv": LOOP, "run_id": "L1", "site_id": "S", "survey_date": "2026-04-01", "benchmark_id": "BM", "known_elevation": 100, "tolerance": 0.5, "well_ids": ["TP1", "BM"]})).unwrap();
        assert!((value["run"]["misclosure"].as_f64().unwrap() - 0.1).abs() < 1e-9);
        assert!((value["update_plan"]["updates"]["TP1"].as_f64().unwrap() - 98.95).abs() < 1e-9);
        assert!(value["update_plan"]["updates"].get("BM").is_none());
        assert_eq!(value["update_plan"]["blocked"], false);
        assert_eq!(value["observations"].as_array().unwrap().len(), 4);
        assert!(s.execute("levelloop", &json!({"csv": LOOP, "benchmark_id": "BM", "known_elevation": 100, "tolerance": -1})).is_err());
    }

    #[test]
    fn point_command_returns_layer_csv_landxml_manifest_and_geojson() {
        let value = Session::new().execute("pointexport", &json!({"csv": POINTS, "crs": "EPSG:2256", "mapping": {"MW": "Wells"}, "landxml": true, "geojson": true, "linear_unit": "foot", "date": "2026-11-03", "time": "12:00:00", "start_number": 100})).unwrap();
        assert_eq!(value["manifest"].as_array().unwrap().len(), 2);
        assert!(value["files"]["Wells.csv"].as_str().unwrap().contains("MW-1,1000,2000,50,MW"));
        assert!(value["files"]["Wells.xml"].as_str().unwrap().contains("2000 1000 50"));
        assert!(value["files"]["points_pnezd.csv"].as_str().unwrap().contains("100,2000,1000,50,MW-1"));
        let geo: serde_json::Value = serde_json::from_str(value["files"]["Wells.geojson"].as_str().unwrap()).unwrap();
        assert_eq!(geo["features"][0]["geometry"]["coordinates"], json!([1000.0, 2000.0, 50.0]));
        assert!(value["files"]["projection_note.txt"].as_str().unwrap().contains("EPSG:2256"));
        let xml = value["files"]["points_pnezd.xml"].as_str().unwrap();
        assert!(xml.contains("name=\"100\""));
    }

    #[test]
    fn invalid_export_metadata_does_not_create_partial_package() {
        let path = std::env::temp_dir().join(format!("cadcraft-survey-invalid-{}", std::process::id()));
        for extra in [
            json!({}),
            json!({"crs": "EPSG:2256", "landxml": true}),
            json!({"crs": "NAD83", "landxml": true, "linear_unit": "foot", "date": "2026-11-03", "time": "12:00:00"}),
            json!({"crs": "EPSG:2256", "mapping": {"MW": 5}}),
        ] {
            let mut params = extra;
            params["csv"] = POINTS.into();
            params["output_dir"] = path.to_string_lossy().as_ref().into();
            assert!(Session::new().execute("pointexport", &params).is_err());
            assert!(!path.exists());
        }
    }

    #[test]
    fn empty_export_has_no_artifacts() {
        let value = Session::new().execute("pointexport", &json!({"csv": "point_id,x,y,z\n", "crs": "EPSG:2256"})).unwrap();
        assert_eq!(value["manifest"], json!([]));
        assert_eq!(value["files"], json!({}));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn point_command_reads_path_and_writes_package() {
        let path = std::env::temp_dir().join(format!("cadcraft-survey-export-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let source = path.join("input.csv");
        std::fs::write(&source, POINTS).unwrap();
        let output = path.join("out");
        let value = Session::new().execute("pointexport", &json!({"path": source, "output_dir": output, "crs": "EPSG:2256"})).unwrap();
        assert_eq!(value["written"], true);
        assert!(output.join("Miscellaneous.csv").is_file());
        assert!(output.join("manifest.json").is_file());
        std::fs::remove_dir_all(path).unwrap();
    }
}
