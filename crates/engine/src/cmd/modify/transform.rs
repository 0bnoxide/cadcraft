use crate::cmd::{alignment_store, sheet_store};
use cadcraft_doc::{Drawing, EntityKind, LwPolyline};
use cadcraft_geom::Vec3;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{Entity, Handle, Mat3, Result, Session, Value, Vec2, bad, bool_or, erase_kept, f64_req, json, point_param, point_req, targets};

// ---------------- core operations ----------------

pub(crate) fn transform_entities(s: &mut Session, hs: &[Handle], m: &Mat3, copy: bool) -> Result<Vec<Handle>> {
    refuse_ole_frames(s, hs, m, copy)?;
    edit_transformed(s, hs, m, copy, None)
}

pub(super) fn rotate_entities(s: &mut Session, hs: &[Handle], base: Vec2, angle: f64, copy: bool) -> Result<Vec<Handle>> {
    let turn = std::f64::consts::TAU;
    let remainder = (angle % turn).abs();
    // Conversion to radians can leave a small remainder even for an exact full turn.
    let roundoff = (angle.abs() * f64::EPSILON).min(1e-12);
    let angle = if angle.abs() >= turn / 2. && remainder.min(turn - remainder) <= roundoff { 0. } else { angle };
    transform_entities(s, hs, &Mat3::rotate_about(base, angle), copy)
}

/// Scale `hs` by `f` about `base` in 3D, as AutoCAD 2024's SCALE does: about the base point's XY,
/// and every height of the objects about its Z, their thickness too.
pub(crate) fn scale_entities(s: &mut Session, hs: &[Handle], base: Vec3, f: f64, copy: bool) -> Result<Vec<Handle>> {
    let m = Mat3::scale_about(base.xy(), f);
    refuse_ole_frames(s, hs, &m, copy)?;
    edit_transformed(s, hs, &m, copy, Some((base.z, f)))
}

/// An OLE2FRAME only translates or scales uniformly (its corners stay axis aligned): refuse any other `m` before anything
/// changes, instead of reporting success and leaving the frame where it was.
pub(crate) fn refuse_ole_frames(s: &Session, hs: &[Handle], m: &Mat3, copy: bool) -> Result<()> {
    if cadcraft_doc::ole_frame_accepts(m) || !holds_ole_frame(s, hs, copy)? {
        return Ok(());
    }
    Err(ole_refusal())
}

/// The refusal for a transform an OLE2FRAME cannot take.
pub(crate) fn ole_refusal() -> crate::EngineError {
    crate::EngineError::Other(
        "An OLE object can only be moved, copied or scaled uniformly; other transforms (rotation, mirroring, non-uniform scale, alignment) are not supported.".into(),
    )
}

/// Whether `hs` holds an OLE2FRAME a transform would touch. An in-place edit (`copy` false) skips
/// an object on a locked layer (see edit_entities), so such a frame cannot be refused.
pub(crate) fn holds_ole_frame(s: &Session, hs: &[Handle], copy: bool) -> Result<bool> {
    let d = s.doc()?;
    Ok(hs.iter().any(|h| {
        d.entity(*h).is_some_and(|e| {
            let locked = !copy && d.layer(&e.common.layer).is_some_and(|l| l.locked);
            !locked && matches!(&e.kind, cadcraft_doc::EntityKind::Unknown(u) if u.is_ole_frame())
        })
    }))
}

/// `p` at the current elevation (ELEVATION), the Z AutoCAD gives a point entered without one.
pub(crate) fn at_elevation(s: &Session, p: Vec2) -> Vec3 {
    let z = s.doc().map(|d| d.header.f64("ELEVATION", 0.0)).ok().filter(|z| z.is_finite());
    Vec3::new(p.x, p.y, z.unwrap_or(0.0))
}

/// Apply `edit` to each of `hs` still in the drawing, or to a copy of each when `copy`; an object
/// on a locked layer is left alone unless it is copied. The handles edited or made.
fn edit_entities(d: &mut Drawing, hs: &[Handle], copy: bool, edit: impl Fn(&mut Entity)) -> Result<Vec<Handle>> {
    let mut out = Vec::new();
    for h in hs {
        let Some(sp) = d.space_of(*h) else { continue };
        if copy {
            let Some(e) = d.entity(*h).map(|e| (**e).clone()) else { continue };
            let nh = d.new_handle();
            let mut ne = Entity { handle: nh, ..e };
            edit(&mut ne);
            if let Some(st) = d.space_mut(&sp) {
                st.push(ne);
                out.push(nh);
            }
        } else {
            let locked = d.entity(*h).and_then(|e| d.layer(&e.common.layer)).is_some_and(|l| l.locked);
            if locked {
                continue;
            }
            d.modify_entity(*h, &edit)?;
            out.push(*h);
        }
    }
    Ok(out)
}

