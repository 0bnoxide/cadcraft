//! TINCREATE and TINEXPORT, plus the shared TIN <-> polyface mesh conversion whose API was agreed
//! with Codex by Paseo message (2e005518 / 5048efaf). Owner: Claude (see
//! `docs/agents/slice-2-tin.md`).

use cadcraft_doc::{Drawing, EntityKind, Handle, PolyfaceMesh, Space};
use cadcraft_geom::Vec3;
use cadcraft_surface::Tin;
use cadcraft_surface::landxml::{self, ExportOptions, LinearUnit};
use cadcraft_surface::triangulate::{self, BuildOptions, signed_area2};
use serde_json::{Value, json};

use super::curves::handle_param;
use super::{CommandSpec, bad, size_param, str_param, targets};
use crate::{Result, Session};

/// Error prefix for the conversion, which serves several commands.
const CONV: &str = "tin";
const CREATE: &str = "tincreate";
const EXPORT: &str = "tinexport";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(CREATE, "Create TIN Surface", run_create).params(
            "{points?: [[x,y,z]] (else the POINTs in handles/selection, else every POINT in model space), \
             breaklines?: [[[x,y,z]]], boundary?: [[x,y]], max_edge?} → \
             {handle, vertices, triangles, duplicates, skipped_segments, dropped_triangles}",
        ),
        CommandSpec::new(EXPORT, "Export TIN to LandXML", run_export).params(
            "{surface?: handle (else the one selected polyface), epsg, unit?: meter|foot|USSurveyFoot (default meter), \
             name?: surface name (default \"Surface\"), path?: .xml file (else the document is returned as `xml`)} → \
             {points, faces, path | xml}",
        ),
    ]
}

/// `[x, y, z]` (or `[x, y]` with `two_d`) of finite numbers.
fn xyz(cmd: &str, key: &str, v: &Value, two_d: bool) -> Result<[f64; 3]> {
    let a = v.as_array().filter(|a| a.len() == 3 || (two_d && a.len() == 2));
    let n = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).filter(|f| f.is_finite());
    match (n(0), n(1), if two_d { Some(n(2).unwrap_or(0.0)) } else { n(2) }) {
        (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
        _ => Err(bad(cmd, format!("`{key}`: {v} is not a point [x, y{}] of finite numbers", if two_d { "" } else { ", z" }))),
    }
}

/// The array at `key` (absent or null: empty), capped so hostile input can't run away.
fn list<'a>(cmd: &str, p: &'a Value, key: &str) -> Result<&'a [Value]> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) if a.len() <= triangulate::MAX_VERTICES => Ok(a),
        Some(_) => Err(bad(cmd, format!("`{key}` must be an array of at most {} items", triangulate::MAX_VERTICES))),
    }
}

fn run_create(s: &mut Session, p: &Value) -> Result<Value> {
    let mut points = list(CREATE, p, "points")?.iter().map(|v| xyz(CREATE, "points", v, false)).collect::<Result<Vec<_>>>()?;
    if p.get("points").is_none() {
        let picked = targets(s, p)?;
        let d = s.doc()?;
        let from = |e: &cadcraft_doc::Entity| match &e.kind {
            EntityKind::Point(pt) => Some([pt.p.x, pt.p.y, pt.p.z]),
            _ => None,
        };
        points = if picked.is_empty() {
            d.space(&Space::Model).map(|m| m.iter().filter_map(|e| from(e)).collect()).unwrap_or_default()
        } else {
            picked.iter().filter_map(|&h| d.entity(h)).filter_map(|e| from(e)).collect()
        };
    }
    let breaklines = list(CREATE, p, "breaklines")?
        .iter()
        .map(|line| match line.as_array() {
            Some(a) => a.iter().map(|v| xyz(CREATE, "breaklines", v, false)).collect(),
            None => Err(bad(CREATE, "`breaklines` must be an array of point arrays")),
        })
        .collect::<Result<Vec<Vec<_>>>>()?;
    let boundary = match p.get("boundary") {
        None | Some(Value::Null) => None,
        Some(_) => {
            Some(list(CREATE, p, "boundary")?.iter().map(|v| xyz(CREATE, "boundary", v, true).map(|q| [q[0], q[1]])).collect::<Result<Vec<_>>>()?)
        }
    };
    let opts = BuildOptions { max_edge: size_param(CREATE, p, "max_edge", false)?, boundary };
    let built = triangulate::build(&points, &breaklines, &opts).map_err(|e| bad(CREATE, e.to_string()))?;
    let mesh = tin_to_polyface(&built.tin)?;
    let handle = s.add_entity(EntityKind::PolyfaceMesh(mesh))?;
    s.set_selection(vec![handle]);
    Ok(json!({
        "handle": handle.hex(),
        "vertices": built.tin.vertices.len(),
        "triangles": built.tin.triangles.len(),
        "duplicates": built.duplicates,
        "skipped_segments": built.skipped_segments,
        "dropped_triangles": built.dropped_triangles,
    }))
}

