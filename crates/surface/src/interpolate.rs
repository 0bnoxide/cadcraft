//! Scattered-point interpolation (IDW and natural neighbour) to a TIN or grid, and the
//! least-squares plane gradient. Reference: AutoGIS `groundwater_contours.py` (`fit_plane_gradient`,
//! the point minimums). Owner: Codex (see `docs/agents/slice-5-analysis.md`).

use crate::{Tin, triangulate};
use spade::{DelaunayTriangulation, Point2, Triangulation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolationError(pub &'static str);
impl std::fmt::Display for InterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for InterpolationError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Idw,
    NaturalNeighbor,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub method: Method,
    pub power: f64,
    pub radius: Option<f64>,
    pub max_neighbors: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self { method: Method::Idw, power: 2., radius: None, max_neighbors: 12 }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub downhill: [f64; 2],
    pub magnitude: f64,
    /// Clockwise from north, in degrees.
    pub azimuth: f64,
    pub r_squared: f64,
}

fn clean(points: &[[f64; 3]]) -> Result<Vec<[f64; 3]>, InterpolationError> {
    if points.len() > 100_000 || points.iter().flatten().any(|n| !n.is_finite() || n.abs() > 1e50) {
        return Err(InterpolationError("well coordinates must be finite within 1e50; at most 100000 wells"));
    }
    let mut out = points.to_vec();
    // Signed zero is the same XY location; canonicalize before sorting duplicates together.
    for p in &mut out {
        for v in p.iter_mut().take(2) {
            if *v == 0. {
                *v = 0.;
            }
        }
    }
    out.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    for pair in out.windows(2) {
        if let [a, b] = pair
            && a[0] == b[0]
            && a[1] == b[1]
            && a[2] != b[2]
        {
            return Err(InterpolationError("conflicting elevations at duplicate XY"));
        }
    }
    out.dedup_by(|a, b| a[0] == b[0] && a[1] == b[1]);
    if out.len() < 3 {
        return Err(InterpolationError("at least three distinct wells required"));
    }
    Ok(out)
}

/// Least-squares plane using centered coordinates; flat or collinear wells have no flow arrow.
pub fn fit_plane_gradient(points: &[[f64; 3]]) -> Result<Option<Gradient>, InterpolationError> {
    if points.len() < 3 {
        return Ok(None);
    }
    let points = clean(points)?;
    let n = points.len() as f64;
    let mean = points.iter().fold([0.; 3], |mut s, p| {
        for (s, p) in s.iter_mut().zip(p) {
            *s += p / n;
        }
        s
    });
    let (mut xx, mut yy, mut xy, mut xz, mut yz, mut zz) = (0., 0., 0., 0., 0., 0.);
    for &[x, y, z] in &points {
        let (x, y, z) = (x - mean[0], y - mean[1], z - mean[2]);
        xx += x * x;
        yy += y * y;
        xy += x * y;
        xz += x * z;
        yz += y * z;
        zz += z * z;
    }
    let det = xx * yy - xy * xy;
    if det <= 1e-12 * xx * yy || !det.is_finite() {
        return Ok(None);
    }
    let a = (xz * yy - yz * xy) / det;
    let b = (yz * xx - xz * xy) / det;
    if a.abs() < 1e-12 && b.abs() < 1e-12 {
        return Ok(None);
    }
    let residual = points.iter().map(|&[x, y, z]| (z - mean[2] - a * (x - mean[0]) - b * (y - mean[1])).powi(2)).sum::<f64>();
    Ok(Some(Gradient {
        downhill: [-a, -b],
        magnitude: a.hypot(b),
        azimuth: (-a).atan2(-b).to_degrees().rem_euclid(360.),
        r_squared: if zz > 0. { (1. - residual / zz).clamp(0., 1.) } else { 1. },
    }))
}

/// Interpolate without extrapolation outside the well convex hull. Radius/neighbour options apply to IDW.
pub fn at_points(wells: &[[f64; 3]], queries: &[[f64; 2]], options: &Options) -> Result<Vec<Option<f64>>, InterpolationError> {
    let wells = clean(wells)?;
    if !options.power.is_finite()
        || options.power <= 0.
        || options.power > 100.
        || options.max_neighbors == 0
        || options.max_neighbors > 100_000
        || options.radius.is_some_and(|r| !r.is_finite() || r <= 0.)
    {
        return Err(InterpolationError("invalid IDW power, radius or neighbour limit"));
    }
    if queries.len() > 1_000_000
        || queries.len().checked_mul(wells.len()).is_none_or(|n| n > 10_000_000)
        || queries.iter().flatten().any(|n| !n.is_finite() || n.abs() > 1e50)
    {
        return Err(InterpolationError("query coordinate or interpolation work limit exceeded"));
    }
    let mut dt = DelaunayTriangulation::<Point2<f64>>::new();
    let mut heights = Vec::new();
    for &[x, y, z] in &wells {
        let h = dt.insert(Point2::new(x, y)).map_err(|_| InterpolationError("cannot triangulate well coordinates"))?;
        if h.index() != heights.len() {
            return Err(InterpolationError("duplicate triangulation vertex"));
        }
        heights.push(z);
    }
    if dt.num_inner_faces() == 0 {
        return Err(InterpolationError("interpolation requires noncollinear wells"));
    }
    let nn = dt.natural_neighbor();
    let mut weights = Vec::new();
    let mut out = Vec::with_capacity(queries.len());
    for &[x, y] in queries {
        nn.get_weights(Point2::new(x, y), &mut weights);
        if weights.is_empty() {
            out.push(None);
            continue;
        }
        let value = match options.method {
            Method::NaturalNeighbor => weights.iter().try_fold(0., |sum, (h, w)| heights.get(h.index()).map(|z| sum + z * w)),
            Method::Idw => {
                let mut near: Vec<_> =
                    wells.iter().map(|p| ((p[0] - x).hypot(p[1] - y), p[2])).filter(|(d, _)| options.radius.is_none_or(|r| *d <= r)).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                near.truncate(options.max_neighbors);
                match near.first() {
                    None => None,
                    Some(&(0., z)) => Some(z),
                    Some(&(nearest, _)) => {
                        let (sum, total) = near.iter().fold((0., 0.), |(sum, total), &(d, z)| {
                            let w = (nearest / d).powf(options.power);
                            (sum + w * z, total + w)
                        });
                        Some(sum / total)
                    }
                }
            }
        };
        if value.is_some_and(|v| !v.is_finite()) {
            return Err(InterpolationError("interpolation overflow"));
        }
        out.push(value);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub origin: [f64; 2],
    pub cell_size: f64,
    pub width: usize,
    pub height: usize,
    /// Row-major grid nodes; None is outside the well hull or radius.
    pub values: Vec<Option<f64>>,
}
/// A node grid including the requested bounds (the final row/column can exceed the bounds).
pub fn grid(wells: &[[f64; 3]], bounds: [[f64; 2]; 2], cell_size: f64, options: &Options) -> Result<Grid, InterpolationError> {
    let [lo, hi] = bounds;
    if bounds.iter().flatten().any(|n| !n.is_finite() || n.abs() > 1e50) || !cell_size.is_finite() || cell_size <= 0. {
        return Err(InterpolationError("invalid grid bounds or spacing"));
    }
    let nx = ((hi[0] - lo[0]) / cell_size).ceil() + 1.;
    let ny = ((hi[1] - lo[1]) / cell_size).ceil() + 1.;
    if nx < 2. || ny < 2. || !nx.is_finite() || !ny.is_finite() || nx * ny > 1_000_000. {
        return Err(InterpolationError("grid cell limit exceeded or empty bounds"));
    }
    let (width, height) = (nx as usize, ny as usize);
    let mut queries = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            queries.push([lo[0] + x as f64 * cell_size, lo[1] + y as f64 * cell_size]);
        }
    }
    let values = at_points(wells, &queries, options)?;
    Ok(Grid { origin: lo, cell_size, width, height, values })
}

