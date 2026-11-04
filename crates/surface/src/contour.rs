//! Contours: marching triangles, chained into polylines, at an interval with index contours.
//! Flat faces and isolated level vertices produce no line. Shared level edges appear once.
//! Branches at saddle vertices end paths rather than choosing an arbitrary continuation.

use std::collections::{BTreeMap, BTreeSet};

use crate::Tin;

#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub elevation: f64,
    pub index: bool,
    /// Closed paths omit the repeated closing point.
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContourError(pub &'static str);

impl std::fmt::Display for ContourError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ContourError {}

// Topology identifies intersections exactly, without rounding nearby independent contours together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Node {
    Vertex(u32),
    Edge(u32, u32),
}

/// Levels are multiples of `interval` from zero; every `index_every`th level is an index contour.
/// At most 10,000 levels, 10 million face/level visits and 2 million output points are allowed.
pub fn contours(tin: &Tin, interval: f64, index_every: u32) -> Result<Vec<Contour>, ContourError> {
    if !interval.is_finite() || interval <= 0. || index_every == 0 {
        return Err(ContourError("interval and index_every must be positive and finite"));
    }
    validate(tin)?;
    if tin.triangles.is_empty() {
        return Ok(Vec::new());
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for face in &tin.triangles {
        for &i in face {
            let [_, _, z] = vertex(tin, i)?;
            low = low.min(z);
            high = high.max(z);
        }
    }
    let round_ratio = |r: f64| if (r - r.round()).abs() <= (4. * f64::EPSILON * r.abs()).min(1e-7) { r.round() } else { r };
    let first = round_ratio(low / interval).ceil();
    let last = round_ratio(high / interval).floor();
    let count = (last - first + 1.).max(0.);
    if !first.is_finite()
        || !last.is_finite()
        || first.abs().max(last.abs()) > 9_007_199_254_740_991.
        || count > 10_000.
        || count * tin.triangles.len() as f64 > 10_000_000.
    {
        return Err(ContourError("contour level range or work limit exceeded"));
    }
    let mut result = Vec::new();
    let mut points_left = 2_000_000usize;
    for offset in 0..count as u32 {
        let level = first as i64 + i64::from(offset);
        let elevation = level as f64 * interval;
        if !elevation.is_finite() {
            return Err(ContourError("contour elevation overflow"));
        }
        let (nodes, segments) = segments_at(tin, elevation, interval)?;
        for (points, closed) in chain(&nodes, &segments)? {
            points_left = points_left.checked_sub(points.len()).ok_or(ContourError("contour output limit exceeded"))?;
            result.push(Contour { elevation, index: level % i64::from(index_every) == 0, points, closed });
        }
    }
    Ok(result)
}

fn vertex(tin: &Tin, i: u32) -> Result<[f64; 3], ContourError> {
    tin.vertices.get(i as usize).copied().ok_or(ContourError("triangle index out of range"))
}

fn validate(tin: &Tin) -> Result<(), ContourError> {
    if tin.vertices.len() > 1_000_000 || tin.triangles.len() > 2_000_000 {
        return Err(ContourError("TIN size limit exceeded"));
    }
    if tin.vertices.iter().flatten().any(|n| !n.is_finite() || n.abs() > 1e100) {
        return Err(ContourError("TIN coordinates must be finite and within 1e100"));
    }
    for &[ia, ib, ic] in &tin.triangles {
        let [ax, ay, _] = vertex(tin, ia)?;
        let [bx, by, _] = vertex(tin, ib)?;
        let [cx, cy, _] = vertex(tin, ic)?;
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        if ia == ib || ib == ic || ic == ia || area == 0. {
            return Err(ContourError("triangle has repeated vertices or zero XY area"));
        }
    }
    Ok(())
}

fn segments_at(tin: &Tin, z: f64, interval: f64) -> Result<(BTreeMap<Node, [f64; 3]>, BTreeSet<(Node, Node)>), ContourError> {
    let mut nodes = BTreeMap::new();
    let mut segments = BTreeSet::new();
    for &[a, b, c] in &tin.triangles {
        let at_level = |i| -> Result<_, ContourError> {
            let [x, y, h] = vertex(tin, i)?;
            let tolerance = (4. * f64::EPSILON * z.abs().max(h.abs())).min(interval * 1e-6);
            Ok((i, [x, y, if (h - z).abs() <= tolerance { z } else { h }]))
        };
        let vertices = [at_level(a)?, at_level(b)?, at_level(c)?];
        if vertices.iter().all(|(_, [_, _, h])| *h == z) {
            continue;
        }
        let mut hits = BTreeMap::new();
        for ((ia, pa), (ib, pb)) in [(vertices[0], vertices[1]), (vertices[1], vertices[2]), (vertices[2], vertices[0])] {
            let [ax, ay, az] = pa;
            let [bx, by, bz] = pb;
            if az == z {
                hits.insert(Node::Vertex(ia), pa);
            }
            if bz == z {
                hits.insert(Node::Vertex(ib), pb);
            }
            if (az < z && bz > z) || (az > z && bz < z) {
                // Always compute from the same endpoint to keep shared-edge coordinates identical.
                let (lo, hi, p, q) = if ia < ib { (ia, ib, [ax, ay, az], [bx, by, bz]) } else { (ib, ia, [bx, by, bz], [ax, ay, az]) };
                let [px, py, pz] = p;
                let [qx, qy, qz] = q;
                let t = (z - pz) / (qz - pz);
                hits.insert(Node::Edge(lo, hi), [px + t * (qx - px), py + t * (qy - py), z]);
            }
        }
        let mut iter = hits.iter();
        if let (Some((&a, &pa)), Some((&b, &pb))) = (iter.next(), iter.next()) {
            if pa == pb {
                continue;
            }
            segments.insert((a, b));
            nodes.extend(hits);
        }
    }
    Ok((nodes, segments))
}

fn chain(nodes: &BTreeMap<Node, [f64; 3]>, segments: &BTreeSet<(Node, Node)>) -> Result<Vec<(Vec<[f64; 3]>, bool)>, ContourError> {
    let mut neighbors: BTreeMap<Node, Vec<Node>> = BTreeMap::new();
    for &(a, b) in segments {
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
    }
    let mut unused = segments.clone();
    let mut paths = Vec::new();
    // Open paths and branches first; all remaining components are cycles.
    let starts = neighbors.iter().filter(|(_, next)| next.len() != 2).map(|(&node, _)| node).chain(neighbors.keys().copied());
    for start in starts {
        for &next in neighbors.get(&start).into_iter().flatten() {
            let mut current = start;
            let mut following = next;
            let mut points = vec![*nodes.get(&start).ok_or(ContourError("missing contour node"))?];
            let mut closed = false;
            loop {
                let edge = if current < following { (current, following) } else { (following, current) };
                if !unused.remove(&edge) {
                    break;
                }
                current = following;
                if current == start {
                    closed = true;
                    break;
                }
                points.push(*nodes.get(&current).ok_or(ContourError("missing contour node"))?);
                let adjacent = neighbors.get(&current).ok_or(ContourError("missing contour adjacency"))?;
                if adjacent.len() != 2 {
                    break;
                }
                let Some(&candidate) = adjacent.iter().find(|&&node| unused.contains(&(node.min(current), node.max(current)))) else {
                    break;
                };
                following = candidate;
            }
            if points.len() >= 2 {
                paths.push((points, closed));
            }
        }
    }
    Ok(paths)
}
