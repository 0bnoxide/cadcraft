//! Sheet rectangles in model units. All search and containment work shares one budget.
use crate::{Alignment, Kind, POSITION_TOLERANCE, Result, ValidatedAlignment, error};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub const MAX_FRAMES: usize = 4096;
const MAX_WORK: usize = 2_000_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub start: f64,
    pub end: f64,
    pub paper_size: [f64; 2],
    pub scale: f64,
    /// None searches every admissible rounded station, longest first.
    pub spacing: Option<f64>,
    pub increment: f64,
    pub vertex_buffer: f64,
    pub offset: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub range: [f64; 2],
    pub center: [f64; 2],
    /// First rectangle edge, counterclockwise from +X.
    pub angle: f64,
    pub size: [f64; 2],
    pub half_chord: f64,
    pub offset: f64,
}
impl Frame {
    pub fn corners(&self) -> [[f64; 2]; 4] {
        let (sn, cs) = self.angle.sin_cos();
        [[-1., -1.], [1., -1.], [1., 1.], [-1., 1.]].map(|[x, y]| {
            [
                self.center[0] + x * self.size[0] / 2. * cs - y * self.size[1] / 2. * sn,
                self.center[1] + x * self.size[0] / 2. * sn + y * self.size[1] / 2. * cs,
            ]
        })
    }
    pub fn validate(&self) -> Result<()> {
        if self.center.iter().chain(&self.size).chain(&self.range).chain([&self.angle, &self.offset, &self.half_chord]).any(|v| !v.is_finite())
            || self.size.iter().any(|v| *v <= 0. || *v > 1e9)
            || self.half_chord <= 0.
            || self.range[1] - self.range[0] < POSITION_TOLERANCE
            || self.corners().iter().flatten().any(|v| v.abs() > 1e9 || !v.is_finite())
        {
            return Err(error("INVALID_FRAME", "invalid rectangle geometry or range"));
        }
        Ok(())
    }
}
fn spend(work: &mut usize) -> Result<()> {
    if *work == 0 {
        return Err(error("NUMERICAL_LIMIT", "frame search exceeds bounded work"));
    }
    *work -= 1;
    Ok(())
}
fn rounded(station: f64, increment: f64) -> Result<f64> {
    let n = (station / increment).floor();
    let result = n * increment;
    if !result.is_finite() {
        return Err(error("NUMERICAL_LIMIT", "station rounding overflow"));
    }
    Ok(result)
}
fn interior(view: &ValidatedAlignment<'_>, mut station: f64, o: &Options, work: &mut usize) -> Result<f64> {
    station = rounded(station, o.increment)?;
    if o.vertex_buffer == 0. {
        return Ok(station);
    }
    loop {
        let mut next = station;
        for distance in view.cumulative.iter().skip(1).take(view.alignment.elements.len().saturating_sub(1)) {
            spend(work)?;
            let joint = view.alignment.start_station + distance;
            if (station - joint).abs() < o.vertex_buffer {
                next = next.min(rounded(joint - o.vertex_buffer, o.increment)?);
            }
        }
        if next == station {
            return Ok(station);
        }
        station = next;
    }
}
fn rectangle(view: &ValidatedAlignment<'_>, start: f64, end: f64, o: &Options) -> Result<Frame> {
    let a = view.at(start, 0.)?.point;
    let b = view.at(end, 0.)?.point;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let chord = dx.hypot(dy);
    if chord < POSITION_TOLERANCE {
        return Err(error("DEGENERATE_CHORD", format!("station {end}: coincident chord ends")));
    }
    let angle = dy.atan2(dx);
    let f = Frame {
        range: [start, end],
        center: [a[0] + dx / 2. - o.offset * angle.sin(), a[1] + dy / 2. + o.offset * angle.cos()],
        angle,
        size: o.paper_size.map(|v| v * o.scale),
        half_chord: chord / 2.,
        offset: o.offset,
    };
    f.validate()?;
    Ok(f)
}
pub fn tile(a: &Alignment, o: &Options) -> Result<Vec<Frame>> {
    let view = a.validated()?;
    if o.paper_size.iter().chain([&o.scale, &o.increment]).any(|v| !v.is_finite() || *v <= 0.)
        || o.spacing.is_some_and(|v| !v.is_finite() || v <= 0.)
        || !o.vertex_buffer.is_finite()
        || o.vertex_buffer < 0.
        || !o.offset.is_finite()
        || !o.start.is_finite()
        || !o.end.is_finite()
        || o.start < a.start_station
        || o.end > a.start_station + view.length()
        || o.end - o.start < POSITION_TOLERANCE
    {
        return Err(error("INVALID_FRAME_SETTINGS", "positive finite size, scale, spacing, increment and valid range required"));
    }
    if o.spacing.is_none() && (!(o.end / o.increment).is_finite() || (o.end - o.start) / o.increment > 100_000.) {
        return Err(error("NUMERICAL_LIMIT", "automatic search has more than 100000 candidate stations"));
    }
    let mut work = MAX_WORK;
    let mut frames = Vec::new();
    let mut start = o.start;
    while start < o.end {
        if frames.len() == MAX_FRAMES {
            return Err(error("FRAME_LIMIT", "more than 4096 frames"));
        }
        let mut end = match o.spacing {
            Some(spacing) if o.end - start > spacing => interior(&view, start + spacing, o, &mut work)?,
            _ => o.end,
        };
        loop {
            spend(&mut work)?;
            if end - start < POSITION_TOLERANCE {
                return Err(error("NO_PROGRESS", format!("no admissible matchline after station {start}")));
            }
            let candidate = rectangle(&view, start, end, o).and_then(|f| {
                check(&view, &f, &mut work)?;
                Ok(f)
            });
            match candidate {
                Ok(frame) => {
                    frames.push(frame);
                    start = end;
                    break;
                }
                Err(e) if o.spacing.is_none() && matches!(e.code, "FRAME_OUTSIDE" | "DEGENERATE_CHORD") => {
                    let previous = if end == o.end { rounded(end, o.increment)? } else { end - o.increment };
                    let previous = if previous >= end { previous - o.increment } else { previous };
                    if previous >= end {
                        return Err(error("NO_PROGRESS", "increment cannot advance station"));
                    }
                    end = interior(&view, previous, o, &mut work)?;
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(frames)
}
pub fn contains(a: &Alignment, f: &Frame) -> Result<()> {
    check(&a.validated()?, f, &mut MAX_WORK.clone())
}
fn check(view: &ValidatedAlignment<'_>, f: &Frame, work: &mut usize) -> Result<()> {
    f.validate()?;
    let start = view.alignment.start_station;
    if f.range[0] < start || f.range[1] > start + view.length() {
        return Err(error("INVALID_FRAME", "station range outside alignment"));
    }
    let (sn, cs) = f.angle.sin_cos();
    let local = |point: [f64; 2]| {
        let x = point[0] - f.center[0];
        let y = point[1] - f.center[1];
        [x * cs + y * sn, -x * sn + y * cs]
    };
    let inside =
        |p: [f64; 2], pad: f64| p[0].abs() + pad <= f.size[0] / 2. + POSITION_TOLERANCE && p[1].abs() + pad <= f.size[1] / 2. + POSITION_TOLERANCE;
    for (index, e) in view.alignment.elements.iter().enumerate() {
        spend(work)?;
        let base = start + view.cumulative.get(index).copied().ok_or_else(|| error("INVALID_ALIGNMENT", "missing prefix"))?;
        let lo = (f.range[0] - base).max(0.);
        let hi = (f.range[1] - base).min(e.length);
        if lo > hi {
            continue;
        }
        let test = |s: f64| -> Result<()> {
            if !inside(local(e.pose(s).point), 0.) {
                return Err(error("FRAME_OUTSIDE", format!("alignment leaves frame at station {}", base + s)));
            }
            Ok(())
        };
        test(lo)?;
        test(hi)?;
        match e.kind {
            Kind::Line => {}
            Kind::Arc { .. } => {
                let curvature = e.curvatures().0;
                for axis in [f.angle, f.angle + FRAC_PI_2] {
                    for turn in -4..=8 {
                        let s = (axis + FRAC_PI_2 + f64::from(turn) * PI - e.direction.rem_euclid(TAU)) / curvature;
                        if s > lo && s < hi {
                            spend(work)?;
                            test(s)?;
                        }
                    }
                }
            }
            Kind::Spiral { .. } => {
                let mut intervals = vec![(lo, hi)];
                while let Some((a, b)) = intervals.pop() {
                    spend(work)?;
                    let pad = e.max_curvature() * (b - a).powi(2) / 8.;
                    if inside(local(e.pose(a).point), pad) && inside(local(e.pose(b).point), pad) {
                        continue;
                    }
                    let mid = a + (b - a) / 2.;
                    test(mid)?;
                    if mid == a || mid == b {
                        return Err(error("NUMERICAL_LIMIT", "cannot certify spiral containment"));
                    }
                    intervals.extend([(a, mid), (mid, b)]);
                }
            }
        }
    }
    Ok(())
}
