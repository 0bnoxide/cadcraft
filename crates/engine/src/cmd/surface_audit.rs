//! SURFACEAUDIT (Survey Preflight and Audit EG Surface; reports only). Owner: Claude (see `docs/agents/slice-5-analysis.md`).

use cadcraft_doc::{EntityKind, Handle};
use cadcraft_surface::audit::{self, Audit, PreflightOptions, SurveyPoint, TinAuditOptions};
use cadcraft_surface::triangulate::MAX_VERTICES;
use cadcraft_surface::{Region, Tin};
use serde_json::{Value, json};

use super::curves::handle_param;
use super::{CommandSpec, bad, size_param, str_param};
use crate::{Result, Session};

const ID: &str = "surfaceaudit";
/// Cap on point × (wildcard key + extent vertex) tests in one preflight.
const MAX_WORK: usize = 200_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(ID, "Surface Audit", run).noundo().params(
        "{mode: preflight, points: [{number?, x, y, z, description?} | [x,y,z]], keys?: [code, \"PREFIX*\"], \
         elevation_min?, elevation_max?, neighbor_tolerance?, extent?: [[x,y]], points_crs?, expected_crs?} | \
         {mode: tin, surface: polyface handle, boundary?: [[x,y]], breaklines?: [[[x,y,z]]], max_slope?: 60 (degrees), min_edge?: 1} \
         → {receipt: {check, version, inputs, thresholds, counts}, issues: [{code, severity, message, at, refs}]}; never edits the drawing",
    )]
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let (mut a, inputs) = match str_param(p, "mode") {
        Some("preflight") => {
            let (points, opts) = (points(p)?, preflight_options(p)?);
            // Each point is tested against every wildcard key and every extent edge.
            let per_point = audit::wildcard_keys(opts.keys.as_deref().unwrap_or(&[])) + opts.extent.as_ref().map_or(0, |r| r.outer.len());
            if points.len().checked_mul(per_point).is_none_or(|w| w > MAX_WORK) {
                return Err(bad(ID, format!("too much work: points × (wildcard keys + extent vertices) must be at most {MAX_WORK}")));
            }
            (audit::preflight(&points, &opts), vec![])
        }
        Some("tin") => {
            let handle = handle_param(p, "surface").ok_or_else(|| bad(ID, "`surface` must be a TIN polyface handle"))?;
            let tin = audit_surface(s, handle)?;
            let boundary = ring(p, "boundary")?.map(|outer| Region { outer, holes: vec![] });
            let lines = list(p, "breaklines")?;
            // Check the combined size before converting anything, so hostile input can't allocate past the cap.
            let total = lines.iter().try_fold(0usize, |n, l| n.checked_add(l.as_array().map_or(0, Vec::len)));
            if total.is_none_or(|n| n > MAX_VERTICES) {
                return Err(bad(ID, format!("too many breakline vertices (the limit is {MAX_VERTICES})")));
            }
            let breaklines = lines
                .iter()
                .map(|l| l.as_array().ok_or_else(|| bad(ID, "`breaklines` must be an array of point arrays"))?.iter().map(|v| xyz(v, 3)).collect())
                .collect::<Result<Vec<Vec<_>>>>()?;
            let d = TinAuditOptions::default();
            let opts = TinAuditOptions {
                max_slope_deg: size_param(ID, p, "max_slope", false)?.unwrap_or(d.max_slope_deg),
                min_edge: size_param(ID, p, "min_edge", false)?.unwrap_or(d.min_edge),
            };
            (audit::audit_tin(&tin, boundary.as_ref(), &breaklines, &opts), vec![("surface", handle.hex())])
        }
        _ => return Err(bad(ID, "`mode` must be \"preflight\" or \"tin\"")),
    };
    for (k, v) in inputs {
        a.receipt.inputs.insert(k.into(), v);
    }
    Ok(to_json(&a))
}

