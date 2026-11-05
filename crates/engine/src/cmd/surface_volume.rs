//! TINVOLUME and GWCONTOUR. Owner: Codex (see `docs/agents/slice-5-analysis.md`).

use super::surface_build::tin_from_handle;
use super::{CommandSpec, bad, point_req, size_param};
use crate::{Result, Session};
use cadcraft_doc::{Common, EntityKind, Handle, Polyline3d};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_surface::{Region, contour, interpolate, volume};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tinvolume","Surface Volumes",run_volume).params("{base: hex, comparison: hex, boundary?: closed straight polyline handle, regions?: [{outer:[[x,y]],holes?:[[[x,y]]]}], cell_size?: 1, at?: [x,y]} → midpoint-grid report and receipt; at draws Table; drawing units, net=fill-cut, outside skipped; overlapping regions counted separately"),
        CommandSpec::new("gwcontour","Groundwater Contours",run_groundwater).params("{wells:[[x,y,water_level]], method?: idw|natural_neighbor, power?:2, radius?:number, max_neighbors?:12, cell_size?:extent/50, interval?:1, index_every?:5} → draft contours and downhill arrow; no extrapolation"),
    ]
}

fn handle(p: &Value, key: &str, id: &str) -> Result<Handle> {
    p.get(key).and_then(Value::as_str).and_then(Handle::parse_hex).ok_or_else(|| bad(id, format!("`{key}` must be a hex handle")))
}
fn point<const N: usize>(v: &Value, id: &str) -> Result<[f64; N]> {
    let row = v.as_array().filter(|r| r.len() == N).ok_or_else(|| bad(id, format!("expected {N} coordinates")))?;
    let mut p = [0.; N];
    for (out, v) in p.iter_mut().zip(row) {
        *out = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(id, "coordinates must be finite"))?;
    }
    Ok(p)
}
fn ring(v: &Value, id: &str) -> Result<Vec<[f64; 2]>> {
    v.as_array().filter(|r| r.len() <= 10_000).ok_or_else(|| bad(id, "ring must have at most 10000 vertices"))?.iter().map(|v| point(v, id)).collect()
}
fn quantities(q: &volume::Quantities) -> Value {
    json!({"requested_area":q.requested_area,"covered_area":q.covered_area,"skipped_area":q.skipped_area,"cut":q.cut,"fill":q.fill,"net":q.net,"samples":q.samples})
}
fn run_volume(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "tinvolume";
    let base_handle = handle(p, "base", id)?;
    let other_handle = handle(p, "comparison", id)?;
    let base = tin_from_handle(s.doc()?, base_handle)?;
    let other = tin_from_handle(s.doc()?, other_handle)?;
    let spacing = size_param(id, p, "cell_size", false)?.unwrap_or(1.);
    if p.get("boundary").is_some() && p.get("regions").is_some() {
        return Err(bad(id, "use boundary or regions, not both"));
    }
    let regions = if let Some(value) = p.get("regions") {
        value
            .as_array()
            .filter(|r| !r.is_empty() && r.len() <= 1000)
            .ok_or_else(|| bad(id, "regions must contain 1..1000 regions"))?
            .iter()
            .map(|r| {
                let outer = ring(r.get("outer").ok_or_else(|| bad(id, "region needs outer ring"))?, id)?;
                let holes = match r.get("holes") {
                    None => Vec::new(),
                    Some(v) => v
                        .as_array()
                        .filter(|h| h.len() <= 1000)
                        .ok_or_else(|| bad(id, "invalid holes"))?
                        .iter()
                        .map(|h| ring(h, id))
                        .collect::<Result<_>>()?,
                };
                Ok(Region { outer, holes })
            })
            .collect::<Result<Vec<_>>>()?
    } else if p.get("boundary").is_some() {
        let h = handle(p, "boundary", id)?;
        let e = s.doc()?.entity(h).ok_or_else(|| bad(id, "boundary not found"))?;
        let outer = match &e.kind {
            EntityKind::LwPolyline(line) if line.closed && line.vertices.len() <= 10_000 && line.vertices.iter().all(|v| v.bulge == 0.) => {
                line.vertices.iter().map(|v| [v.p.x, v.p.y]).collect()
            }
            EntityKind::Polyline3d(line) if line.closed && line.points.len() <= 10_000 => line.points.iter().map(|v| [v.x, v.y]).collect(),
            _ => return Err(bad(id, "boundary must be a closed straight polyline with at most 10000 vertices")),
        };
        vec![Region { outer, holes: Vec::new() }]
    } else {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for &[x, y, _] in &base.vertices {
            lo[0] = lo[0].min(x);
            lo[1] = lo[1].min(y);
            hi[0] = hi[0].max(x);
            hi[1] = hi[1].max(y);
        }
        vec![Region { outer: vec![lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]], holes: Vec::new() }]
    };
    let at = if p.get("at").is_some() { Some(point_req(id, p, "at")?) } else { None };
    let report = volume::calculate(&base, &other, &regions, spacing).map_err(|e| bad(id, e.to_string()))?;
    let columns = vec!["Region", "Covered area", "Skipped area", "Cut", "Fill", "Net"];
    let mut cells: Vec<Vec<String>> = report
        .regions
        .iter()
        .enumerate()
        .map(|(i, r)| {
            vec![
                (i + 1).to_string(),
                r.covered_area.to_string(),
                r.skipped_area.to_string(),
                r.cut.to_string(),
                r.fill.to_string(),
                r.net.to_string(),
            ]
        })
        .collect();
    let total = &report.total;
    cells.push(vec![
        "Total".into(),
        total.covered_area.to_string(),
        total.skipped_area.to_string(),
        total.cut.to_string(),
        total.fill.to_string(),
        total.net.to_string(),
    ]);
    let table_handle = if let Some(at) = at {
        let table = super::table::make_table(
            &Default::default(),
            at,
            cells.len(),
            6,
            None,
            Some(4.),
            Some("Surface Volumes (midpoint grid)"),
            Some(columns.iter().map(|s| s.to_string()).collect()),
            &cells,
        );
        Some(s.add_entity(EntityKind::Table(table))?.hex())
    } else {
        None
    };
    Ok(
        json!({"receipt":{"check":"volume","version":1,"inputs":{"base":base_handle.hex(),"comparison":other_handle.hex(),"method":"midpoint_grid","boundary":p.get("boundary").cloned().unwrap_or(Value::Null).to_string(),"regions":regions.len().to_string()},"thresholds":{"cell_size":spacing},"counts":{"regions":regions.len(),"samples":total.samples,"issues":0,"errors":0,"warnings":0}},"method":"midpoint_grid","approximation":"Boundary and cut/fill zero-crossing cells use midpoint sampling; refine cell_size to assess convergence.","cell_size":spacing,"regions":report.regions.iter().map(quantities).collect::<Vec<_>>(),"total":quantities(total),"columns":columns,"rows":cells,"table_handle":table_handle}),
    )
}