/// The TIN at `surface` (or the one selected object) and its LandXML document.
fn export_xml(s: &Session, p: &Value) -> Result<(Tin, String)> {
    let handle = match p.get("surface") {
        Some(_) => handle_param(p, "surface").ok_or_else(|| bad(EXPORT, "`surface` must be an object handle"))?,
        None => match s.selection().as_slice() {
            [h] => *h,
            _ => return Err(bad(EXPORT, "give `surface` (a TIN polyface handle) or select exactly one")),
        },
    };
    let tin = tin_from_handle(s.doc()?, handle)?;
    let epsg = p.get("epsg").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|&n| n > 0);
    let epsg = epsg.ok_or_else(|| bad(EXPORT, "`epsg` (a positive EPSG code, e.g. 26913) is required"))?;
    let unit = match str_param(p, "unit") {
        None => LinearUnit::Meter,
        Some(u) => LinearUnit::parse(u).ok_or_else(|| bad(EXPORT, format!("`unit` {u:?} must be meter, foot or USSurveyFoot")))?,
    };
    let name = str_param(p, "name").unwrap_or("Surface").to_owned();
    #[cfg(not(target_arch = "wasm32"))]
    let unix_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
    #[cfg(target_arch = "wasm32")]
    let unix_time = None; // std has no clock on wasm32-unknown-unknown
    let xml = landxml::export(&tin, &ExportOptions { name, unit, epsg, unix_time }).map_err(|e| bad(EXPORT, e.to_string()))?;
    Ok((tin, xml))
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    let (tin, xml) = export_xml(s, p)?;
    let mut out = json!({ "points": tin.vertices.len(), "faces": tin.triangles.len() });
    match str_param(p, "path") {
        #[cfg(not(target_arch = "wasm32"))]
        Some(path) => {
            crate::atomic_write::write_atomic_making_folders(std::path::Path::new(path), xml.as_bytes())
                .map_err(|e| bad(EXPORT, format!("{path}: {e}")))?;
            out["path"] = json!(path);
        }
        #[cfg(target_arch = "wasm32")]
        Some(path) => return Err(bad(EXPORT, format!("{path}: paths are not available on the web; omit `path` to get `xml`"))),
        None => out["xml"] = json!(xml),
    }
    Ok(out)
}

