//! A plan-view region (outer ring with optional holes), shared by volumes, interpolation and
//! audits. Changing it needs both owners to agree by Paseo message (see
//! `docs/agents/slice-5-analysis.md`).

/// A polygon region in the drawing plane: `[x, y]` rings, each implicitly closed. Points on an
/// edge count as inside.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Region {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}

impl Region {
    /// Whether the outer ring has at least three finite vertices, and every hole does too.
    pub fn is_valid(&self) -> bool {
        let ring_ok = |r: &[[f64; 2]]| r.len() >= 3 && r.iter().all(|p| p[0].is_finite() && p[1].is_finite());
        ring_ok(&self.outer) && self.holes.iter().all(|h| ring_ok(h))
    }

    /// Whether `p` lies inside the outer ring (edges included) and outside every hole (hole edges
    /// count as inside the region). An invalid region contains nothing.
    pub fn contains(&self, p: [f64; 2]) -> bool {
        if !self.is_valid() || !(p[0].is_finite() && p[1].is_finite()) {
            return false;
        }
        match ring_side(&self.outer, p) {
            Side::Outside => false,
            Side::Edge => true,
            Side::Inside => self.holes.iter().all(|h| ring_side(h, p) != Side::Inside),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Inside,
    Edge,
    Outside,
}

/// Even-odd ray test with an exact on-segment check first.
fn ring_side(ring: &[[f64; 2]], p: [f64; 2]) -> Side {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (Some(a), Some(b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let within = p[0] >= a[0].min(b[0]) && p[0] <= a[0].max(b[0]) && p[1] >= a[1].min(b[1]) && p[1] <= a[1].max(b[1]);
        if cross == 0.0 && within {
            return Side::Edge;
        }
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    if inside { Side::Inside } else { Side::Outside }
}

#[cfg(test)]
mod tests {
    use super::Region;

    fn square(size: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]]
    }

    #[test]
    fn inside_edge_outside_and_holes() {
        let r = Region { outer: square(10.0), holes: vec![vec![[4.0, 4.0], [6.0, 4.0], [6.0, 6.0], [4.0, 6.0]]] };
        assert!(r.contains([1.0, 1.0]));
        assert!(r.contains([10.0, 5.0]), "outer edge is inside");
        assert!(r.contains([0.0, 0.0]), "vertex is inside");
        assert!(!r.contains([11.0, 5.0]));
        assert!(!r.contains([5.0, 5.0]), "inside a hole");
        assert!(r.contains([4.0, 5.0]), "hole edge stays in the region");
        assert!(!r.contains([f64::NAN, 1.0]));
    }

    #[test]
    fn invalid_regions_contain_nothing() {
        assert!(!Region { outer: vec![[0.0, 0.0], [1.0, 0.0]], holes: vec![] }.contains([0.5, 0.0]));
        assert!(!Region { outer: vec![[0.0, 0.0], [f64::INFINITY, 0.0], [0.0, 1.0]], holes: vec![] }.contains([0.1, 0.1]));
    }
}
