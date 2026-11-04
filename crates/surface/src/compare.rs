//! Surface comparison: elevation of one TIN at points or at the vertices of another, and the
//! difference statistics. Outside samples are reported separately, without extrapolation.

use crate::Tin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareError(pub &'static str);
impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for CompareError {}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Statistics {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub rms: Option<f64>,
    pub count: usize,
    pub outside: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Target elevation minus reference elevation, in input order; `None` means outside.
    pub differences: Vec<Option<f64>>,
    pub statistics: Statistics,
}

/// Interpolate at XY points. Points within 1e-9 drawing units of the triangle are
/// clamped to its closest edge. Invalid geometry and excessive work return errors.
pub fn elevations(tin: &Tin, points: &[[f64; 2]]) -> Result<Vec<Option<f64>>, CompareError> {
    let faces = faces(tin)?;
    check_work(tin, points.len())?;
    let mut out = Vec::with_capacity(points.len());
    for &[x, y] in points {
        if !valid_number(x) || !valid_number(y) {
            return Err(CompareError("query coordinates must be finite and within 1e100"));
        }
        out.push(faces.iter().find_map(|face| elevation(face, [x, y])));
    }
    Ok(out)
}

/// Compare XYZ measurements with the reference TIN. No drawing or TIN is modified.
pub fn compare_points(reference: &Tin, points: &[[f64; 3]]) -> Result<Comparison, CompareError> {
    check_work(reference, points.len())?;
    if points.iter().flatten().any(|&n| !valid_number(n)) {
        return Err(CompareError("query coordinates must be finite and within 1e100"));
    }
    let xy: Vec<_> = points.iter().map(|&[x, y, _]| [x, y]).collect();
    let heights = elevations(reference, &xy)?;
    let mut statistics = Statistics::default();
    let mut differences = Vec::with_capacity(points.len());
    let mut mean = 0.;
    let mut norm = 0f64;
    for (&[_, _, z], height) in points.iter().zip(heights) {
        let diff = height.map(|h| z - h);
        if let Some(d) = diff {
            statistics.count += 1;
            statistics.min = Some(statistics.min.map_or(d, |m| m.min(d)));
            statistics.max = Some(statistics.max.map_or(d, |m| m.max(d)));
            mean += (d - mean) / statistics.count as f64;
            norm = norm.hypot(d);
        } else {
            statistics.outside += 1;
        }
        differences.push(diff);
    }
    if statistics.count > 0 {
        statistics.mean = Some(mean);
        statistics.rms = Some(norm / (statistics.count as f64).sqrt());
    }
    Ok(Comparison { differences, statistics })
}

/// Compare every target vertex, including unused vertices, against the reference.
pub fn compare_vertices(reference: &Tin, target: &Tin) -> Result<Comparison, CompareError> {
    faces(target)?;
    compare_points(reference, &target.vertices)
}

fn check_work(tin: &Tin, count: usize) -> Result<(), CompareError> {
    // shortcut: bounded linear face search, add a spatial index when larger surveys are needed.
    if count > 1_000_000 || tin.triangles.len().checked_mul(count).is_none_or(|n| n > 10_000_000) {
        return Err(CompareError("comparison limit exceeded (1 million samples, 10 million face/sample visits)"));
    }
    Ok(())
}

fn valid_number(n: f64) -> bool {
    n.is_finite() && n.abs() <= 1e100
}

fn faces(tin: &Tin) -> Result<Vec<[[f64; 3]; 3]>, CompareError> {
    if tin.vertices.len() > 1_000_000 || tin.triangles.len() > 2_000_000 {
        return Err(CompareError("TIN size limit exceeded"));
    }
    if tin.vertices.iter().flatten().any(|&n| !valid_number(n)) {
        return Err(CompareError("TIN coordinates must be finite and within 1e100"));
    }
    tin.triangles
        .iter()
        .map(|&[ia, ib, ic]| {
            let get = |i: u32| tin.vertices.get(i as usize).copied().ok_or(CompareError("triangle index out of range"));
            let (a, b, c) = (get(ia)?, get(ib)?, get(ic)?);
            let [ax, ay, _] = a;
            let [bx, by, _] = b;
            let [cx, cy, _] = c;
            let scale = (bx - ax).hypot(by - ay).max((cx - ax).hypot(cy - ay)).max((cx - bx).hypot(cy - by));
            let area = ((bx - ax) / scale) * ((cy - ay) / scale) - ((by - ay) / scale) * ((cx - ax) / scale);
            if !area.is_finite() || area.abs() <= 1e-12 {
                return Err(CompareError("degenerate XY triangle"));
            }
            Ok([a, b, c])
        })
        .collect()
}

fn elevation(&[a, b, c]: &[[f64; 3]; 3], [x, y]: [f64; 2]) -> Option<f64> {
    let query = [x, y];
    let [ax, ay, az] = a;
    let [bx, by, bz] = b;
    let [cx, cy, cz] = c;
    let scale = (bx - ax).hypot(by - ay).max((cx - ax).hypot(cy - ay));
    let (bx, by, cx, cy, x, y) = ((bx - ax) / scale, (by - ay) / scale, (cx - ax) / scale, (cy - ay) / scale, (x - ax) / scale, (y - ay) / scale);
    let det = bx * cy - by * cx;
    let wb = (x * cy - y * cx) / det;
    let wc = (bx * y - by * x) / det;
    let wa = 1. - wb - wc;
    if wa >= 0. && wb >= 0. && wc >= 0. {
        return Some(wa * az + wb * bz + wc * cz);
    }
    // Distance to the actual segments handles corners as well as long and short edges.
    let mut closest = f64::INFINITY;
    let mut height = None;
    for (p, q) in [(a, b), (b, c), (c, a)] {
        let [px, py, pz] = p;
        let [qx, qy, qz] = q;
        let [x, y] = query;
        let len = (qx - px).hypot(qy - py);
        let (ux, uy) = ((qx - px) / len, (qy - py) / len);
        let along = ((x - px) * ux + (y - py) * uy).clamp(0., len);
        let distance = (x - (px + along * ux)).hypot(y - (py + along * uy));
        if distance <= 1e-9 && distance < closest {
            closest = distance;
            let t = along / len;
            height = Some(pz + t * (qz - pz));
        }
    }
    height
}
