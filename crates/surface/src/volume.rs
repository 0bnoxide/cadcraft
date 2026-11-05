//! Cut and fill volumes between two TINs, optionally inside a `Region`, plus per-region totals.
//! Behaviour reference: REVENG `earthwork_processor.md` (behaviour only, no structure). Owner: Codex
//! (see `docs/agents/slice-5-analysis.md`).

use crate::{Region, Tin, compare};
use std::collections::HashMap;

pub const MAX_CELLS: usize = 1_000_000;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeError(pub &'static str);
impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for VolumeError {}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quantities {
    pub requested_area: f64,
    pub covered_area: f64,
    pub skipped_area: f64,
    pub cut: f64,
    pub fill: f64,
    /// Fill minus cut, in cubic drawing units.
    pub net: f64,
    pub samples: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub regions: Vec<Quantities>,
    /// Sum of region rows; overlapping regions are intentionally counted separately.
    pub total: Quantities,
    pub cell_size: f64,
}

fn valid_region(region: &Region) -> bool {
    if !region.is_valid() {
        return false;
    }
    let rings: Vec<_> = std::iter::once(&region.outer).chain(region.holes.iter()).collect();
    let edges = rings.iter().map(|r| r.len()).sum::<usize>();
    if edges.checked_mul(edges).is_none_or(|n| n > 1_000_000) {
        return false;
    }
    let cross = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let on = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
        cross(a, b, p) == 0. && p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1])
    };
    let mut segments = Vec::new();
    for (r, ring) in rings.iter().enumerate() {
        let mut area = 0.;
        let Some(&origin) = ring.first() else {
            return false;
        };
        for (i, (&a, &b)) in ring.iter().zip(ring.iter().cycle().skip(1)).enumerate() {
            if a == b || a.iter().chain(b.iter()).any(|n| n.abs() > 1e50) {
                return false;
            }
            area += cross(origin, a, b);
            segments.push((r, i, a, b));
        }
        if area == 0. || !area.is_finite() {
            return false;
        }
    }
    for (i, &(r, j, a, b)) in segments.iter().enumerate() {
        for &(s, k, c, d) in segments.iter().skip(i + 1) {
            if r == s && (j.abs_diff(k) == 1 || j.abs_diff(k) == rings.get(r).map_or(0, |r| r.len() - 1)) {
                continue;
            }
            let (ab_c, ab_d, cd_a, cd_b) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
            if (ab_c.signum() != ab_d.signum() && cd_a.signum() != cd_b.signum() && ab_c != 0. && ab_d != 0. && cd_a != 0. && cd_b != 0.)
                || on(a, b, c)
                || on(a, b, d)
                || on(c, d, a)
                || on(c, d, b)
            {
                return false;
            }
        }
    }
    let outer = Region { outer: region.outer.clone(), holes: vec![] };
    for (i, hole) in region.holes.iter().enumerate() {
        let Some(&p) = hole.first() else {
            return false;
        };
        if !outer.contains(p) {
            return false;
        }
        for (j, other) in region.holes.iter().enumerate() {
            if i != j && (Region { outer: other.clone(), holes: vec![] }).contains(p) {
                return false;
            }
        }
    }
    true
}

/// Bin regular-grid queries so each face samples only the cells in its XY bounding box.
fn sampled(tin: &Tin, points: &[[f64; 2]], bounds: [[f64; 2]; 2], step: f64, work: &mut usize) -> Result<Vec<Option<f64>>, VolumeError> {
    let spend = |work: &mut usize, n: usize| -> Result<(), VolumeError> {
        *work = work.checked_sub(n).ok_or(VolumeError("volume sampling work limit exceeded"))?;
        Ok(())
    };
    spend(work, tin.triangles.len())?;
    compare::elevations(tin, &[]).map_err(|e| VolumeError(e.0))?;
    let [lo, hi] = bounds;
    let tile = |p: [f64; 2]| -> Result<(usize, usize), VolumeError> {
        let x = ((p[0] - lo[0]) / step).floor();
        let y = ((p[1] - lo[1]) / step).floor();
        if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. || x > MAX_CELLS as f64 || y > MAX_CELLS as f64 {
            return Err(VolumeError("invalid sample tile"));
        }
        Ok((x as usize, y as usize))
    };
    let mut bins: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (i, &p) in points.iter().enumerate() {
        bins.entry(tile(p)?).or_default().push(i);
    }
    let mut out = vec![None; points.len()];
    for face in &tin.triangles {
        let mut vertices = Vec::with_capacity(3);
        for &i in face {
            vertices.push(*tin.vertices.get(i as usize).ok_or(VolumeError("invalid triangle index"))?);
        }
        let mut a = [f64::INFINITY; 2];
        let mut b = [f64::NEG_INFINITY; 2];
        for &[x, y, _] in &vertices {
            a[0] = a[0].min(x);
            a[1] = a[1].min(y);
            b[0] = b[0].max(x);
            b[1] = b[1].max(y);
        }
        // Preserve compare's 1e-9 edge-distance tolerance at tile boundaries.
        for axis in 0..2 {
            a[axis] = (a[axis] - 1e-9).max(lo[axis]);
            b[axis] = (b[axis] + 1e-9).min(hi[axis]);
        }
        if a[0] > b[0] || a[1] > b[1] {
            continue;
        }
        let (x0, y0) = tile(a)?;
        let (x1, y1) = tile(b)?;
        let count = (x1 - x0 + 1).checked_mul(y1 - y0 + 1).ok_or(VolumeError("sample tile count overflow"))?;
        spend(work, count)?;
        let mut indices = Vec::new();
        let mut queries = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(bucket) = bins.get(&(x, y)) {
                    spend(work, bucket.len())?;
                    for &i in bucket {
                        if out.get(i).is_some_and(Option::is_none) {
                            queries.push(*points.get(i).ok_or(VolumeError("invalid query index"))?);
                            indices.push(i);
                        }
                    }
                }
            }
        }
        if !queries.is_empty() {
            let local = Tin { vertices, triangles: vec![[0, 1, 2]] };
            let heights = compare::elevations(&local, &queries).map_err(|e| VolumeError(e.0))?;
            for (i, z) in indices.into_iter().zip(heights) {
                if let Some(slot) = out.get_mut(i) {
                    *slot = z;
                }
            }
        }
    }
    Ok(out)
}

