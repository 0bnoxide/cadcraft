use crate::geometry::{cross, distance, dot, sub};
use crate::{Alignment, Beyond, Kind, Projection, Result, error};
use std::f64::consts::TAU;

impl Alignment {
    pub fn project(&self, point: [f64; 2]) -> Result<Projection> {
        self.checked()?;
        if point.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err(error("OUT_OF_RANGE", "projection point outside finite XY range"));
        }
        let mut best: Option<(f64, f64, usize)> = None;
        let mut station = self.start_station;
        let mut budget = 200_000usize;
        for (index, e) in self.elements.iter().enumerate() {
            let mut consider = |s: f64| {
                let d = distance(e.pose(s).point, point);
                let st = station + s;
                if best.is_none_or(|(bd, bs, _)| d < bd - 1e-9 || ((d - bd).abs() <= 1e-9 && st < bs)) {
                    best = Some((d, st, index));
                }
            };
            consider(0.);
            consider(e.length);
            match e.kind {
                Kind::Line => {
                    let (sn, cs) = e.direction.rem_euclid(TAU).sin_cos();
                    consider(dot(sub(point, e.start), [cs, sn]).clamp(0., e.length));
                }
                Kind::Arc { radius, turn } => {
                    let d = e.direction.rem_euclid(TAU);
                    let center = [e.start[0] - turn.sign() * radius * d.sin(), e.start[1] + turn.sign() * radius * d.cos()];
                    let r = sub(point, center);
                    if r[0] != 0. || r[1] != 0. {
                        let initial = (e.start[1] - center[1]).atan2(e.start[0] - center[0]);
                        let target = r[1].atan2(r[0]);
                        let delta = (turn.sign() * (target - initial)).rem_euclid(TAU);
                        let s = delta * radius;
                        if s <= e.length {
                            consider(s);
                        }
                    }
                }
                Kind::Spiral { .. } => {}
            }
            if matches!(e.kind, Kind::Spiral { .. }) {
                let mut intervals = vec![(0., e.length)];
                while let Some((lo, hi)) = intervals.pop() {
                    if budget == 0 {
                        return Err(error("NUMERICAL_LIMIT", "projection exceeded 200000 intervals"));
                    }
                    budget -= 1;
                    let a = e.pose(lo).point;
                    let b = e.pose(hi).point;
                    let ab = sub(b, a);
                    let ab2 = dot(ab, ab);
                    let t = if ab2 > 0. { (dot(sub(point, a), ab) / ab2).clamp(0., 1.) } else { 0. };
                    let s = lo + t * (hi - lo);
                    let d = distance(e.pose(s).point, point);
                    let st = station + s;
                    if best.is_none_or(|(bd, bs, _)| d < bd - 1e-9 || ((d - bd).abs() <= 1e-9 && st < bs)) {
                        best = Some((d, st, index));
                    }
                    let chord = [a[0] + t * ab[0], a[1] + t * ab[1]];
                    let lower = (distance(point, chord) - e.max_curvature() * (hi - lo).powi(2) / 8.).max(0.);
                    let best_d = best.map_or(f64::INFINITY, |v| v.0);
                    if lower >= best_d - 1e-9 || hi - lo <= 1e-8 {
                        continue;
                    }
                    let mid = (lo + hi) / 2.;
                    intervals.push((mid, hi));
                    intervals.push((lo, mid));
                }
            }
            station += e.length;
        }
        let (distance, station, index) = best.ok_or_else(|| error("INVALID_ALIGNMENT", "empty alignment"))?;
        let pose = self.at(station, 0.)?;
        let delta = sub(point, pose.point);
        let (sn, cs) = pose.direction.sin_cos();
        let lateral = cross([cs, sn], delta);
        let along = dot(delta, [cs, sn]);
        let end = self.start_station + self.elements.iter().map(|e| e.length).sum::<f64>();
        let beyond = if station == self.start_station && along < 0. {
            Some(Beyond::Before)
        } else if station == end && along > 0. {
            Some(Beyond::After)
        } else {
            None
        };
        let offset = if beyond.is_some() { lateral } else { distance.copysign(lateral) };
        Ok(Projection { station, offset, distance, element: index, beyond })
    }
}
