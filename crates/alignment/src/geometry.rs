use crate::{DIRECTION_TOLERANCE, MAX_ELEMENTS, MAX_POINTS, POSITION_TOLERANCE, Result, error};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    Left,
    Right,
}
impl Turn {
    pub(crate) fn sign(self) -> f64 {
        if self == Self::Left { 1. } else { -1. }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Kind {
    Line,
    Arc { radius: f64, turn: Turn },
    Spiral { radius_start: Option<f64>, radius_end: Option<f64>, turn: Turn },
}

/// Directions are radians CCW from +X. Positive offsets lie left of the tangent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub start: [f64; 2],
    pub direction: f64,
    pub length: f64,
    #[serde(flatten)]
    pub kind: Kind,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationFormat {
    #[default]
    Thousands,
    Hundreds,
}
impl StationFormat {
    pub fn display(self, station: f64) -> Result<String> {
        if !station.is_finite() {
            return Err(error("OUT_OF_RANGE", "station must be finite"));
        }
        let (base, scale, places, width) = match self {
            Self::Thousands => (1000., 1000., 3, 7),
            Self::Hundreds => (100., 100., 2, 5),
        };
        let mut major = (station.abs() / base).floor();
        let mut minor = ((station.abs() % base) * scale).round() / scale;
        if minor >= base {
            major += 1.;
            minor = 0.;
        }
        let sign = if station < 0. && (major > 0. || minor > 0.) { "-" } else { "" };
        Ok(format!("{sign}{major:.0}+{minor:0width$.places$}"))
    }
}
/// Valid stations are inclusive [start_station, start_station + length]. at() never extrapolates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    pub name: String,
    pub start_station: f64,
    #[serde(default)]
    pub format: StationFormat,
    pub elements: Vec<Element>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Pose {
    pub point: [f64; 2],
    pub direction: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JointIssue {
    pub index: usize,
    pub gap: f64,
    pub direction_break: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Beyond {
    Before,
    After,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Projection {
    pub station: f64,
    pub offset: f64,
    pub distance: f64,
    pub element: usize,
    pub beyond: Option<Beyond>,
}

pub(crate) fn angle_delta(a: f64, b: f64) -> f64 {
    ((a - b + PI).rem_euclid(TAU) - PI).abs()
}
pub(crate) fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub(crate) fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub(crate) fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
pub(crate) fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

impl Element {
    pub fn validate(&self) -> Result<()> {
        if self.start.iter().any(|x| !x.is_finite() || x.abs() > 1e9)
            || !self.direction.is_finite()
            || !self.length.is_finite()
            || self.length <= 0.
            || self.length > 1e7
        {
            return Err(error("INVALID_ELEMENT", "finite XY within 1e9, positive length <=1e7 and finite direction required"));
        }
        let radius = |r: f64| r.is_finite() && r > 0. && r <= 1e12;
        match self.kind {
            Kind::Line => {}
            Kind::Arc { radius: r, .. } if !radius(r) || self.length / r >= TAU => {
                return Err(error("INVALID_ELEMENT", "arc radius must be positive; sweep must be below 2*pi"));
            }
            Kind::Spiral { radius_start: a, radius_end: b, .. } if a == b || a.is_some_and(|r| !radius(r)) || b.is_some_and(|r| !radius(r)) => {
                return Err(error("INVALID_ELEMENT", "spiral needs unequal positive radii (null = infinite)"));
            }
            _ => {}
        }
        let (a, b) = self.curvatures();
        if !(a + b).is_finite() || (a + b).abs() * self.length / 2. > 16. * PI {
            return Err(error("NUMERICAL_LIMIT", "element turning exceeds 16*pi"));
        }
        Ok(())
    }
    pub(crate) fn curvatures(&self) -> (f64, f64) {
        match self.kind {
            Kind::Line => (0., 0.),
            Kind::Arc { radius, turn } => (turn.sign() / radius, turn.sign() / radius),
            Kind::Spiral { radius_start, radius_end, turn } => {
                (turn.sign() * radius_start.map_or(0., |r| 1. / r), turn.sign() * radius_end.map_or(0., |r| 1. / r))
            }
        }
    }
    pub(crate) fn max_curvature(&self) -> f64 {
        let (a, b) = self.curvatures();
        a.abs().max(b.abs())
    }
    pub fn at(&self, distance: f64) -> Result<Pose> {
        self.validate()?;
        if !distance.is_finite() || distance < 0. || distance > self.length {
            return Err(error("OUT_OF_RANGE", "distance outside element"));
        }
        let pose = self.pose(distance);
        if pose.point.iter().any(|x| !x.is_finite() || x.abs() > 1e9) {
            return Err(error("NUMERICAL_LIMIT", "evaluated XY outside 1e9"));
        }
        Ok(pose)
    }
    pub(crate) fn pose(&self, s: f64) -> Pose {
        let (k0, k1) = self.curvatures();
        let rate = (k1 - k0) / self.length;
        let direction = self.direction.rem_euclid(TAU);
        let theta = k0 * s + rate * s * s / 2.;
        let local = match self.kind {
            Kind::Line => [s, 0.],
            Kind::Arc { .. } => {
                // sin(theta)/k and (1-cos(theta))/k without small-angle cancellation.
                let half = theta / 2.;
                let sinc = if half.abs() < 1e-8 { 1. - half * half / 6. } else { half.sin() / half };
                [s * sinc * half.cos(), s * sinc * half.sin()]
            }
            Kind::Spiral { .. } => spiral_integral(k0, rate, s, self.max_curvature()),
        };
        let (sn, cs) = direction.sin_cos();
        Pose {
            point: [self.start[0] + cs * local[0] - sn * local[1], self.start[1] + sn * local[0] + cs * local[1]],
            direction: (direction + theta).rem_euclid(TAU),
        }
    }
}

fn spiral_integral(k: f64, rate: f64, length: f64, max_k: f64) -> [f64; 2] {
    // |phase| <= .25 in each local interval. The exponential Taylor remainder is
    // <= exp(.25)*.25^17/17! times total length; integrate locally before translation.
    let count = (length * max_k / 0.25).ceil().max(1.) as u32;
    let h = length / f64::from(count);
    let mut sum = [0.; 2];
    let mut compensation = [0.; 2];
    for i in 0..count {
        let s = f64::from(i) * h;
        let b = (k + rate * s) * h;
        let c = rate * h * h / 2.;
        let mut polynomial = [0.; 33];
        polynomial[0] = 1.;
        let mut integral = [1., 0.];
        for n in 1..=16usize {
            let mut next = [0.; 33];
            for (j, &v) in polynomial.iter().enumerate().take(2 * n - 1) {
                if let Some(slot) = next.get_mut(j + 1) {
                    *slot += v * b / (n as f64);
                }
                if let Some(slot) = next.get_mut(j + 2) {
                    *slot += v * c / (n as f64);
                }
            }
            let value: f64 = next.iter().enumerate().map(|(j, v)| v / ((j + 1) as f64)).sum();
            let (axis, sign) = match n % 4 {
                0 => (0, 1.),
                1 => (1, 1.),
                2 => (0, -1.),
                _ => (1, -1.),
            };
            if let Some(v) = integral.get_mut(axis) {
                *v += sign * value;
            }
            polynomial = next;
        }
        let (sn, cs) = (k * s + rate * s * s / 2.).sin_cos();
        let step = [h * (cs * integral[0] - sn * integral[1]), h * (sn * integral[0] + cs * integral[1])];
        for ((total, comp), v) in sum.iter_mut().zip(compensation.iter_mut()).zip(step) {
            let y = v - *comp;
            let t = *total + y;
            *comp = (t - *total) - y;
            *total = t;
        }
    }
    sum
}

impl Alignment {
    /// Invalid scalar/size data is an error; all geometric joint defects are returned together.
    pub fn validate(&self) -> Result<Vec<JointIssue>> {
        if self.name.is_empty()
            || self.name.len() > 256
            || self.name.chars().any(char::is_control)
            || !self.start_station.is_finite()
            || self.elements.is_empty()
            || self.elements.len() > MAX_ELEMENTS
        {
            return Err(error("INVALID_ALIGNMENT", "name/station invalid or element count outside 1..4096"));
        }
        let mut length = 0.;
        let mut issues = Vec::new();
        let mut previous: Option<Pose> = None;
        for (i, e) in self.elements.iter().enumerate() {
            e.validate().map_err(|mut err| {
                err.reason = format!("element {i}: {}", err.reason);
                err
            })?;
            if let Some(p) = previous {
                let gap = distance(p.point, e.start);
                let direction_break = angle_delta(p.direction, e.direction.rem_euclid(TAU));
                if gap > POSITION_TOLERANCE || direction_break > DIRECTION_TOLERANCE {
                    issues.push(JointIssue { index: i, gap, direction_break });
                }
            }
            previous = Some(e.at(e.length)?);
            length += e.length;
        }
        if !length.is_finite() || !(self.start_station + length).is_finite() || self.start_station + length <= self.start_station {
            return Err(error("NUMERICAL_LIMIT", "station extent overflows or cannot be represented at this start station"));
        }
        Ok(issues)
    }
    pub(crate) fn checked(&self) -> Result<()> {
        let issues = self.validate()?;
        if !issues.is_empty() {
            return Err(error("INVALID_JOINTS", format!("{issues:?}")));
        }
        Ok(())
    }
    pub fn length(&self) -> Result<f64> {
        self.checked()?;
        Ok(self.elements.iter().map(|e| e.length).sum())
    }
    pub fn at(&self, station: f64, offset: f64) -> Result<Pose> {
        self.checked()?;
        if !station.is_finite() || !offset.is_finite() {
            return Err(error("OUT_OF_RANGE", "station and offset must be finite"));
        }
        let mut distance = station - self.start_station;
        let total: f64 = self.elements.iter().map(|e| e.length).sum();
        if distance < 0. || distance > total {
            return Err(error("OUT_OF_RANGE", "station outside alignment"));
        }
        for (i, e) in self.elements.iter().enumerate() {
            if distance < e.length || i + 1 == self.elements.len() {
                let mut p = e.at(distance.min(e.length))?;
                p.point[0] -= offset * p.direction.sin();
                p.point[1] += offset * p.direction.cos();
                if p.point.iter().any(|x| !x.is_finite() || x.abs() > 1e9) {
                    return Err(error("NUMERICAL_LIMIT", "offset coordinate overflow"));
                }
                return Ok(p);
            }
            distance -= e.length;
        }
        Err(error("OUT_OF_RANGE", "station outside alignment"))
    }
    pub fn polyline(&self, tolerance: f64) -> Result<Vec<[f64; 2]>> {
        self.checked()?;
        if !tolerance.is_finite() || tolerance <= 0. {
            return Err(error("INVALID_TOLERANCE", "positive finite chord tolerance required"));
        }
        let mut out = Vec::new();
        for e in &self.elements {
            let k = e.max_curvature();
            let n = if k == 0. { 1. } else { (e.length / (8. * tolerance / k).sqrt()).ceil().max(1.) };
            if !n.is_finite() || n > MAX_POINTS as f64 || out.len().saturating_add(n as usize).saturating_add(1) > MAX_POINTS {
                return Err(error("POINT_LIMIT", "polyline exceeds 100000 vertices"));
            }
            for i in 0..(n as usize) {
                out.push(e.pose(e.length * (i as f64) / n).point);
            }
        }
        if let Some(e) = self.elements.last() {
            out.push(e.pose(e.length).point);
        }
        Ok(out)
    }
}
