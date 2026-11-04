//! TINCONTOUR and TINCOMPARE. Owner: Codex (see `docs/agents/slice-2-tin.md`).

use cadcraft_doc::{Common, EntityKind, Handle, Polyline3d};
use cadcraft_geom::Vec3;
use cadcraft_surface::{compare, contour};
use serde_json::{Value, json};

use super::surface_build::tin_from_handle;
use super::{CommandSpec, bad, size_param};
use crate::{Result, Session};

const MINOR: &str = "C-TOPO-MINR";
const INDEX: &str = "C-TOPO-MAJR";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tincontour", "Create Contours", run_contour)
            .params("{surface: hex handle, interval?: 1, index_every?: 5} → {handles, count, minor, index}; zero-based levels, 3D polylines on C-TOPO-MINR / C-TOPO-MAJR"),
        CommandSpec::new("tincompare", "Compare Surfaces", run_compare)
            .noundo()
            .params("{surface: reference hex handle, points: [[x,y,z],...]} | {surface, other: target hex handle} → {differences, statistics: {min,max,mean,rms,count,outside}}; target minus reference, outside excluded, no extrapolation or coordinate conversion"),
    ]
}

fn surface(s: &Session, p: &Value, id: &str, key: &str) -> Result<cadcraft_surface::Tin> {
    let handle = p
        .get(key)
        .and_then(Value::as_str)
        .and_then(Handle::parse_hex)
        .ok_or_else(|| bad(id, format!("`{key}` must be a hex polyface mesh handle")))?;
    tin_from_handle(s.doc()?, handle)
}

fn run_contour(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "tincontour";
    let interval = size_param(id, p, "interval", false)?.unwrap_or(1.);
    let index_every = p
        .get("index_every")
        .map(|v| {
            v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| n > 0).ok_or_else(|| bad(id, "`index_every` must be a positive u32 integer"))
        })
        .transpose()?
        .unwrap_or(5);
    let tin = surface(s, p, id, "surface")?;
    let paths = contour::contours(&tin, interval, index_every).map_err(|e| bad(id, e.to_string()))?;
    let count = paths.len();
    let index = paths.iter().filter(|p| p.index).count();
    let mut handles = Vec::with_capacity(count);
    if !paths.is_empty() {
        // Stage the drawing so even a failed add cannot leave half the contour set behind.
        let mut drawing = s.doc()?.clone();
        let space = s.space();
        drawing.ensure_layer(MINOR);
        drawing.ensure_layer(INDEX);
        for path in paths {
            let common = Common { layer: if path.index { INDEX } else { MINOR }.into(), ..Common::default() };
            let points = path.points.into_iter().map(|[x, y, z]| Vec3::new(x, y, z)).collect();
            let handle = drawing.add(&space, common, EntityKind::Polyline3d(Polyline3d { points, closed: path.closed }))?;
            handles.push(handle.hex());
        }
        *s.doc_mut()? = drawing;
    }
    s.echo(format!("Created {count} contours ({index} index, {} minor).", count - index));
    Ok(json!({"handles":handles,"count":count,"minor":count-index,"index":index}))
}