fn edit_transformed(s: &mut Session, hs: &[Handle], matrix: &Mat3, copy: bool, height_scale: Option<(f64, f64)>) -> Result<Vec<Handle>> {
    let source = s.doc()?;
    let mut index = None;
    let mut alignments = BTreeMap::new();
    for &h in hs {
        let Some(e) = source.entity(h) else { continue };
        if !copy && source.layer(&e.common.layer).is_some_and(|layer| layer.locked) {
            continue;
        }
        if alignment_store::is_carrier(source, h, &mut index)? {
            if copy {
                return copy_transformed(s, hs, std::iter::once(Ok(*matrix)), height_scale);
            }
            let (data, changed) = alignment_store::transformed(source, h, matrix, height_scale, false)?;
            alignments.insert(h, changed.then_some(data));
        }
    }
    if alignments.is_empty() {
        return edit_entities(s.doc_mut()?, hs, copy, |e| edit_geometry(e, matrix, height_scale));
    }

    let mut staged = source.clone();
    let out = edit_entities(&mut staged, hs, false, |e| {
        if !alignments.contains_key(&e.handle) {
            edit_geometry(e, matrix, height_scale);
        }
    })?;
    let index = index.get_or_insert_with(|| alignment_store::RecordIndex::new(&staged));
    for (h, data) in alignments {
        let Some(data) = data else { continue };
        let carrier = alignment_store::carrier(&data)?;
        staged.modify_entity(h, |e| replace_carrier(e, carrier.clone(), height_scale))?;
        alignment_store::save_in(&mut staged, index, h, &data)?;
        sheet_store::mark_alignment_transformed(&mut staged, index, h)?;
    }
    *s.doc_mut()? = staged;
    Ok(out)
}

fn edit_geometry(e: &mut Entity, matrix: &Mat3, height_scale: Option<(f64, f64)>) {
    e.kind.transform(matrix);
    if let Some((base_z, factor)) = height_scale {
        e.kind.scale_heights(base_z, factor);
        e.common.thickness *= factor;
    }
}

fn replace_carrier(e: &mut Entity, mut carrier: LwPolyline, height_scale: Option<(f64, f64)>) {
    if let EntityKind::LwPolyline(p) = &e.kind {
        carrier.plinegen = p.plinegen;
    }
    e.kind = EntityKind::LwPolyline(carrier);
    if let Some((_, factor)) = height_scale {
        e.common.thickness *= factor;
    }
}

pub(super) fn copy_translated(s: &mut Session, hs: &[Handle], delta: Vec2, count: u64) -> Result<Vec<Handle>> {
    let matrices = (1..=count).map(|k| {
        let offset = delta * k as f64;
        if !offset.x.is_finite() || !offset.y.is_finite() {
            return Err(bad("copy", "copy offset must remain finite"));
        }
        Ok(Mat3::translate(offset))
    });
    copy_transformed(s, hs, matrices, None)
}

