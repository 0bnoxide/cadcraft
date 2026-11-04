//! Build a TIN from points, breaklines and an optional outer boundary (constrained Delaunay via
//! `spade`). Owner: Claude (see `docs/agents/slice-2-tin.md`).
//!
//! Breaklines are inserted first, so where a breakline vertex and a point share an XY position the
//! breakline's elevation wins; among points the first one wins. Breakline segments that would
//! cross an earlier breakline are skipped and counted, never forced.

use std::fmt;

use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use crate::Tin;

/// Most vertices (points plus breakline vertices) one build accepts.
pub const MAX_VERTICES: usize = 5_000_000;
/// Most boundary vertices.
pub const MAX_BOUNDARY: usize = 100_000;

/// Why a TIN could not be built.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BuildError {}

/// Options for [`build`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuildOptions {
    /// Drop triangles with any side longer than this (XY). `None` keeps the whole convex hull.
    pub max_edge: Option<f64>,
    /// Outer boundary (closed implicitly). Only triangles wholly inside it are kept (no triangle is
    /// cut, so the edge of the TIN follows the triangulation, inside the boundary).
    pub boundary: Option<Vec<[f64; 2]>>,
}

/// A built TIN and what the build left out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Built {
    pub tin: Tin,
    /// Input vertices whose XY repeated an earlier one (merged; the first elevation kept).
    pub duplicates: usize,
    /// Breakline segments skipped because they crossed an earlier breakline.
    pub skipped_segments: usize,
    /// Triangles dropped by the maximum edge length, the boundary, or for having no area.
    pub dropped_triangles: usize,
}

fn err(msg: impl Into<String>) -> BuildError {
    BuildError(msg.into())
}

/// Twice the signed XY area of a triangle, and whether it is too thin to keep: the LandXML
/// handoff contract (v1) rejects |cross| <= 1e-12 times the longest squared edge.
pub fn signed_area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> (f64, bool) {
    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let d2 = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
    let longest = d2(a, b).max(d2(b, c)).max(d2(c, a));
    (cross, cross.is_nan() || cross.abs() <= 1e-12 * longest)
}

/// Boundary edges as `(a, b)` pairs, closing the ring.
fn ring(poly: &[[f64; 2]]) -> impl Iterator<Item = ([f64; 2], [f64; 2])> + '_ {
    poly.iter().zip(poly.iter().cycle().skip(1)).map(|(a, b)| (*a, *b))
}

