use crate::{Alignment, Result, ValidatedAlignment, error};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pvi {
    pub station: f64,
    pub elevation: f64,
    #[serde(default)]
    pub curve_length: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignProfile {
    pub name: String,
    pub pvis: Vec<Pvi>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub station: f64,
    pub elevation: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundProfile {
    pub name: String,
    pub segments: Vec<Vec<Sample>>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    #[serde(default)]
    pub design: Option<DesignProfile>,
    #[serde(default)]
    pub ground: Option<GroundProfile>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Vertical {
    pub elevation: f64,
    pub grade: f64,
}
impl DesignProfile {
    pub fn validate(&self, a: &Alignment) -> Result<()> {
        self.validate_on(&a.validated()?)
    }
    fn validate_on(&self, view: &ValidatedAlignment<'_>) -> Result<()> {
        self.curves()?;
        let a = view.alignment;
        let end = a.start_station + view.length();
        if self.pvis.first().is_none_or(|p| p.station < a.start_station) || self.pvis.last().is_none_or(|p| p.station > end) {
            return Err(error("INVALID_PROFILE", "PVIs outside horizontal alignment"));
        }
        Ok(())
    }
    fn curves(&self) -> Result<Vec<Curve>> {
        if !valid_name(&self.name) || self.pvis.len() < 2 || self.pvis.len() > crate::MAX_ELEMENTS {
            return Err(error("INVALID_PROFILE", "profile name or PVI count invalid"));
        }
        for p in &self.pvis {
            if !p.station.is_finite()
                || !p.elevation.is_finite()
                || p.elevation.abs() > 1e9
                || p.curve_length.is_some_and(|l| !l.is_finite() || l <= 0.)
            {
                return Err(error("INVALID_PROFILE", "finite PVIs and positive curve lengths required"));
            }
        }
        if self.pvis.first().is_some_and(|p| p.curve_length.is_some()) || self.pvis.last().is_some_and(|p| p.curve_length.is_some()) {
            return Err(error("INVALID_PROFILE", "endpoint PVIs cannot have curves"));
        }
        for pair in self.pvis.windows(2) {
            if let [a, b] = pair
                && (b.station <= a.station || !grade(a, b).is_finite())
            {
                return Err(error("INVALID_PROFILE", "PVI stations must increase and grades must be finite"));
            }
        }
        let mut curves = Vec::new();
        let mut previous = f64::NEG_INFINITY;
        for triple in self.pvis.windows(3) {
            let [a, p, b] = triple else { continue };
            let Some(length) = p.curve_length else { continue };
            let start = p.station - length / 2.;
            let end = p.station + length / 2.;
            if start < a.station || end > b.station || start < previous || start >= end {
                return Err(error("OVERLAPPING_CURVES", "PVC/PVT outside adjacent spans or curves overlap"));
            }
            let g1 = grade(a, p);
            let g2 = grade(p, b);
            let z = p.elevation - g1 * length / 2.;
            if !z.is_finite() || !(g2 - g1).is_finite() {
                return Err(error("INVALID_PROFILE", "curve arithmetic overflow"));
            }
            curves.push(Curve { start, end, z, g1, g2 });
            previous = end;
        }
        Ok(curves)
    }
    pub fn at(&self, station: f64) -> Result<Vertical> {
        let curves = self.curves()?;
        self.evaluate(station, &curves)
    }
    fn evaluate(&self, station: f64, curves: &[Curve]) -> Result<Vertical> {
        let first = self.pvis.first().ok_or_else(|| error("INVALID_PROFILE", "no PVIs"))?;
        let last = self.pvis.last().ok_or_else(|| error("INVALID_PROFILE", "no PVIs"))?;
        if !station.is_finite() || station < first.station || station > last.station {
            return Err(error("OUT_OF_RANGE", "station outside design profile"));
        }
        if let Some(c) = curves.iter().rev().find(|c| station >= c.start && station <= c.end) {
            return c.at(station);
        }
        for pair in self.pvis.windows(2).rev() {
            if let [a, b] = pair
                && station >= a.station
                && station <= b.station
            {
                let g = grade(a, b);
                return finite_vertical(a.elevation + g * (station - a.station), g);
            }
        }
        Err(error("OUT_OF_RANGE", "station outside design profile"))
    }
    pub fn polyline(&self, tolerance: f64) -> Result<Vec<Sample>> {
        if !tolerance.is_finite() || tolerance <= 0. {
            return Err(error("INVALID_TOLERANCE", "positive finite profile chord tolerance required"));
        }
        let curves = self.curves()?;
        let cuts = self.vertex_stations(&curves);
        let mut out = Vec::new();
        for pair in cuts.windows(2) {
            let [lo, hi] = pair else { continue };
            let mid = (lo + hi) / 2.;
            let change = curves.iter().find(|c| mid >= c.start && mid <= c.end).map_or(0., |c| ((c.g2 - c.g1) / (c.end - c.start)).abs());
            let n = if change == 0. { 1. } else { ((hi - lo) / (8. * tolerance / change).sqrt()).ceil().max(1.) };
            if !n.is_finite() || n > crate::MAX_POINTS as f64 || out.len().saturating_add(n as usize + 1) > crate::MAX_POINTS {
                return Err(error("POINT_LIMIT", "design profile exceeds 100000 vertices"));
            }
            for i in 0..=(n as usize) {
                let station = subdivision_station(*lo, *hi, i, n as usize);
                if out.last().is_some_and(|p: &Sample| p.station == station) {
                    continue;
                }
                out.push(Sample { station, elevation: self.evaluate(station, &curves)?.elevation });
            }
        }
        Ok(out)
    }
    fn vertex_stations(&self, curves: &[Curve]) -> Vec<f64> {
        let mut cuts = self.pvis.iter().map(|p| p.station).collect::<Vec<_>>();
        for c in curves {
            cuts.extend([c.start, c.end]);
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        cuts
    }
    /// Evaluated PVIs and PVC/PVT endpoints, without rendering subdivisions.
    pub fn vertices(&self) -> Result<Vec<Sample>> {
        let curves = self.curves()?;
        self.vertex_stations(&curves)
            .into_iter()
            .map(|station| Ok(Sample { station, elevation: self.evaluate(station, &curves)?.elevation }))
            .collect()
    }
    pub fn extrema(&self) -> Result<Vec<Sample>> {
        let mut points = Vec::new();
        for c in self.curves()? {
            if c.g1 != c.g2 {
                let x = -c.g1 * (c.end - c.start) / (c.g2 - c.g1);
                if x >= 0. && x <= c.end - c.start {
                    let station = c.start + x;
                    points.push(Sample { station, elevation: c.at(station)?.elevation });
                }
            }
        }
        Ok(points)
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
}
fn grade(a: &Pvi, b: &Pvi) -> f64 {
    (b.elevation - a.elevation) / (b.station - a.station)
}
pub(crate) fn subdivision_station(lo: f64, hi: f64, i: usize, count: usize) -> f64 {
    // Multiplication/division can round an inclusive endpoint outside its interval.
    if i == 0 {
        lo
    } else if i == count {
        hi
    } else {
        lo + (hi - lo) * (i as f64) / (count as f64)
    }
}
fn finite_vertical(elevation: f64, grade: f64) -> Result<Vertical> {
    if !elevation.is_finite() || !grade.is_finite() {
        return Err(error("INVALID_PROFILE", "profile arithmetic overflow"));
    }
    Ok(Vertical { elevation, grade })
}
struct Curve {
    start: f64,
    end: f64,
    z: f64,
    g1: f64,
    g2: f64,
}
impl Curve {
    fn at(&self, s: f64) -> Result<Vertical> {
        let x = s - self.start;
        let change = (self.g2 - self.g1) / (self.end - self.start);
        finite_vertical(self.z + self.g1 * x + change * x * x / 2., self.g1 + change * x)
    }
}
impl GroundProfile {
    pub fn validate(&self, a: &Alignment) -> Result<()> {
        self.validate_on(&a.validated()?)
    }
    pub(crate) fn validate_on(&self, view: &ValidatedAlignment<'_>) -> Result<()> {
        let a = view.alignment;
        let end = a.start_station + view.length();
        let mut previous = f64::NEG_INFINITY;
        let mut count = 0usize;
        if !valid_name(&self.name) || self.segments.is_empty() || self.segments.len() > crate::MAX_POINTS {
            return Err(error("INVALID_PROFILE", "ground profile name or segment count invalid"));
        }
        for segment in &self.segments {
            count = count.saturating_add(segment.len());
            if count > crate::MAX_POINTS || segment.is_empty() {
                return Err(error("POINT_LIMIT", "ground profile needs 1..100000 total samples"));
            }
            for (i, p) in segment.iter().enumerate() {
                if !p.station.is_finite()
                    || !p.elevation.is_finite()
                    || p.station < a.start_station
                    || p.station > end
                    || p.station < previous
                    || (i > 0 && p.station == previous)
                {
                    return Err(error("INVALID_PROFILE", "ground samples must be finite, ordered and inside the alignment"));
                }
                previous = p.station;
            }
        }
        Ok(())
    }
}
impl Profiles {
    pub fn validate(&self, a: &Alignment) -> Result<()> {
        self.validate_on(&a.validated()?)
    }
    pub fn validate_on(&self, a: &ValidatedAlignment<'_>) -> Result<()> {
        if let Some(d) = &self.design {
            d.validate_on(a)?;
        }
        if let Some(g) = &self.ground {
            g.validate_on(a)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Kind, StationFormat, geometry::VALIDATION_CALLS};
    #[test]
    fn profile_checks_reuse_the_validated_alignment() {
        let a = Alignment {
            name: "profile batch".into(),
            start_station: 0.,
            format: StationFormat::Thousands,
            elements: vec![Element { start: [0., 0.], direction: 0., length: 10., kind: Kind::Line }],
        };
        let mut profiles = Profiles {
            design: Some(DesignProfile {
                name: "Design".into(),
                pvis: vec![Pvi { station: 0., elevation: 0., curve_length: None }, Pvi { station: 10., elevation: 1., curve_length: None }],
            }),
            ground: Some(GroundProfile {
                name: "Ground".into(),
                segments: vec![vec![Sample { station: 0., elevation: 0. }, Sample { station: 10., elevation: 0. }]],
            }),
        };
        VALIDATION_CALLS.with(|n| n.set(0));
        let view = a.validated().unwrap();
        profiles.validate_on(&view).unwrap();
        profiles.validate_on(&view).unwrap();
        assert_eq!(VALIDATION_CALLS.with(|n| n.get()), 1);
        profiles.design.as_mut().unwrap().pvis[1].station = 10f64.next_up();
        assert_eq!(profiles.validate_on(&view).unwrap_err().code, "INVALID_PROFILE");
    }
}