/// Keep damaged faces for the audit to report. The strict conversion used by export and
/// geometry commands must not run here: it rejects exactly the defects this check diagnoses.
fn audit_surface(s: &Session, handle: Handle) -> Result<Tin> {
    let drawing = s.doc()?;
    let entity = drawing.entity(handle).ok_or_else(|| bad(ID, format!("no object has handle {}", handle.hex())))?;
    let EntityKind::PolyfaceMesh(mesh) = &entity.kind else {
        return Err(bad(ID, format!("object {} is not a polyface mesh (a TIN surface)", handle.hex())));
    };
    // A hostile drawing must not make this reporting-only command allocate unbounded copies.
    if mesh.vertices.len() > MAX_VERTICES || mesh.faces.len() > MAX_VERTICES {
        return Err(bad(ID, format!("surface audit accepts at most {MAX_VERTICES} vertices and faces")));
    }
    let capacity = mesh.faces.len().checked_mul(2).ok_or_else(|| bad(ID, "the surface has too many faces"))?;
    let vertices = mesh.vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
    let mut triangles = Vec::with_capacity(capacity);
    for face in &mesh.faces {
        // Zero terminates a face; missing corners stay out of range so a short face is reported
        // once rather than dropped. The vertex cap guarantees this sentinel cannot be valid.
        let mut corners = [u32::MAX; 4];
        for (corner, index) in corners.iter_mut().zip(face.iter().take_while(|&&i| i != 0)) {
            *corner = index.unsigned_abs().checked_sub(1).unwrap_or(u32::MAX);
        }
        let [a, b, c, d] = corners;
        triangles.push([a, b, c]);
        if face.iter().all(|&i| i != 0) {
            triangles.push([a, c, d]);
        }
    }
    Ok(Tin { vertices, triangles })
}

fn to_json(a: &Audit) -> Value {
    let r = &a.receipt;
    let issues: Vec<Value> = a
        .issues
        .iter()
        .map(|i| json!({ "code": i.code, "severity": i.severity.as_str(), "message": i.message, "at": i.at, "refs": i.refs }))
        .collect();
    json!({
        "receipt": { "check": r.check, "version": r.version, "inputs": r.inputs, "thresholds": r.thresholds, "counts": r.counts },
        "issues": issues,
    })
}

/// The array at `key` (absent or null: empty), capped so hostile input can't run away.
fn list<'a>(p: &'a Value, key: &str) -> Result<&'a [Value]> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) if a.len() <= MAX_VERTICES => Ok(a),
        Some(_) => Err(bad(ID, format!("`{key}` must be an array of at most {MAX_VERTICES} items"))),
    }
}

/// `[x, y, z]` (`n` = 3) or `[x, y]` (`n` = 2, z = 0) of finite numbers.
fn xyz(v: &Value, n: usize) -> Result<[f64; 3]> {
    let a = v.as_array().filter(|a| a.len() == n);
    let c = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).filter(|f| f.is_finite());
    match (c(0), c(1), if n == 2 { Some(0.0) } else { c(2) }) {
        (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
        _ => Err(bad(ID, format!("{v} is not a point of {n} finite numbers"))),
    }
}

/// An optional `[[x, y]]` ring of at least three points.
fn ring(p: &Value, key: &str) -> Result<Option<Vec<[f64; 2]>>> {
    if p.get(key).is_none_or(Value::is_null) {
        return Ok(None);
    }
    let r: Vec<[f64; 2]> = list(p, key)?.iter().map(|v| xyz(v, 2).map(|q| [q[0], q[1]])).collect::<Result<_>>()?;
    if r.len() < 3 {
        return Err(bad(ID, format!("`{key}` needs at least three [x, y] points")));
    }
    Ok(Some(r))
}

/// Survey points as objects (`number` defaults to the 1-based position) or bare `[x, y, z]`.
/// Coordinates may be any number, including ones the audit flags; missing ones are an error.
fn points(p: &Value) -> Result<Vec<SurveyPoint>> {
    if p.get("points").is_none() {
        return Err(bad(ID, "preflight needs `points`"));
    }
    list(p, "points")?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let number = (i + 1).to_string();
            if v.is_array() {
                return Ok(SurveyPoint { number, xyz: xyz(v, 3)?, description: String::new() });
            }
            let c = |k: &str| v.get(k).and_then(Value::as_f64).ok_or_else(|| bad(ID, format!("point {} needs numeric `{k}`", i + 1)));
            let number = match v.get("number") {
                Some(Value::String(n)) => n.clone(),
                Some(n @ Value::Number(_)) => n.to_string(),
                _ => number,
            };
            let description = str_param(v, "description").unwrap_or("").to_owned();
            Ok(SurveyPoint { number, xyz: [c("x")?, c("y")?, c("z")?], description })
        })
        .collect()
}