/// Stage the whole copy batch. Each alignment gets its own extension tree and exact geometry.
fn copy_transformed(
    s: &mut Session,
    hs: &[Handle],
    matrices: impl IntoIterator<Item = Result<Mat3>>,
    height_scale: Option<(f64, f64)>,
) -> Result<Vec<Handle>> {
    let source = s.doc()?;
    let mut staged = source.clone();
    let mut index = None;
    let max_foreign = source.foreign.objects.iter().map(|o| o.handle.0).max().unwrap_or(0);
    staged.handseed = staged.handseed.max(max_foreign.saturating_add(1));
    let mut out = Vec::new();
    for matrix in matrices {
        let matrix = matrix?;
        refuse_ole_frames(s, hs, &matrix, true)?;
        for h in hs {
            let Some(entity) = source.entity(*h).map(|e| (**e).clone()) else { continue };
            let Some(space) = source.space_of(*h) else { continue };
            let data = if alignment_store::is_carrier(source, *h, &mut index)? {
                Some(alignment_store::transformed(source, *h, &matrix, height_scale, true)?.0)
            } else {
                None
            };
            let tree = data
                .as_ref()
                .and_then(|_| source.foreign.entities.get(h))
                .and_then(|extra| extra.xdict)
                .map(|root| source.foreign.complete_tree(root).ok_or_else(|| bad("copy", "incomplete or oversized extension dictionary")))
                .transpose()?;
            let needed = tree.as_ref().map_or(1, |t| t.len() + 3) as u64;
            if staged.handseed.checked_add(needed).is_none() {
                return Err(bad("copy", "handle space exhausted"));
            }
            let new_handle = staged.new_handle();
            let mut copied = Entity { handle: new_handle, ..entity };
            if let Some(data) = &data {
                replace_carrier(&mut copied, alignment_store::carrier(data)?, height_scale);
            } else {
                edit_geometry(&mut copied, &matrix, height_scale);
            }
            let dst = staged.space_mut(&space).ok_or_else(|| bad("copy", "target space missing"))?;
            dst.push(copied);
            if let (Some(data), Some(tree)) = (data, tree) {
                let fresh: Vec<Handle> = (0..tree.len() + 2).map(|_| staged.new_handle()).collect();
                let foreign = Arc::make_mut(&mut staged.foreign);
                foreign.duplicate_tree(&source.foreign, &tree, &fresh, new_handle);
                foreign.entities.entry(new_handle).or_default().xdict = fresh.first().copied();
                alignment_store::save(&mut staged, new_handle, &data)?;
                alignment_store::load(&staged, new_handle)?;
            }
            out.push(new_handle);
        }
    }
    if !out.is_empty() {
        *s.doc_mut()? = staged;
    }
    Ok(out)
}

pub(super) fn erase(s: &mut Session, hs: &[Handle]) -> Result<usize> {
    let d = s.doc()?;
    let free: Vec<Handle> = hs.iter().copied().filter(|h| !d.entity(*h).and_then(|e| d.layer(&e.common.layer)).is_some_and(|l| l.locked)).collect();
    let n = erase_kept(s, &free)?;
    s.set_selection(Vec::new());
    Ok(n)
}

pub(super) fn run_erase(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let n = erase(s, &hs)?;
    Ok(json!({ "erased": n }))
}

pub(super) fn delta_of(cmd: &str, p: &Value) -> Result<Vec2> {
    if let Some(d) = point_param(p, "delta") {
        return Ok(d);
    }
    let a = point_req(cmd, p, "from")?;
    let b = point_req(cmd, p, "to")?;
    Ok(b - a)
}

pub(super) fn run_move(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let d = delta_of("move", p)?;
    let moved = transform_entities(s, &hs, &Mat3::translate(d), false)?;
    Ok(json!({ "moved": moved.len() }))
}

pub(super) fn run_copy(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let d = delta_of("copy", p)?;
    let n = match p.get("count") {
        None => 1,
        Some(v) => v.as_u64().filter(|n| (1..=10_000).contains(n)).ok_or_else(|| bad("copy", "`count` must be an integer from 1 to 10000"))?,
    };
    let out: Vec<_> = copy_translated(s, &hs, d, n)?.into_iter().map(|h| h.hex()).collect();
    Ok(json!({ "handles": out }))
}

pub(super) fn run_rotate(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = point_req("rotate", p, "base")?;
    let a = (f64_req("rotate", p, "angle")? % 360.).to_radians();
    let r = rotate_entities(s, &hs, base, a, bool_or(p, "copy", false))?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

pub(super) fn run_scale(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let base = point_req("scale", p, "base")?;
    let f = f64_req("scale", p, "factor")?;
    if f <= 0.0 {
        return Err(bad("scale", "factor must be positive"));
    }
    // A Z given with the base point, as AutoCAD takes one; without one, the current elevation.
    let base = match p.get("base").and_then(|b| b.get(2)) {
        Some(z) => z.as_f64().map(|z| Vec3::new(base.x, base.y, z)).ok_or_else(|| bad("scale", "`base` Z must be a number"))?,
        None => at_elevation(s, base),
    };
    let r = scale_entities(s, &hs, base, f, bool_or(p, "copy", false))?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

pub(super) fn run_mirror(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = targets(s, p)?;
    let a = point_req("mirror", p, "p1")?;
    let b = point_req("mirror", p, "p2")?;
    if a.near(b, cadcraft_geom::EPS) {
        return Err(bad("mirror", "mirror line points coincide"));
    }
    let erase_src = bool_or(p, "erase", false);
    let r = transform_entities(s, &hs, &Mat3::mirror(a, b), !erase_src)?;
    Ok(json!({ "handles": r.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}
