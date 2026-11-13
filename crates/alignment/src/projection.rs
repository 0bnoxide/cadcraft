use crate::geometry::{cross, distance, dot, sub};
use crate::{Alignment, Beyond, Element, Kind, Projection, Result, ValidatedAlignment, error};
use std::f64::consts::TAU;

impl Alignment {
    pub fn project(&self, point: [f64; 2]) -> Result<Projection> {
        self.validated()?.project(point)
    }
}
impl ValidatedAlignment<'_> {
    pub fn project(&self, point: [f64; 2]) -> Result<Projection> {
        let a = self.alignment;
        if point.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err(error("OUT_OF_RANGE", "projection point outside finite XY range"));
        }
        let mut best: Option<Candidate> = None;
        // Accumulate local lengths before adding the station origin so the last
        // candidate has the same representable end station as Alignment::at.
        let mut distance_along = 0.;
        let mut budget = 200_000usize;
        for (index, e) in a.elements.iter().enumerate() {
            let mut consider = |s: f64| {
                let d = distance(e.pose(s).point, point);
                let st = a.start_station + (distance_along + s);
                consider_candidate(&mut best, Candidate { distance: d, station: st, element: index, root: None });
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
                    // Measure from the start tangent, not the difference of two
                    // global polar angles: subtraction loses shallow-arc sweeps.
                    let tangent = [d.cos(), d.sin()];
                    let relative = sub(point, e.start);
                    let along = dot(relative, tangent);
                    let radial = radius - turn.sign() * cross(tangent, relative);
                    if along != 0. || radial != 0. {
                        let delta = along.atan2(radial).rem_euclid(TAU);
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
                    spend(&mut budget)?;
                    let p = e.pose(lo);
                    let q = e.pose(hi);
                    let ab = sub(q.point, p.point);
                    let ab2 = dot(ab, ab);
                    let t = if ab2 > 0. { (dot(sub(point, p.point), ab) / ab2).clamp(0., 1.) } else { 0. };
                    let chord = [p.point[0] + t * ab[0], p.point[1] + t * ab[1]];
                    let lower = (distance(point, chord) - e.max_curvature() * (hi - lo).powi(2) / 8.).max(0.);
                    if lower >= best.map_or(f64::INFINITY, |v| v.distance) - 1e-9 {
                        continue;
                    }
                    let f_lo = dot(sub(p.point, point), [p.direction.cos(), p.direction.sin()]);
                    let f_hi = dot(sub(q.point, point), [q.direction.cos(), q.direction.sin()]);
                    let known = best.is_some_and(|v| v.element == index && v.root.is_some_and(|s| lo <= s && s <= hi));
                    // Only stationary points and element ends compete for the final answer;
                    // a chord parameter with nearly the same distance can have a poor station.
                    if f_lo <= 0. && f_hi >= 0. && !known {
                        let s = if f_lo == 0. {
                            lo
                        } else if f_hi == 0. {
                            hi
                        } else {
                            refine(e, point, lo, hi, &mut budget)?
                        };
                        consider_candidate(
                            &mut best,
                            Candidate {
                                distance: distance(e.pose(s).point, point),
                                station: a.start_station + (distance_along + s),
                                element: index,
                                root: Some(s),
                            },
                        );
                    }
                    if hi - lo <= 1e-8 {
                        let s = lo + t * (hi - lo);
                        consider_candidate(
                            &mut best,
                            Candidate {
                                distance: distance(e.pose(s).point, point),
                                station: a.start_station + (distance_along + s),
                                element: index,
                                root: None,
                            },
                        );
                        continue;
                    }
                    if lower >= best.map_or(f64::INFINITY, |v| v.distance) - 1e-9 {
                        continue;
                    }
                    let mid = (lo + hi) / 2.;
                    intervals.push((mid, hi));
                    intervals.push((lo, mid));
                }
            }
            distance_along += e.length;
        }
        let Candidate { distance, station, element: index, .. } = best.ok_or_else(|| error("INVALID_ALIGNMENT", "empty alignment"))?;
        let pose = self.at(station, 0.)?;
        let delta = sub(point, pose.point);
        let (sn, cs) = pose.direction.sin_cos();
        let lateral = cross([cs, sn], delta);
        let along = dot(delta, [cs, sn]);
        let end = a.start_station + self.length();
        let beyond = if station == a.start_station && along < 0. {
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

#[derive(Clone, Copy)]
struct Candidate {
    distance: f64,
    station: f64,
    element: usize,
    root: Option<f64>,
}
fn consider_candidate(best: &mut Option<Candidate>, next: Candidate) {
    if best.is_none_or(|b| next.distance < b.distance - 1e-9 || ((next.distance - b.distance).abs() <= 1e-9 && next.station < b.station)) {
        *best = Some(next);
    }
}
fn spend(budget: &mut usize) -> Result<()> {
    if *budget == 0 {
        return Err(error("NUMERICAL_LIMIT", "projection exceeded 200000 search/refinement steps"));
    }
    *budget -= 1;
    Ok(())
}
fn refine(e: &Element, point: [f64; 2], mut lo: f64, mut hi: f64, budget: &mut usize) -> Result<f64> {
    for _ in 0..64 {
        let mid = lo + (hi - lo) / 2.;
        if hi - lo <= 1e-9 || mid == lo || mid == hi {
            return Ok(mid);
        }
        spend(budget)?;
        let pose = e.pose(mid);
        let f = dot(sub(pose.point, point), [pose.direction.cos(), pose.direction.sin()]);
        if f == 0. {
            return Ok(mid);
        }
        if f < 0. {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Err(error("NUMERICAL_LIMIT", "spiral stationary-point refinement did not converge"))
}