/// Midpoint integration; boundary and zero-crossing cells are approximate. No zero filling.
/// Cells at the bounding rectangle's upper edges use their actual, possibly smaller, area.
// shortcut: midpoint boundary sampling, use polygon/prism clipping when exact irregular boundaries are needed.
pub fn calculate(base: &Tin, other: &Tin, regions: &[Region], cell_size: f64) -> Result<Report, VolumeError> {
    if !cell_size.is_finite() || cell_size <= 0. || regions.is_empty() || regions.len() > 10_000 {
        return Err(VolumeError("positive finite cell_size and 1..10000 regions required"));
    }
    let mut rows = Vec::new();
    let mut remaining = MAX_CELLS;
    let mut work_left = 10_000_000usize;
    for region in regions {
        let edges = region.outer.len().checked_add(region.holes.iter().map(Vec::len).sum::<usize>()).ok_or(VolumeError("region size overflow"))?;
        let validation_work = edges.checked_mul(edges).filter(|&n| n <= work_left).ok_or(VolumeError("region validation work limit exceeded"))?;
        work_left -= validation_work;
        if !valid_region(region) {
            return Err(VolumeError("invalid or excessive region"));
        }
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for &[x, y] in &region.outer {
            lo[0] = lo[0].min(x);
            lo[1] = lo[1].min(y);
            hi[0] = hi[0].max(x);
            hi[1] = hi[1].max(y);
        }
        let nx = ((hi[0] - lo[0]) / cell_size).ceil();
        let ny = ((hi[1] - lo[1]) / cell_size).ceil();
        if !nx.is_finite() || !ny.is_finite() || nx <= 0. || ny <= 0. || nx * ny > remaining as f64 {
            return Err(VolumeError("grid cell limit exceeded or zero-area region"));
        }
        let (nx, ny) = (nx as usize, ny as usize);
        let count = nx.checked_mul(ny).filter(|&n| n <= remaining).ok_or(VolumeError("grid cell limit exceeded"))?;
        remaining -= count;
        let edges = region.outer.len() + region.holes.iter().map(Vec::len).sum::<usize>();
        let sampling_work = count.checked_mul(edges).filter(|&n| n <= work_left).ok_or(VolumeError("region sampling work limit exceeded"))?;
        work_left -= sampling_work;
        let mut points = Vec::new();
        let mut areas = Vec::new();
        for y in 0..ny {
            for x in 0..nx {
                let left = lo[0] + x as f64 * cell_size;
                let bottom = lo[1] + y as f64 * cell_size;
                let width = cell_size.min(hi[0] - left);
                let height = cell_size.min(hi[1] - bottom);
                let p = [left + width / 2., bottom + height / 2.];
                if region.contains(p) {
                    points.push(p);
                    areas.push(width * height);
                }
            }
        }
        let a = sampled(base, &points, [lo, hi], cell_size, &mut work_left)?;
        let b = sampled(other, &points, [lo, hi], cell_size, &mut work_left)?;
        let mut row = Quantities::default();
        for ((a, b), area) in a.into_iter().zip(b).zip(areas) {
            row.requested_area += area;
            row.samples += 1;
            if let (Some(a), Some(b)) = (a, b) {
                row.covered_area += area;
                row.cut += (a - b).max(0.) * area;
                row.fill += (b - a).max(0.) * area;
            } else {
                row.skipped_area += area;
            }
        }
        row.net = row.fill - row.cut;
        rows.push(row);
    }
    let mut total = Quantities::default();
    for r in &rows {
        total.requested_area += r.requested_area;
        total.covered_area += r.covered_area;
        total.skipped_area += r.skipped_area;
        total.cut += r.cut;
        total.fill += r.fill;
        total.samples += r.samples;
    }
    total.net = total.fill - total.cut;
    if [total.requested_area, total.covered_area, total.skipped_area, total.cut, total.fill, total.net].iter().any(|v| !v.is_finite()) {
        return Err(VolumeError("quantity overflow"));
    }
    Ok(Report { regions: rows, total, cell_size })
}