fn run_groundwater(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "gwcontour";
    let wells = p
        .get("wells")
        .and_then(Value::as_array)
        .filter(|r| r.len() <= 100_000)
        .ok_or_else(|| bad(id, "wells must be at most 100000 XYZ rows"))?
        .iter()
        .map(|v| point::<3>(v, id))
        .collect::<Result<Vec<_>>>()?;
    let method = match p.get("method") {
        None => interpolate::Method::Idw,
        Some(Value::String(m)) if m == "idw" => interpolate::Method::Idw,
        Some(Value::String(m)) if m == "natural_neighbor" => interpolate::Method::NaturalNeighbor,
        _ => return Err(bad(id, "method must be idw or natural_neighbor")),
    };
    let max_neighbors = match p.get("max_neighbors") {
        None => 12,
        Some(v) => {
            v.as_u64().and_then(|n| usize::try_from(n).ok()).filter(|&n| n > 0 && n <= 100_000).ok_or_else(|| bad(id, "invalid max_neighbors"))?
        }
    };
    let opts = interpolate::Options {
        method,
        power: size_param(id, p, "power", false)?.unwrap_or(2.),
        radius: size_param(id, p, "radius", false)?,
        max_neighbors,
    };
    let gradient = interpolate::fit_plane_gradient(&wells)
        .map_err(|e| bad(id, e.to_string()))?
        .ok_or_else(|| bad(id, "at least three distinct noncollinear wells with nonzero plane gradient required"))?;
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut center = [0.; 3];
    for &[x, y, z] in &wells {
        lo[0] = lo[0].min(x);
        lo[1] = lo[1].min(y);
        hi[0] = hi[0].max(x);
        hi[1] = hi[1].max(y);
        for (c, v) in center.iter_mut().zip([x, y, z]) {
            *c += v / wells.len() as f64;
        }
    }
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let spacing = size_param(id, p, "cell_size", false)?.unwrap_or(extent / 50.);
    let interval = size_param(id, p, "interval", false)?.unwrap_or(1.);
    let index_every = match p.get("index_every") {
        None => 5,
        Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| n > 0).ok_or_else(|| bad(id, "invalid index_every"))?,
    };
    let grid = interpolate::grid(&wells, [lo, hi], spacing, &opts).map_err(|e| bad(id, e.to_string()))?;
    let tin = interpolate::grid_tin(&grid).map_err(|e| bad(id, e.to_string()))?;
    if tin.triangles.is_empty() {
        return Err(bad(id, "no covered grid cells; use a smaller cell_size"));
    }
    let paths = contour::contours(&tin, interval, index_every).map_err(|e| bad(id, e.to_string()))?;
    let count = paths.len();
    let mut handles = Vec::new();
    let mut drawing = s.doc()?.clone();
    let space = s.space();
    drawing.ensure_layer("C-GW-DRAFT");
    drawing.ensure_layer("C-GW-FLOW-DRAFT");
    for path in paths {
        let points = path.points.into_iter().map(|[x, y, z]| Vec3::new(x, y, z)).collect();
        handles.push(
            drawing
                .add(
                    &space,
                    Common { layer: "C-GW-DRAFT".into(), ..Default::default() },
                    EntityKind::Polyline3d(Polyline3d { points, closed: path.closed }),
                )?
                .hex(),
        );
    }
    let length = extent * 0.15;
    let [gx, gy] = gradient.downhill;
    let mag = gradient.magnitude;
    let start = Vec3::new(center[0], center[1], center[2]);
    let end = Vec3::new(start.x + length * gx / mag, start.y + length * gy / mag, start.z);
    let side = Vec2::new(-gy / mag, gx / mag) * length * 0.08;
    let back = Vec3::new(end.x - length * 0.2 * gx / mag, end.y - length * 0.2 * gy / mag, end.z);
    let points = vec![start, end, Vec3::new(back.x + side.x, back.y + side.y, back.z), end, Vec3::new(back.x - side.x, back.y - side.y, back.z)];
    let arrow = drawing.add(
        &space,
        Common { layer: "C-GW-FLOW-DRAFT".into(), ..Default::default() },
        EntityKind::Polyline3d(Polyline3d { points, closed: false }),
    )?;
    *s.doc_mut()? = drawing;
    s.echo(format!("Created {count} DRAFT groundwater contours; review before use."));
    Ok(
        json!({"handles":handles,"contours":count,"flow_arrow":arrow.hex(),"review_status":"DRAFT","gradient":{"downhill":gradient.downhill,"magnitude":gradient.magnitude,"azimuth":gradient.azimuth,"r_squared":gradient.r_squared},"cell_size":spacing,"grid_nodes":grid.values.len()}),
    )
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;
    #[test]
    fn created_tins_report_volumes_and_optional_table() {
        let mut s = Session::new();
        let base = s.execute("tincreate", &json!({"points":[[0,0,0],[10,0,10],[10,10,30],[0,10,20]]})).unwrap();
        let other = s.execute("tincreate", &json!({"points":[[0,0,3],[10,0,13],[10,10,33],[0,10,23]]})).unwrap();
        let p = json!({"base":base["handle"],"comparison":other["handle"],"cell_size":1});
        let before = s.doc().unwrap().clone();
        let v = s.execute("tinvolume", &p).unwrap();
        assert!((v["total"]["fill"].as_f64().unwrap() - 300.).abs() < 1e-9);
        assert_eq!(v["receipt"]["check"], "volume");
        assert_eq!(s.doc().unwrap(), &before);
        let mut draw = p.clone();
        draw["at"] = json!([20, 20]);
        let v = s.execute("tinvolume", &draw).unwrap();
        assert!(v["table_handle"].is_string());
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &before);
    }
    #[test]
    fn groundwater_draws_draft_contours_and_downhill_arrow_atomically() {
        let mut s = Session::new();
        let before = s.doc().unwrap().clone();
        let v = s.execute("gwcontour", &json!({"wells":[[0,0,10],[10,0,20],[10,10,40],[0,10,30]],"cell_size":2,"interval":5})).unwrap();
        assert!(v["contours"].as_u64().unwrap() > 0);
        assert!(v["flow_arrow"].is_string());
        assert_eq!(v["review_status"], "DRAFT");
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &before);
        assert!(s.execute("gwcontour", &json!({"wells":[[0,0,0],[1,1,1],[2,2,2]]})).is_err());
        assert_eq!(s.doc().unwrap(), &before);
    }
}