/// The TIN as a polyface mesh: one face per triangle, 1-based `[a, b, c, 0]`.
pub(crate) fn tin_to_polyface(tin: &Tin) -> Result<PolyfaceMesh> {
    if i32::try_from(tin.vertices.len()).is_err() {
        return Err(bad(CONV, "the surface has too many vertices for a polyface mesh"));
    }
    let mut vertices = Vec::with_capacity(tin.vertices.len());
    for (i, v) in tin.vertices.iter().enumerate() {
        if !v.iter().all(|c| c.is_finite()) {
            return Err(bad(CONV, format!("vertex {i} has a non-finite coordinate")));
        }
        vertices.push(Vec3 { x: v[0], y: v[1], z: v[2] });
    }
    let mut faces = Vec::with_capacity(tin.triangles.len());
    for (i, t) in tin.triangles.iter().enumerate() {
        let mut face = [0i32; 4];
        for (slot, &ix) in face.iter_mut().zip(t) {
            let one_based = i32::try_from(ix).ok().filter(|&n| (n as usize) < vertices.len()).and_then(|n| n.checked_add(1));
            *slot = one_based.ok_or_else(|| bad(CONV, format!("triangle {i} names vertex {ix}, which does not exist")))?;
        }
        if face[0] == face[1] || face[1] == face[2] || face[0] == face[2] {
            return Err(bad(CONV, format!("triangle {i} repeats a vertex")));
        }
        faces.push(face);
    }
    Ok(PolyfaceMesh { vertices, faces })
}

/// A polyface mesh as a TIN, strictly: triangles are kept and quads split into (a,b,c)+(a,c,d);
/// any face naming fewer than three vertices, an unknown or non-finite vertex, or with no XY
/// area is an error, as is a mesh with no faces. Triangles come out counter-clockwise.
pub(crate) fn polyface_to_tin(mesh: &PolyfaceMesh) -> Result<Tin> {
    let vertices: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| [v.x, v.y, v.z]).collect();
    let n = u32::try_from(vertices.len()).map_err(|_| bad(CONV, "the mesh has too many vertices"))?;
    let mut triangles = Vec::with_capacity(mesh.faces.len());
    for (fi, f) in mesh.faces.iter().enumerate() {
        let face_err = |why: &str| bad(CONV, format!("polyface face {} {why}", fi + 1));
        let mut corners: Vec<u32> = Vec::with_capacity(4);
        for &ix in f.iter().take_while(|&&ix| ix != 0) {
            let at = ix.unsigned_abs().checked_sub(1).filter(|&k| k < n);
            corners.push(at.ok_or_else(|| face_err("names a vertex that does not exist"))?);
        }
        let tris: Vec<[u32; 3]> = match corners.as_slice() {
            [a, b, c] => vec![[*a, *b, *c]],
            [a, b, c, d] => vec![[*a, *b, *c], [*a, *c, *d]],
            _ => return Err(face_err("names fewer than three vertices")),
        };
        for mut t in tris {
            let p = t.map(|k| vertices.get(k as usize).copied().unwrap_or([f64::NAN; 3]));
            if !p.iter().flatten().all(|c| c.is_finite()) {
                return Err(face_err("has a non-finite vertex"));
            }
            let (area2, degenerate) = signed_area2(p[0], p[1], p[2]);
            if degenerate {
                return Err(face_err("has no area seen from above"));
            }
            if area2 < 0.0 {
                t.swap(1, 2);
            }
            triangles.push(t);
        }
    }
    if triangles.is_empty() {
        return Err(bad(CONV, "the polyface mesh has no faces"));
    }
    Ok(Tin { vertices, triangles })
}

