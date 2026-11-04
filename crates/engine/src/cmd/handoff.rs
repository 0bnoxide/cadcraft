//! HANDOFFIMPORT: draw the TIN surface of an AutoGIS LandXML handoff package (contract v1) as a
//! polyface mesh. The package is validated first; an invalid one is refused with its first issue.
//! Easting is X, northing Y and elevation Z, in the package's own units and coordinate system.

use std::collections::HashMap;
use std::io::Cursor;

use cadcraft_doc::{EntityKind, PolyfaceMesh};
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

use super::file::base64_decode;
use super::{CommandSpec, bad, str_param};

use crate::{Result, Session};

const ID: &str = "handoffimport";

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(ID, "Import Handoff Surface", run).params(
        "{path | data: base64} (AutoGIS LandXML handoff ZIP, contract v1) → {handle, surface, points, faces, epsg, warnings: [\"CODE message\"]}",
    )]
}

fn read(p: &Value) -> Result<Vec<u8>> {
    if let Some(path) = str_param(p, "path") {
        #[cfg(not(target_arch = "wasm32"))]
        return std::fs::read(path).map_err(|e| bad(ID, format!("{path}: {e}")));
        #[cfg(target_arch = "wasm32")]
        return Err(bad(ID, format!("{path}: paths are not available on the web; pass `data`")));
    }
    let data = str_param(p, "data").ok_or_else(|| bad(ID, "`path` or `data` (base64) is required"))?;
    base64_decode(data).ok_or_else(|| bad(ID, "invalid base64"))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let bytes = read(p)?;
    let report = cadcraft_handoff::validate(Cursor::new(bytes), true).map_err(|e| bad(ID, e.to_string()))?;
    if report.status == cadcraft_handoff::Status::Invalid {
        let first = report.issues.first().map(|i| format!("[{}] {}", i.code, i.message)).unwrap_or_default();
        return Err(bad(ID, format!("the package is invalid: {first}")));
    }
    let tin = report.surface.ok_or_else(|| bad(ID, "the package has no surface"))?;
    let mut index = HashMap::with_capacity(tin.points.len());
    let mut vertices = Vec::with_capacity(tin.points.len());
    for (i, pt) in tin.points.iter().enumerate() {
        let one_based = i32::try_from(i + 1).map_err(|_| bad(ID, "the surface has too many points"))?;
        index.insert(pt.id, one_based);
        vertices.push(Vec3 { x: pt.easting, y: pt.northing, z: pt.elevation });
    }
    let faces = tin
        .faces
        .iter()
        .map(|f| match (index.get(&f[0]), index.get(&f[1]), index.get(&f[2])) {
            (Some(&a), Some(&b), Some(&c)) => Ok([a, b, c, 0]),
            _ => Err(bad(ID, "a face names an unknown point")),
        })
        .collect::<Result<Vec<_>>>()?;
    let (points, face_count) = (vertices.len(), faces.len());
    let handle = s.add_entity(EntityKind::PolyfaceMesh(PolyfaceMesh { vertices, faces }))?;
    s.set_selection(vec![handle]);
    let warnings: Vec<String> = report.issues.iter().map(|i| format!("{} {}", i.code, i.message)).collect();
    Ok(json!({
        "handle": handle.hex(),
        "surface": tin.name,
        "points": points,
        "faces": face_count,
        "epsg": tin.epsg_code,
        "warnings": warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_doc::Space;

    fn fixture(name: &str) -> String {
        format!("{}/../handoff/tests/fixtures/v1/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn imports_the_tin_as_a_polyface_mesh() {
        let mut s = Session::new();
        let v = s.execute(ID, &json!({ "path": fixture("valid/known-vertical-datum.zip") })).unwrap();
        assert_eq!(
            (v["surface"].as_str(), v["points"].as_u64(), v["faces"].as_u64(), v["epsg"].as_i64()),
            (Some("Existing Ground"), Some(3), Some(1), Some(26913))
        );
        assert_eq!(v["warnings"].as_array().map(Vec::len), Some(0));
        let d = s.doc().unwrap();
        let mesh = d.space(&Space::Model).unwrap().iter().find_map(|e| match &e.kind {
            EntityKind::PolyfaceMesh(m) => Some(m.clone()),
            _ => None,
        });
        let mesh = mesh.unwrap();
        assert_eq!(mesh.faces, vec![[1, 2, 3, 0]]);
        assert_eq!(mesh.vertices[1], Vec3 { x: 10.0, y: 0.0, z: 101.0 });
    }

    #[test]
    fn warns_on_unknown_datum_and_refuses_invalid_packages() {
        let mut s = Session::new();
        let v = s.execute(ID, &json!({ "path": fixture("valid/unknown-vertical-datum.zip") })).unwrap();
        assert!(v["warnings"][0].as_str().unwrap().starts_with("WRN001"));
        let e = s.execute(ID, &json!({ "path": fixture("invalid/xml-degenerate-face.zip") })).unwrap_err();
        assert!(e.to_string().contains("XML012"), "{e}");
        assert!(s.execute(ID, &json!({})).is_err());
    }
}