fn preflight_options(p: &Value) -> Result<PreflightOptions> {
    let keys = match p.get("keys") {
        None | Some(Value::Null) => None,
        Some(_) => Some(
            list(p, "keys")?.iter().map(|k| k.as_str().map(str::to_owned).ok_or_else(|| bad(ID, "`keys` must be strings"))).collect::<Result<_>>()?,
        ),
    };
    let num = |k: &str| match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|f| f.is_finite()).map(Some).ok_or_else(|| bad(ID, format!("`{k}` must be a finite number"))),
    };
    let elevation_band = match (num("elevation_min")?, num("elevation_max")?) {
        (None, None) => None,
        (lo, hi) => Some((lo.unwrap_or(f64::NEG_INFINITY), hi.unwrap_or(f64::INFINITY))),
    };
    let extent = ring(p, "extent")?.map(|outer| Region { outer, holes: vec![] });
    Ok(PreflightOptions {
        keys,
        elevation_band,
        neighbor_tolerance: size_param(ID, p, "neighbor_tolerance", true)?,
        extent,
        points_crs: str_param(p, "points_crs").map(str::to_owned),
        expected_crs: str_param(p, "expected_crs").map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn tincreate_then_tin_audit_reports_without_editing() {
        let mut s = Session::new();
        let made = s.execute("tincreate", &json!({"points": [[0,0,0],[10,0,0],[10,10,0],[0,10,0],[5,5,0]]})).unwrap();
        let before = s.doc().unwrap().clone();
        let out = s
            .execute("surfaceaudit", &json!({"mode": "tin", "surface": made["handle"], "breaklines": [[[0,0,1],[10,10,1]],[[0,10,2],[10,0,2]]]}))
            .unwrap();
        assert_eq!(s.doc().unwrap(), &before, "the audit never edits the drawing");
        let codes: Vec<&str> = out["issues"].as_array().unwrap().iter().filter_map(|i| i["code"].as_str()).collect();
        assert_eq!(codes, ["TIN-NO-BOUNDARY", "TIN-BREAKLINE-CROSS"]);
        assert_eq!(out["receipt"]["check"], "tin");
        assert_eq!(out["receipt"]["inputs"]["surface"], made["handle"]);
        assert_eq!(out["receipt"]["counts"]["triangles"], 4);
        assert_eq!(out["issues"][1]["at"], json!([5.0, 5.0, 1.0]));
        assert_eq!(out["issues"][1]["severity"], "error");
    }

    #[test]
    fn preflight_reads_objects_and_arrays() {
        let mut s = Session::new();
        let out = s
            .execute(
                "surfaceaudit",
                &json!({"mode": "preflight", "keys": ["EG"], "elevation_max": 50,
                        "points": [{"number": 1, "x": 0, "y": 0, "z": 10, "description": "EG shot"}, {"number": "1", "x": 1, "y": 0, "z": 99}, [2, 2, 1]]}),
            )
            .unwrap();
        let codes: Vec<&str> = out["issues"].as_array().unwrap().iter().filter_map(|i| i["code"].as_str()).collect();
        assert_eq!(codes, ["PF-DUP-NUMBER", "PF-CODE-UNKEYED", "PF-ELEV-BAND", "PF-CODE-UNKEYED"]);
        assert_eq!(out["receipt"]["counts"]["points"], 3);
        assert_eq!(out["receipt"]["thresholds"]["elevation_max"], 50.0);
        assert!(out["receipt"]["thresholds"].get("elevation_min").is_none(), "an open band side is left out, not null");
    }

    #[test]
    fn bad_params_are_errors_not_panics() {
        let mut s = Session::new();
        for p in [
            json!({}),
            json!({"mode": "nope"}),
            json!({"mode": "preflight"}),
            json!({"mode": "preflight", "points": [[1, 2]]}),
            json!({"mode": "preflight", "points": [], "extent": [[0, 0], [1, 1]]}),
            json!({"mode": "preflight", "points": [], "neighbor_tolerance": -1}),
            json!({"mode": "preflight", "points": vec![json!([0, 0, 0]); 1001], "keys": vec![json!("A*"); 200_000]}),
            json!({"mode": "tin"}),
            json!({"mode": "tin", "surface": "FFFF"}),
        ] {
            assert!(s.execute("surfaceaudit", &p).is_err(), "{p}");
        }
    }
}
