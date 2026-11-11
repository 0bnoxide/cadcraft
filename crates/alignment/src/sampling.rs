use crate::geometry::{cross, dot, sub};
use crate::{
    Alignment, Element, Kind, MAX_POINTS, Result, error,
    profile::{GroundProfile, Sample},
};
use std::{collections::BTreeSet, f64::consts::PI};
type Bounds = [f64; 4];
struct Triangle {
    xy: [[f64; 2]; 3],
    origin: [f64; 3],
    gradient: [f64; 2],
    bounds: Bounds,
    ids: [u32; 3],
}
fn bounds(p: impl IntoIterator<Item = [f64; 2]>) -> Bounds {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for [x, y] in p {
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    b
}
fn overlaps(a: Bounds, b: Bounds) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}
impl Triangle {
    fn new(vertices: &[[f64; 3]], ids: [u32; 3]) -> Result<Self> {
        let mut p = [[0.; 3]; 3];
        for (slot, id) in p.iter_mut().zip(ids) {
            *slot = *vertices.get(id as usize).ok_or_else(|| error("INVALID_TIN", "triangle index out of range"))?;
        }
        let xy = p.map(|p| [p[0], p[1]]);
        let u = sub(xy[1], xy[0]);
        let v = sub(xy[2], xy[0]);
        let det = cross(u, v);
        if det == 0. || !det.is_finite() {
            return Err(error("INVALID_TIN", "triangle has no finite plan area"));
        }
        let dz1 = p[1][2] - p[0][2];
        let dz2 = p[2][2] - p[0][2];
        let gradient = [(dz1 * v[1] - dz2 * u[1]) / det, (u[0] * dz2 - v[0] * dz1) / det];
        if gradient.iter().any(|g| !g.is_finite()) {
            return Err(error("INVALID_TIN", "triangle plane overflow"));
        }
        Ok(Self { xy, origin: p[0], gradient, bounds: bounds(xy), ids })
    }
    fn contains(&self, p: [f64; 2]) -> bool {
        let u = sub(self.xy[1], self.xy[0]);
        let v = sub(self.xy[2], self.xy[0]);
        let q = sub(p, self.xy[0]);
        let d = cross(u, v);
        let a = cross(q, v) / d;
        let b = cross(u, q) / d;
        a >= 0. && b >= 0. && a + b <= 1.
    }
    fn z(&self, p: [f64; 2]) -> f64 {
        self.origin[2] + dot(self.gradient, [p[0] - self.origin[0], p[1] - self.origin[1]])
    }
}
fn element_bounds(e: &Element) -> Bounds {
    let n = (e.length * e.max_curvature() / 0.1).ceil().max(1.) as u32;
    let h = e.length / f64::from(n);
    let margin = e.max_curvature() * h * h / 8.;
    let mut b = bounds((0..=n).map(|i| e.pose(f64::from(i) * h).point));
    b[0] -= margin;
    b[1] -= margin;
    b[2] += margin;
    b[3] += margin;
    b
}
fn spend(budget: &mut usize) -> Result<()> {
    if *budget == 0 {
        return Err(error("NUMERICAL_LIMIT", "TIN crossing work exceeds 5000000 evaluations"));
    }
    *budget -= 1;
    Ok(())
}
fn crossings(e: &Element, a: [f64; 2], b: [f64; 2], budget: &mut usize) -> Result<Vec<f64>> {
    let edge = sub(b, a);
    let edge2 = dot(edge, edge);
    if edge2 == 0. {
        return Ok(Vec::new());
    }
    if matches!(e.kind, Kind::Line) {
        return Ok(line_crossings(e, a, b));
    }
    let (k0, k1) = e.curvatures();
    let theta0 = e.direction.rem_euclid(2. * PI);
    let theta1 = theta0 + (k0 + k1) * e.length / 2.;
    let angle = edge[1].atan2(edge[0]);
    let mut cuts = vec![0., e.length];
    let first = ((theta0.min(theta1) - angle) / PI).ceil() as i32;
    let last = ((theta0.max(theta1) - angle) / PI).floor() as i32;
    for n in first..=last {
        let target = angle + f64::from(n) * PI;
        let mut lo = 0.;
        let mut hi = e.length;
        for _ in 0..64 {
            let mid = (lo + hi) / 2.;
            let theta = theta0 + k0 * mid + (k1 - k0) * mid * mid / (2. * e.length);
            if (theta < target) == (theta1 > theta0) { lo = mid } else { hi = mid }
        }
        cuts.push((lo + hi) / 2.);
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    let f = |s: f64| cross(sub(e.pose(s).point, a), edge);
    let mut result = Vec::new();
    for pair in cuts.windows(2) {
        let [start, end] = pair else { continue };
        spend(budget)?;
        let fs = f(*start);
        let fe = f(*end);
        let mut roots = Vec::new();
        if fs.abs() <= 1e-10 * edge2.sqrt() {
            roots.push(*start)
        }
        if fe.abs() <= 1e-10 * edge2.sqrt() {
            roots.push(*end)
        }
        if fs.signum() != fe.signum() {
            let mut lo = *start;
            let mut hi = *end;
            for _ in 0..64 {
                spend(budget)?;
                let mid = (lo + hi) / 2.;
                if f(mid).signum() == fs.signum() { lo = mid } else { hi = mid }
            }
            roots.push((lo + hi) / 2.);
        }
        for s in roots {
            let t = dot(sub(e.pose(s).point, a), edge) / edge2;
            if (0.0..=1.0).contains(&t) {
                result.push(s);
            }
        }
    }
    Ok(result)
}
fn line_crossings(e: &Element, a: [f64; 2], b: [f64; 2]) -> Vec<f64> {
    let p = e.pose(0.).point;
    let q = e.pose(e.length).point;
    let r = sub(q, p);
    let edge = sub(b, a);
    let det = cross(r, edge);
    let ap = sub(a, p);
    if det != 0. {
        let u = cross(ap, edge) / det;
        let v = cross(ap, r) / det;
        if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) { vec![u * e.length] } else { Vec::new() }
    } else if cross(ap, r).abs() <= 1e-10 * e.length {
        let r2 = dot(r, r);
        [a, b].into_iter().map(|p| dot(sub(p, e.start), r) / r2).filter(|u| (0.0..=1.0).contains(u)).map(|u| u * e.length).collect()
    } else {
        Vec::new()
    }
}
pub fn sample(a: &Alignment, vertices: &[[f64; 3]], triangles: &[[u32; 3]], tolerance: f64) -> Result<GroundProfile> {
    a.checked()?;
    if !tolerance.is_finite() || tolerance <= 0. {
        return Err(error("INVALID_TOLERANCE", "positive finite elevation tolerance required"));
    }
    if vertices.len() > 1_000_000
        || triangles.is_empty()
        || triangles.len() > 1_000_000
        || vertices.iter().flatten().any(|v| !v.is_finite() || v.abs() > 1e9)
    {
        return Err(error("INVALID_TIN", "finite vertices and 1..1000000 triangles required"));
    }
    let triangles = triangles.iter().map(|t| Triangle::new(vertices, *t)).collect::<Result<Vec<_>>>()?;
    let mut profile = GroundProfile { name: "Ground".into(), segments: Vec::new() };
    let mut segment = Vec::new();
    let mut station = a.start_station;
    let mut budget = 5_000_000;
    let mut count = 0usize;
    for e in &a.elements {
        let extent = element_bounds(e);
        let candidates: Vec<_> = triangles.iter().filter(|t| overlaps(extent, t.bounds)).collect();
        let mut edges = BTreeSet::new();
        for t in &candidates {
            let [a, b, c] = t.ids;
            for [u, v] in [[a, b], [b, c], [c, a]] {
                edges.insert([u.min(v), u.max(v)]);
                if edges.len() > 2_000_000 {
                    return Err(error("EDGE_LIMIT", "too many candidate edges"));
                }
            }
        }
        let mut cuts = vec![0., e.length];
        for [u, v] in edges {
            let a = vertices.get(u as usize).ok_or_else(|| error("INVALID_TIN", "edge vertex missing"))?;
            let b = vertices.get(v as usize).ok_or_else(|| error("INVALID_TIN", "edge vertex missing"))?;
            let p = [a[0], a[1]];
            let q = [b[0], b[1]];
            if overlaps(extent, bounds([p, q])) {
                spend(&mut budget)?;
                cuts.extend(crossings(e, p, q, &mut budget)?);
                if cuts.len() > MAX_POINTS {
                    return Err(error("POINT_LIMIT", "too many crossings"));
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        // Nearby crossings can bound a real uncovered interval; never merge by distance.
        cuts.dedup();
        let mut covered = Vec::new();
        for pair in cuts.windows(2) {
            let [lo, hi] = pair else { continue };
            let mid = e.pose((lo + hi) / 2.).point;
            let triangle = candidates.iter().find(|t| t.contains(mid));
            covered.push(triangle.is_some());
            if let Some(t) = triangle {
                append_interval(&mut segment, &mut count, e, t, (*lo, *hi), station, tolerance)?;
            } else if !segment.is_empty() {
                profile.segments.push(std::mem::take(&mut segment));
            }
        }
        if !segment.is_empty() {
            profile.segments.push(std::mem::take(&mut segment));
        }
        // A tangent/vertex contact can have no covered interval on either side.
        for (i, &cut) in cuts.iter().enumerate() {
            let before = i.checked_sub(1).and_then(|i| covered.get(i)).copied().unwrap_or(false);
            if before || covered.get(i).copied().unwrap_or(false) {
                continue;
            }
            let point = e.pose(cut).point;
            if let Some(t) = candidates.iter().find(|t| t.contains(point)) {
                if count >= MAX_POINTS {
                    return Err(error("POINT_LIMIT", "too many contact samples"));
                }
                count += 1;
                profile.segments.push(vec![Sample { station: station + cut, elevation: t.z(point) }]);
            }
        }
        station += e.length;
    }
    if !segment.is_empty() {
        profile.segments.push(segment);
    }
    if profile.segments.is_empty() {
        return Err(error("NO_COVERAGE", "alignment does not cross the TIN"));
    }
    profile.segments.sort_by(|a, b| a.first().map_or(f64::INFINITY, |p| p.station).total_cmp(&b.first().map_or(f64::INFINITY, |p| p.station)));
    let mut merged: Vec<Vec<Sample>> = Vec::new();
    for segment in profile.segments {
        if let Some(previous) = merged.last_mut()
            && let (Some(end), Some(start)) = (previous.last(), segment.first())
            && end.station == start.station
        {
            if (end.elevation - start.elevation).abs() > 1e-6 {
                return Err(error("DISCONTINUOUS_GROUND", "different elevations at a shared station"));
            }
            previous.extend(segment.into_iter().skip(1));
            continue;
        }
        merged.push(segment);
    }
    profile.segments = merged;
    profile.validate(a)?;
    Ok(profile)
}
fn append_interval(
    out: &mut Vec<Sample>,
    count: &mut usize,
    e: &Element,
    t: &Triangle,
    span: (f64, f64),
    station: f64,
    tolerance: f64,
) -> Result<()> {
    let (lo, hi) = span;
    let bound = t.gradient[0].hypot(t.gradient[1]) * e.max_curvature();
    let n = if bound == 0. { 1. } else { ((hi - lo) / (8. * tolerance / bound).sqrt()).ceil().max(1.) };
    if !n.is_finite() || n > MAX_POINTS as f64 || count.saturating_add(n as usize + 1) > MAX_POINTS {
        return Err(error("POINT_LIMIT", "ground profile exceeds 100000 samples"));
    }
    for i in 0..=(n as usize) {
        let distance = lo + (hi - lo) * (i as f64) / n;
        let s = station + distance;
        let elevation = t.z(e.pose(distance).point);
        if !elevation.is_finite() {
            return Err(error("INVALID_TIN", "sample elevation overflow"));
        }
        if out.last().is_some_and(|p| p.station == s) {
            continue;
        }
        out.push(Sample { station: s, elevation });
        *count += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crossing_outside_the_edge_segment_is_excluded() {
        let e = Element { start: [1., 0.], direction: PI / 2., length: PI / 2., kind: Kind::Arc { radius: 1., turn: crate::Turn::Left } };
        // Unit-circle x=1/2 crosses at y=sqrt(3)/2, just beyond this edge.
        let end = 3f64.sqrt() / 2. - 1e-11;
        assert!(crossings(&e, [0.5, -0.2], [0.5, end], &mut 10000).unwrap().is_empty());
    }
}