fn run_compare(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "tincompare";
    let tin = surface(s, p, id, "surface")?;
    let report = match (p.get("points"), p.get("other")) {
        (Some(value), None) => {
            let rows = value
                .as_array()
                .filter(|a| a.len() <= 1_000_000)
                .ok_or_else(|| bad(id, "`points` must be an array of at most 1 million XYZ points"))?;
            let points = rows
                .iter()
                .map(|row| {
                    let numbers = row.as_array().filter(|r| r.len() == 3).ok_or_else(|| bad(id, "each point must be [x,y,z]"))?;
                    let mut point = [0.; 3];
                    for (out, value) in point.iter_mut().zip(numbers) {
                        *out = value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(id, "point coordinates must be finite numbers"))?;
                    }
                    Ok(point)
                })
                .collect::<Result<Vec<_>>>()?;
            compare::compare_points(&tin, &points)
        }
        (None, Some(_)) => compare::compare_vertices(&tin, &surface(s, p, id, "other")?),
        _ => return Err(bad(id, "pass exactly one of `points` or `other`")),
    }
    .map_err(|e| bad(id, e.to_string()))?;
    let stats = report.statistics;
    s.echo(format!("Compared {} points; {} outside. Differences are target minus reference in drawing units.", stats.count, stats.outside));
    Ok(
        json!({"differences":report.differences,"statistics":{"min":stats.min,"max":stats.max,"mean":stats.mean,"rms":stats.rms,"count":stats.count,"outside":stats.outside}}),
    )
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use cadcraft_doc::{EntityKind, Handle, PolyfaceMesh};
    use cadcraft_geom::Vec3;
    use serde_json::json;

    fn surface(s: &mut Session, offset: f64) -> Handle {
        s.add_entity(EntityKind::PolyfaceMesh(PolyfaceMesh {
            vertices: vec![Vec3::new(0., 0., offset), Vec3::new(2., 0., 2. + offset), Vec3::new(2., 2., 2. + offset), Vec3::new(0., 2., offset)],
            faces: vec![[1, 2, 3, 0], [1, 3, 4, 0]],
        }))
        .unwrap()
    }

    #[test]
    fn draws_3d_contours_on_agreed_layers_and_undo_restores_drawing() {
        let mut s = Session::new();
        let h = surface(&mut s, 0.);
        let original = s.doc().unwrap().clone();
        let v = s.execute("tincontour", &json!({"surface":h.hex(),"interval":0.5,"index_every":2})).unwrap();
        let handles = v["handles"].as_array().unwrap();
        assert!(!handles.is_empty());
        let doc = s.doc().unwrap();
        assert!(doc.layer("C-TOPO-MINR").is_some());
        assert!(doc.layer("C-TOPO-MAJR").is_some());
        let mut minor = 0;
        let mut major = 0;
        for handle in handles {
            let entity = doc.entity(Handle::parse_hex(handle.as_str().unwrap()).unwrap()).unwrap();
            let EntityKind::Polyline3d(line) = &entity.kind else { panic!("not a 3D polyline") };
            assert!(line.points.len() >= 2);
            let z = line.points[0].z;
            assert!(line.points.iter().all(|p| p.z == z));
            if z == 0.5 || z == 1.5 {
                assert_eq!(entity.common.layer, "C-TOPO-MINR");
                minor += 1;
            } else {
                assert_eq!(entity.common.layer, "C-TOPO-MAJR");
                major += 1;
            }
        }
        assert!(minor > 0 && major > 0);
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &original);
    }

    #[test]
    fn reports_comparison_from_points_or_other_surface_without_editing_drawing() {
        let mut s = Session::new();
        let h = surface(&mut s, 0.);
        let other = surface(&mut s, 3.);
        let before = s.doc().unwrap().clone();
        let v = s.execute("tincompare", &json!({"surface":h.hex(),"points":[[1,1,5],[20,20,50]]})).unwrap();
        assert_eq!(v["statistics"]["mean"], 4.);
        assert_eq!(v["statistics"]["outside"], 1);
        assert_eq!(v["differences"], json!([4., null]));
        let v = s.execute("tincompare", &json!({"surface":h.hex(),"other":other.hex()})).unwrap();
        assert_eq!(v["statistics"]["count"], 4);
        assert_eq!(v["statistics"]["mean"], 3.);
        assert_eq!(s.doc().unwrap(), &before);
    }

    #[test]
    fn bad_parameters_fail_before_layers_or_geometry_are_created() {
        let mut s = Session::new();
        let h = surface(&mut s, 0.);
        let before = s.doc().unwrap().clone();
        for p in [
            json!({}),
            json!({"surface": "garbage"}),
            json!({"surface":h.hex(),"interval":0}),
            json!({"surface":h.hex(),"interval":"bad"}),
            json!({"surface":h.hex(),"index_every":0}),
            json!({"surface":h.hex(),"index_every":1.5}),
            json!({"surface":h.hex(),"index_every":4294967296u64}),
        ] {
            assert!(s.execute("tincontour", &p).is_err());
            assert_eq!(s.doc().unwrap(), &before);
        }
        for p in [
            json!({"surface":h.hex()}),
            json!({"surface":h.hex(),"points":[[1,2]]}),
            json!({"surface":h.hex(),"points":[],"other":h.hex()}),
            json!({"surface":h.hex(),"other":"FFFF"}),
        ] {
            assert!(s.execute("tincompare", &p).is_err());
            assert_eq!(s.doc().unwrap(), &before);
        }
    }

    #[test]
    fn preserves_existing_contour_layer_properties() {
        let mut s = Session::new();
        let h = surface(&mut s, 0.);
        s.doc_mut().unwrap().ensure_layer(super::MINOR);
        let layer = s.doc_mut().unwrap().layers.iter_mut().find(|l| l.name == super::MINOR).unwrap();
        layer.color = cadcraft_color::Color::Index(2);
        layer.description = "survey team settings".into();
        let before = layer.clone();
        s.execute("tincontour", &json!({"surface":h.hex(),"interval":0.5})).unwrap();
        assert_eq!(s.doc().unwrap().layer(super::MINOR).unwrap(), &before);
    }
}
