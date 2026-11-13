use super::{Bounds, Triangle, overlaps, spend};
use crate::{Result, error};
use std::ops::Range;
struct Node {
    bounds: Bounds,
    range: Range<usize>,
    children: Option<[usize; 2]>,
}
pub(super) struct TriangleIndex {
    order: Vec<usize>,
    nodes: Vec<Node>,
}
impl TriangleIndex {
    pub(super) fn new(triangles: &[Triangle]) -> Result<Self> {
        if triangles.is_empty() || triangles.len() > 1_000_000 {
            return Err(error("INVALID_TIN", "triangle index needs 1..1000000 triangles"));
        }
        let mut index = Self { order: (0..triangles.len()).collect(), nodes: Vec::new() };
        index.build(triangles, 0..triangles.len(), 0)?;
        Ok(index)
    }
    fn build(&mut self, triangles: &[Triangle], range: Range<usize>, depth: usize) -> Result<usize> {
        let ids = self.order.get_mut(range.clone()).ok_or_else(|| error("INVALID_TIN", "invalid triangle index range"))?;
        let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for id in ids.iter() {
            let t = triangles.get(*id).ok_or_else(|| error("INVALID_TIN", "triangle index missing"))?;
            bounds[0] = bounds[0].min(t.bounds[0]);
            bounds[1] = bounds[1].min(t.bounds[1]);
            bounds[2] = bounds[2].max(t.bounds[2]);
            bounds[3] = bounds[3].max(t.bounds[3]);
        }
        let slot = self.nodes.len();
        self.nodes.push(Node { bounds, range: range.clone(), children: None });
        // Median splits bound construction depth even when every triangle overlaps.
        if ids.len() > 8 && depth < 20 {
            let axis = usize::from(bounds[3] - bounds[1] > bounds[2] - bounds[0]);
            let center = |id: &usize| triangles.get(*id).map_or(0., |t| t.bounds[axis] + (t.bounds[axis + 2] - t.bounds[axis]) / 2.);
            let middle = ids.len() / 2;
            ids.select_nth_unstable_by(middle, |a, b| center(a).total_cmp(&center(b)).then(a.cmp(b)));
            let split = range.start + middle;
            let left = self.build(triangles, range.start..split, depth + 1)?;
            let right = self.build(triangles, split..range.end, depth + 1)?;
            if let Some(node) = self.nodes.get_mut(slot) {
                node.children = Some([left, right]);
            }
        }
        Ok(slot)
    }
    pub(super) fn locate<'a>(&self, triangles: &'a [Triangle], p: [f64; 2], budget: &mut usize) -> Result<Option<&'a Triangle>> {
        let mut stack = vec![0];
        let mut best: Option<usize> = None;
        while let Some(slot) = stack.pop() {
            spend(budget)?;
            let node = self.nodes.get(slot).ok_or_else(|| error("INVALID_TIN", "triangle index node missing"))?;
            if !overlaps(node.bounds, [p[0], p[1], p[0], p[1]]) {
                continue;
            }
            if let Some(children) = node.children {
                stack.extend(children);
            } else {
                for id in self.order.get(node.range.clone()).ok_or_else(|| error("INVALID_TIN", "triangle index leaf missing"))? {
                    if best.is_some_and(|best| *id >= best) {
                        continue;
                    }
                    spend(budget)?;
                    let t = triangles.get(*id).ok_or_else(|| error("INVALID_TIN", "triangle index missing"))?;
                    if t.contains(p) {
                        best = Some(*id);
                    }
                }
            }
        }
        Ok(best.and_then(|id| triangles.get(id)))
    }
}