/// Interpolate elevations at every vertex of an existing TIN, preserving its topology.
/// Missing vertices are an error, rather than fabricated elevations.
pub fn onto_tin(wells: &[[f64; 3]], target: &Tin, options: &Options) -> Result<Tin, InterpolationError> {
    crate::compare::elevations(target, &[]).map_err(|_| InterpolationError("invalid target TIN"))?;
    let q: Vec<_> = target.vertices.iter().map(|p| [p[0], p[1]]).collect();
    let values = at_points(wells, &q, options)?;
    let vertices = q
        .into_iter()
        .zip(values)
        .map(|([x, y], z)| z.map(|z| [x, y, z]).ok_or(InterpolationError("target vertex outside interpolation coverage")))
        .collect::<Result<_, _>>()?;
    Ok(Tin { vertices, triangles: target.triangles.clone() })
}

/// Convert only fully covered grid cells; never bridge missing nodes by retriangulating them.
pub fn grid_tin(grid: &Grid) -> Result<Tin, InterpolationError> {
    let count =
        grid.width.checked_mul(grid.height).filter(|&n| n <= 1_000_000 && n == grid.values.len()).ok_or(InterpolationError("invalid grid size"))?;
    if grid.width < 2 || grid.height < 2 || !grid.cell_size.is_finite() || grid.cell_size <= 0. || grid.origin.iter().any(|n| !n.is_finite()) {
        return Err(InterpolationError("invalid grid"));
    }
    let mut tin = Tin::default();
    let mut indices = vec![None; count];
    for (i, z) in grid.values.iter().enumerate() {
        if let Some(z) = z {
            let p = [grid.origin[0] + (i % grid.width) as f64 * grid.cell_size, grid.origin[1] + (i / grid.width) as f64 * grid.cell_size, *z];
            if p.iter().any(|n| !n.is_finite()) {
                return Err(InterpolationError("invalid grid coordinate"));
            }
            if let Some(slot) = indices.get_mut(i) {
                *slot = Some(tin.vertices.len() as u32);
            }
            tin.vertices.push(p);
        }
    }
    for y in 0..grid.height - 1 {
        for x in 0..grid.width - 1 {
            let i = y * grid.width + x;
            let get = |i| indices.get(i).copied().flatten();
            if let (Some(a), Some(b), Some(c), Some(d)) = (get(i), get(i + 1), get(i + grid.width + 1), get(i + grid.width)) {
                tin.triangles.push([a, b, c]);
                tin.triangles.push([a, c, d]);
            }
        }
    }
    // Validate geometry using the same tolerance as the existing surface builder.
    for &[a, b, c] in &tin.triangles {
        let get = |i: u32| tin.vertices.get(i as usize).copied().ok_or(InterpolationError("invalid grid topology"));
        if triangulate::signed_area2(get(a)?, get(b)?, get(c)?).1 {
            return Err(InterpolationError("degenerate grid triangle"));
        }
    }
    Ok(tin)
}