/// Inside the polygon (even-odd) or within `ON` of its edge.
fn inside_or_on(poly: &[[f64; 2]], x: f64, y: f64) -> bool {
    const ON: f64 = 1e-9;
    let mut odd = false;
    for (a, b) in ring(poly) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((x - a[0]) * dx + (y - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        if (x - a[0] - t * dx).hypot(y - a[1] - t * dy) <= ON {
            return true;
        }
        if (a[1] > y) != (b[1] > y) && x < dx * (y - a[1]) / dy + a[0] {
            odd = !odd;
        }
    }
    odd
}

/// Does segment `rs` pass through the open interior of triangle `c` (more than `ON` inside)?
/// Clips the segment to the triangle's three half-planes and tests the clipped piece's midpoint,
/// which covers proper crossings, boundary vertices inside, and chords between touch points.
fn enters(c: [[f64; 2]; 3], r: [f64; 2], s: [f64; 2]) -> bool {
    const ON: f64 = 1e-9;
    let turn = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[1][1] - c[0][1]) * (c[2][0] - c[0][0]);
    let sign = if turn < 0.0 { -1.0 } else { 1.0 };
    // Signed distance of `p` inside side i (positive = towards the triangle).
    let dist = |i: usize, p: [f64; 2]| {
        let (a, b) = (c[i], c[(i + 1) % 3]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        sign * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / len
    };
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for i in 0..3 {
        let (d0, d1) = (dist(i, r), dist(i, s));
        if d0 < 0.0 && d1 < 0.0 {
            return false;
        }
        if d0 != d1 {
            let t = d0 / (d0 - d1);
            if d0 < 0.0 {
                lo = lo.max(t);
            } else if d1 < 0.0 {
                hi = hi.min(t);
            }
        }
    }
    if lo >= hi {
        return false;
    }
    let t = (lo + hi) / 2.0;
    let m = [r[0] + t * (s[0] - r[0]), r[1] + t * (s[1] - r[1])];
    (0..3).all(|i| dist(i, m) > ON)
}

/// Whole-triangle clipping: kept only when every corner is inside (or on) the boundary and no
/// boundary edge passes through its interior, so nothing of a kept triangle lies outside.
// shortcut: O(triangles x boundary edges); index the boundary edges (rstar) if big boundaries get slow.
fn within(poly: &[[f64; 2]], t: [[f64; 3]; 3]) -> bool {
    let c = t.map(|p| [p[0], p[1]]);
    c.iter().all(|p| inside_or_on(poly, p[0], p[1]))
        && inside_or_on(poly, (c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0)
        && ring(poly).all(|(r, s)| !enters(c, r, s))
}

/// Builds a constrained Delaunay TIN. Every input coordinate must be finite.
pub fn build(points: &[[f64; 3]], breaklines: &[Vec<[f64; 3]>], opts: &BuildOptions) -> Result<Built, BuildError> {
    let total = breaklines.iter().try_fold(points.len(), |n, b| n.checked_add(b.len())).filter(|&n| n <= MAX_VERTICES);
    if total.is_none() {
        return Err(err(format!("too many vertices (the limit is {MAX_VERTICES})")));
    }
    if let Some(m) = opts.max_edge
        && !(m.is_finite() && m > 0.0)
    {
        return Err(err("the maximum edge length must be a positive number"));
    }
    if let Some(b) = &opts.boundary {
        if b.len() < 3 || b.len() > MAX_BOUNDARY {
            return Err(err(format!("the boundary needs 3 to {MAX_BOUNDARY} vertices")));
        }
        if !b.iter().flatten().all(|c| c.is_finite()) {
            return Err(err("a boundary vertex has a non-finite coordinate"));
        }
    }

    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    // Elevation per spade vertex index; spade hands back the existing handle for a repeated XY.
    let mut zs: Vec<f64> = Vec::new();
    let mut out = Built::default();
    let mut insert = |cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>, p: &[f64; 3], out: &mut Built| {
        if !p.iter().all(|c| c.is_finite()) {
            return Err(err(format!("the vertex {p:?} has a non-finite coordinate")));
        }
        let h = cdt.insert(Point2::new(p[0], p[1])).map_err(|e| err(format!("cannot insert {p:?}: {e:?}")))?;
        if h.index() < zs.len() {
            out.duplicates += 1;
        } else {
            zs.push(p[2]);
        }
        Ok(h)
    };
    for line in breaklines {
        let mut prev = None;
        for p in line {
            let h = insert(&mut cdt, p, &mut out)?;
            if let Some(from) = prev
                && from != h
                && cdt.try_add_constraint(from, h).is_empty()
                && !cdt.exists_constraint(from, h)
            {
                out.skipped_segments += 1;
            }
            prev = Some(h);
        }
    }
    for p in points {
        insert(&mut cdt, p, &mut out)?;
    }

    let max2 = opts.max_edge.map(|m| m * m);
    let mut used: Vec<Option<u32>> = vec![None; zs.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for face in cdt.inner_faces() {
        let vs = face.vertices();
        let p = vs.map(|v| v.fix().index());
        let mut corner = [[0.0f64; 3]; 3];
        for (c, v) in corner.iter_mut().zip(vs) {
            let z = zs.get(v.fix().index()).copied().ok_or_else(|| err("internal: a triangle names an unknown vertex"))?;
            *c = [v.position().x, v.position().y, z];
        }
        let [a, b, c] = corner;
        let (_, degenerate) = signed_area2(a, b, c);
        let d2 = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
        let too_long = max2.is_some_and(|m| d2(a, b) > m || d2(b, c) > m || d2(c, a) > m);
        let outside = opts.boundary.as_deref().is_some_and(|poly| !within(poly, [a, b, c]));
        if degenerate || too_long || outside {
            out.dropped_triangles += 1;
            continue;
        }
        let mut tri = [0u32; 3];
        for (slot, (&i, xyz)) in tri.iter_mut().zip(p.iter().zip(corner)) {
            let entry = used.get_mut(i).ok_or_else(|| err("internal: vertex index out of range"))?;
            *slot = match *entry {
                Some(k) => k,
                None => {
                    let k = u32::try_from(vertices.len()).map_err(|_| err("too many vertices"))?;
                    vertices.push(xyz);
                    *entry = Some(k);
                    k
                }
            };
        }
        triangles.push(tri);
    }
    if triangles.is_empty() {
        return Err(err("no triangles: give at least three points that are not all in a line (inside the boundary)"));
    }
    out.tin = Tin { vertices, triangles };
    Ok(out)
}