/// The TIN drawn as the polyface mesh `handle`.
pub(crate) fn tin_from_handle(drawing: &Drawing, handle: Handle) -> Result<Tin> {
    let e = drawing.entity(handle).ok_or_else(|| bad(CONV, format!("no object has handle {}", handle.hex())))?;
    match &e.kind {
        EntityKind::PolyfaceMesh(m) => polyface_to_tin(m),
        _ => Err(bad(CONV, format!("object {} is not a polyface mesh (a TIN surface)", handle.hex()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn square() -> Tin {
        Tin { vertices: vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [10.0, 10.0, 3.0], [0.0, 10.0, 4.0]], triangles: vec![[0, 1, 2], [0, 2, 3]] }
    }

    #[test]
    fn tin_round_trips_through_a_polyface_mesh() {
        let tin = square();
        let mesh = tin_to_polyface(&tin).unwrap();
        assert_eq!(mesh.faces, vec![[1, 2, 3, 0], [1, 3, 4, 0]]);
        assert_eq!(polyface_to_tin(&mesh).unwrap(), tin);
    }

    #[test]
    fn quads_split_and_clockwise_faces_turn() {
        let tin = square();
        let mut mesh = tin_to_polyface(&tin).unwrap();
        mesh.faces = vec![[1, -4, 3, 2]]; // clockwise quad with a hidden edge
        let t = polyface_to_tin(&mesh).unwrap();
        assert_eq!(t.triangles, vec![[0, 2, 3], [0, 1, 2]]);
    }

    #[test]
    fn hostile_input_is_an_error_not_a_panic() {
        let mut bad_index = square();
        bad_index.triangles.push([0, 1, u32::MAX]);
        assert!(tin_to_polyface(&bad_index).is_err());
        let mut repeated = square();
        repeated.triangles.push([1, 1, 2]);
        assert!(tin_to_polyface(&repeated).is_err());
        let mut nan = square();
        nan.vertices[2][2] = f64::NAN;
        assert!(tin_to_polyface(&nan).is_err());

        let mesh = tin_to_polyface(&square()).unwrap();
        for faces in [vec![], vec![[1, 2, 0, 0]], vec![[1, 2, 99, 0]], vec![[1, 2, i32::MIN, 0]], vec![[1, 2, 2, 0]]] {
            assert!(polyface_to_tin(&PolyfaceMesh { vertices: mesh.vertices.clone(), faces }).is_err());
        }
        let collinear = PolyfaceMesh {
            vertices: vec![Vec3 { x: 0.0, y: 0.0, z: 0.0 }, Vec3 { x: 1.0, y: 1.0, z: 5.0 }, Vec3 { x: 2.0, y: 2.0, z: 0.0 }],
            faces: vec![[1, 2, 3, 0]],
        };
        assert!(polyface_to_tin(&collinear).is_err());
    }

    /// A stored (uncompressed) ZIP of `(name, bytes)` entries.
    fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data) in entries {
            let offset = out.len() as u32;
            let (crc, len, nlen) = (crc32fast::hash(data), data.len() as u32, name.len() as u16);
            let mut common = Vec::new();
            for v in [20u16, 0, 0, 0, 0] {
                common.extend_from_slice(&v.to_le_bytes()); // version, flags, method 0, time, date
            }
            for v in [crc, len, len] {
                common.extend_from_slice(&v.to_le_bytes());
            }
            common.extend_from_slice(&nlen.to_le_bytes());
            common.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&common);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&common);
            central.extend_from_slice(&[0; 8]); // comment length, disk, internal attributes, external attributes (low)
            central.extend_from_slice(&[0; 2]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        let n = entries.len() as u16;
        for v in [0u16, 0, n, n] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn tincreate_then_tinexport_round_trips_through_a_valid_handoff_package() {
        use sha2::{Digest, Sha256};
        let mut s = Session::new();
        // Points drawn as POINTs (as RTKIMPORT draws them), one repeated.
        for (x, y, z) in [
            (500000.0, 4100000.0, 10.0),
            (500010.0, 4100000.0, 11.5),
            (500010.0, 4100010.0, 12.25),
            (500000.0, 4100010.0, 9.75),
            (500005.0, 4100005.0, 13.0),
            (500005.0, 4100005.0, 99.0),
        ] {
            s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: Vec3 { x, y, z }, angle: 0.0 })).unwrap();
        }
        s.set_selection(vec![]);
        let v = s.execute(CREATE, &json!({})).unwrap();
        assert_eq!((v["vertices"].as_u64(), v["triangles"].as_u64(), v["duplicates"].as_u64()), (Some(5), Some(4), Some(1)));
        let handle = v["handle"].as_str().unwrap().to_owned();
        let tin = tin_from_handle(s.doc().unwrap(), Handle::parse_hex(&handle).unwrap()).unwrap();

        let v = s.execute(EXPORT, &json!({ "surface": handle, "epsg": 32613, "name": "EG", "unit": "meter" })).unwrap();
        let xml = v["xml"].as_str().unwrap().as_bytes().to_vec();
        let digest: String = Sha256::digest(&xml).iter().map(|b| format!("{b:02x}")).collect();
        let manifest = json!({
            "contract_version": "1.0",
            "package_id": "3f2b8c1e-4d5a-4b6c-9d7e-0123456789ab",
            "created_utc": "2026-11-04T09:00:00Z",
            "producer": { "name": "CADCraft", "version": "0.1.0" },
            "surface": { "filename": "surface.landxml", "sha256": digest, "landxml_version": "1.2", "name": "EG", "point_count": 5, "face_count": 4 },
            "coordinate_reference": {
                "horizontal": { "kind": "projected", "authority": "EPSG", "code": 32613, "unit": "metre" },
                "vertical": { "unit": "metre", "direction": "positive_up", "datum": { "status": "known", "authority": "EPSG", "code": 5703, "name": "NAVD88 height" } }
            }
        });
        let zip = stored_zip(&[("handoff.json", manifest.to_string().as_bytes()), ("surface.landxml", &xml)]);
        let report = cadcraft_handoff::validate(std::io::Cursor::new(zip), true).unwrap();
        assert_eq!(report.status, cadcraft_handoff::Status::Valid, "{}", report.to_text());

        // Round trip: the package's surface is the drawn TIN (northing/easting swapped back).
        let back = report.surface.unwrap();
        let points: Vec<[f64; 3]> = back.points.iter().map(|p| [p.easting, p.northing, p.elevation]).collect();
        assert_eq!(points, tin.vertices);
        let faces: Vec<[u32; 3]> = back.faces.iter().map(|f| f.map(|id| id as u32 - 1)).collect();
        assert_eq!(faces, tin.triangles);
    }

    #[test]
    fn tin_commands_refuse_bad_params() {
        let mut s = Session::new();
        assert!(s.execute(CREATE, &json!({})).is_err(), "no points");
        assert!(s.execute(CREATE, &json!({ "points": [[0, 0, 0], [1, 0]] })).is_err());
        assert!(s.execute(CREATE, &json!({ "points": "x" })).is_err());
        assert!(s.execute(CREATE, &json!({ "points": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "breaklines": [5] })).is_err());
        assert!(s.execute(CREATE, &json!({ "points": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "max_edge": -2 })).is_err());
        let v = s.execute(CREATE, &json!({ "points": [[0, 0, 0], [1, 0, 0], [0, 1, 0]], "boundary": [[-1, -1], [5, -1], [-1, 5]] })).unwrap();
        let h = v["handle"].as_str().unwrap().to_owned();
        assert!(s.execute(EXPORT, &json!({ "surface": h })).is_err(), "no epsg");
        assert!(s.execute(EXPORT, &json!({ "surface": h, "epsg": 26913, "unit": "yard" })).is_err());
        assert!(s.execute(EXPORT, &json!({ "surface": "ZZ", "epsg": 26913 })).is_err());
        assert!(s.execute(EXPORT, &json!({ "epsg": 26913 })).is_ok(), "the new TIN is selected");
    }

    #[test]
    fn reads_the_tin_drawn_at_a_handle() {
        let mut s = Session::new();
        let h = s.add_entity(EntityKind::PolyfaceMesh(tin_to_polyface(&square()).unwrap())).unwrap();
        let line = s.add_entity(EntityKind::Line(cadcraft_doc::Line { a: Vec3::default(), b: Vec3 { x: 1.0, y: 0.0, z: 0.0 } })).unwrap();
        let d = s.doc().unwrap();
        assert_eq!(tin_from_handle(d, h).unwrap(), square());
        assert!(tin_from_handle(d, line).is_err());
        assert!(tin_from_handle(d, Handle(0xFFFF_FFFF)).is_err());
    }
}
