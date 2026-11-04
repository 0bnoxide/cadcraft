//! TINCREATE and TINEXPORT, plus the shared TIN <-> polyface mesh conversion whose API was agreed
//! with Codex by Paseo message (2e005518 / 5048efaf). Owner: Claude (see
//! `docs/agents/slice-2-tin.md`).

use cadcraft_doc::{Drawing, EntityKind, Handle, PolyfaceMesh};
use cadcraft_geom::Vec3;
use cadcraft_surface::Tin;

use super::{CommandSpec, bad};
use crate::Result;

/// Error prefix for the conversion, which serves several commands.
const CONV: &str = "tin";

pub fn specs() -> Vec<CommandSpec> {
    Vec::new()
}

/// Twice the signed XY area of a triangle, and whether it is too thin to keep: the LandXML
/// handoff contract (v1) rejects |cross| <= 1e-12 times the longest squared edge.
fn signed_area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> (f64, bool) {
    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let d2 = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
    let longest = d2(a, b).max(d2(b, c)).max(d2(c, a));
    (cross, cross.is_nan() || cross.abs() <= 1e-12 * longest)
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
        let collinear = PolyfaceMesh { vertices: vec![Vec3 { x: 0.0, y: 0.0, z: 0.0 }, Vec3 { x: 1.0, y: 1.0, z: 5.0 }, Vec3 { x: 2.0, y: 2.0, z: 0.0 }], faces: vec![[1, 2, 3, 0]] };
        assert!(polyface_to_tin(&collinear).is_err());
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
