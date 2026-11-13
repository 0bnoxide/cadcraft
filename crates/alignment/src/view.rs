//! Ranged profile figures. Elevation windows clip curves and report every clipped span.
use crate::{
    Alignment, Result, error,
    profile::{Profiles, Sample},
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug)]
pub enum Mark {
    Line { a: [f64; 2], b: [f64; 2] },
    Text { at: [f64; 2], text: String },
    Polyline { layer: &'static str, points: Vec<[f64; 2]> },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Ground,
    Design,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Center {
    Highest,
    Median,
    Lowest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustment {
    pub height: f64,
    pub profile: Profile,
    pub center: Center,
    pub increment: f64,
    pub start_buffer: f64,
    pub end_buffer: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub range: [f64; 2],
    pub elevation: [f64; 2],
    pub clamped: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Clipped {
    pub profile: &'static str,
    pub range: [f64; 2],
}
pub struct Figure {
    pub marks: Vec<Mark>,
    pub clipped: Vec<Clipped>,
    pub window: Window,
}
fn step(range: f64) -> f64 {
    let raw = (range / 10.).max(1e-6);
    let power = 10f64.powf(raw.log10().floor());
    let ratio = raw / power;
    let multiple = if ratio <= 1. {
        1.
    } else if ratio <= 2. {
        2.
    } else if ratio <= 5. {
        5.
    } else {
        10.
    };
    multiple * power
}
fn range_check(a: &Alignment, p: &Profiles, range: [f64; 2]) -> Result<()> {
    let view = a.validated()?;
    p.validate_on(&view)?;
    if range.iter().any(|v| !v.is_finite()) || range[0] < a.start_station || range[1] > a.start_station + view.length() || range[1] <= range[0] {
        return Err(error("INVALID_PROFILE_VIEW", "station range outside alignment or empty"));
    }
    Ok(())
}
fn interpolate(a: Sample, b: Sample, station: f64) -> Sample {
    if station == a.station {
        a
    } else if station == b.station {
        b
    } else {
        Sample { station, elevation: a.elevation + (b.elevation - a.elevation) * ((station - a.station) / (b.station - a.station)) }
    }
}
fn slice(points: &[Sample], range: [f64; 2]) -> Vec<Sample> {
    if let [p] = points {
        return if p.station >= range[0] && p.station <= range[1] { vec![*p] } else { Vec::new() };
    }
    let mut out = Vec::new();
    for pair in points.windows(2) {
        let [a, b] = pair else { continue };
        if b.station < range[0] || a.station > range[1] {
            continue;
        }
        let a1 = interpolate(*a, *b, a.station.max(range[0]));
        let b1 = interpolate(*a, *b, b.station.min(range[1]));
        if out.last().is_none_or(|p: &Sample| p.station != a1.station) {
            out.push(a1);
        }
        if b1.station != a1.station {
            out.push(b1);
        }
    }
    out
}
fn lines(p: &Profiles, range: [f64; 2], tolerance: f64) -> Result<Vec<(&'static str, Vec<Sample>)>> {
    let mut out = Vec::new();
    if let Some(g) = &p.ground {
        for segment in &g.segments {
            let points = slice(segment, range);
            if !points.is_empty() {
                out.push(("ground", points));
            }
        }
    }
    if let Some(d) = &p.design {
        let mut points = d.polyline(tolerance)?;
        points.extend(d.extrema()?);
        // Exact range ends precede chord interpolation at a curved boundary.
        for station in range {
            if let Ok(v) = d.at(station) {
                points.push(Sample { station, elevation: v.elevation });
            }
        }
        points.sort_by(|a, b| a.station.total_cmp(&b.station));
        points.dedup_by(|a, b| a.station == b.station);
        let points = slice(&points, range);
        if !points.is_empty() {
            out.push(("design", points));
        }
    }
    Ok(out)
}
/// Median is the vertex median including interpolated range ends and design extrema,
/// with an even-count mean; it is deliberately not weighted by station length.
pub fn adjust(a: &Alignment, p: &Profiles, range: [f64; 2], settings: &Adjustment) -> Result<Window> {
    range_check(a, p, range)?;
    if [settings.height, settings.increment].iter().any(|v| !v.is_finite() || *v <= 0.)
        || [settings.start_buffer, settings.end_buffer].iter().any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(error("INVALID_PROFILE_VIEW", "positive height/increment and nonnegative finite buffers required"));
    }
    let requested = [range[0] - settings.start_buffer, range[1] + settings.end_buffer];
    if requested.iter().any(|v| !v.is_finite()) {
        return Err(error("INVALID_PROFILE_VIEW", "buffer overflow"));
    }
    let range = [requested[0].max(a.start_station), requested[1].min(a.start_station + a.length()?)];
    let segments = match settings.profile {
        Profile::Ground => p.ground.as_ref().map(|g| g.segments.iter().map(|s| slice(s, range)).collect::<Vec<_>>()).unwrap_or_default(),
        Profile::Design => {
            let mut points = Vec::new();
            if let Some(d) = &p.design {
                points = d.vertices()?;
                points.extend(d.extrema()?);
                for station in range {
                    if let Ok(v) = d.at(station) {
                        points.push(Sample { station, elevation: v.elevation });
                    }
                }
                points.retain(|p| p.station >= range[0] && p.station <= range[1]);
                points.sort_by(|a, b| a.station.total_cmp(&b.station));
                points.dedup_by(|a, b| a.station == b.station);
            }
            vec![points]
        }
    };
    let mut values = Vec::new();
    let mut covered = range[0];
    for points in segments.iter().filter(|p| !p.is_empty()) {
        let first = points.first().ok_or_else(|| error("NO_PROFILE", "empty profile"))?;
        if first.station > covered {
            return Err(error("PROFILE_GAP", "selected profile has a gap in the buffered range"));
        }
        covered = points.last().map_or(covered, |p| p.station);
        values.extend(points.iter().map(|v| v.elevation));
    }
    if covered < range[1] || values.is_empty() {
        return Err(error("PROFILE_GAP", "selected profile does not cover the buffered range"));
    }
    values.sort_by(f64::total_cmp);
    let value = match settings.center {
        Center::Highest => values.last().copied(),
        Center::Lowest => values.first().copied(),
        Center::Median => {
            let i = values.len() / 2;
            values.get(i).copied().and_then(|b| if values.len().is_multiple_of(2) { values.get(i - 1).map(|a| a / 2. + b / 2.) } else { Some(b) })
        }
    }
    .ok_or_else(|| error("NO_PROFILE", "no selected elevations"))?;
    let base = ((value - settings.height / 2.) / settings.increment).floor() * settings.increment;
    let elevation = [base, base + settings.height];
    if elevation.iter().any(|v| !v.is_finite()) || elevation[1] <= elevation[0] {
        return Err(error("INVALID_PROFILE_VIEW", "elevation window overflow"));
    }
    Ok(Window { range, elevation, clamped: range != requested })
}
pub fn draw(a: &Alignment, p: &Profiles, at: [f64; 2], exaggeration: f64) -> Result<Vec<Mark>> {
    Ok(draw_range(a, p, at, exaggeration, [a.start_station, a.start_station + a.length()?], None)?.marks)
}
pub fn draw_range(a: &Alignment, p: &Profiles, at: [f64; 2], exaggeration: f64, range: [f64; 2], elevation: Option<[f64; 2]>) -> Result<Figure> {
    range_check(a, p, range)?;
    if at.iter().any(|v| !v.is_finite()) || !exaggeration.is_finite() || exaggeration <= 0. {
        return Err(error("INVALID_PROFILE_VIEW", "finite origin and positive finite exaggeration required"));
    }
    let curves = lines(p, range, 0.01 / exaggeration)?;
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (_, points) in &curves {
        for point in points {
            low = low.min(point.elevation);
            high = high.max(point.elevation);
        }
    }
    if !low.is_finite() || !high.is_finite() {
        return Err(error("NO_PROFILE", "no profile in the requested range"));
    }
    let length = range[1] - range[0];
    let text_height = (length / 100.).max(0.25);
    let z_step = step((high - low).max(20. * text_height / exaggeration));
    let [base, top] = elevation.unwrap_or([(low / z_step).floor() * z_step, (high / z_step).ceil() * z_step + if high == low { z_step } else { 0. }]);
    let height = (top - base) * exaggeration;
    if !base.is_finite() || !top.is_finite() || !height.is_finite() || height <= 0. {
        return Err(error("INVALID_PROFILE_VIEW", "invalid elevation window"));
    }
    let mut marks = Vec::new();
    let x_step = step(length);
    let first = (range[0] / x_step).ceil() * x_step;
    if !first.is_finite() {
        return Err(error("INVALID_PROFILE_VIEW", "grid overflow"));
    }
    for i in 0..=12 {
        let station = first + f64::from(i) * x_step;
        if station > range[1] {
            break;
        }
        let x = at[0] + station - range[0];
        marks.push(Mark::Line { a: [x, at[1]], b: [x, at[1] + height] });
        marks.push(Mark::Text { at: [x, at[1] - text_height * 2.], text: a.format.display(station)? });
    }
    let grid_step = step(top - base);
    let first = (base / grid_step).ceil() * grid_step;
    for i in 0..=12 {
        let z = first + f64::from(i) * grid_step;
        if z > top {
            break;
        }
        let y = at[1] + (z - base) * exaggeration;
        marks.push(Mark::Line { a: [at[0], y], b: [at[0] + length, y] });
        marks.push(Mark::Text { at: [at[0] - text_height * 8., y], text: format!("{z:.3}") });
    }
    let mut clipped: Vec<Clipped> = Vec::new();
    let xy = |p: Sample| [at[0] + p.station - range[0], at[1] + (p.elevation - base) * exaggeration];
    for (name, points) in curves {
        let layer = if name == "ground" { "PROFILE-GROUND" } else { "PROFILE-DESIGN" };
        let mut run = Vec::new();
        for pair in points.windows(2) {
            let [a, b] = pair else { continue };
            let mut cuts = vec![*a];
            for z in [base, top] {
                let t = (z - a.elevation) / (b.elevation - a.elevation);
                if t > 0. && t < 1. {
                    cuts.push(Sample { station: a.station + t * (b.station - a.station), elevation: z });
                }
            }
            cuts.push(*b);
            cuts.sort_by(|a, b| a.station.total_cmp(&b.station));
            for pair in cuts.windows(2) {
                let [a, b] = pair else { continue };
                let mid = a.elevation / 2. + b.elevation / 2.;
                if mid < base || mid > top {
                    if !run.is_empty() {
                        marks.push(Mark::Polyline { layer, points: std::mem::take(&mut run) });
                    }
                    if let Some(last) = clipped.last_mut().filter(|v| v.profile == name && v.range[1] == a.station) {
                        last.range[1] = b.station;
                    } else {
                        clipped.push(Clipped { profile: name, range: [a.station, b.station] });
                    }
                } else {
                    if run.last() != Some(&xy(*a)) {
                        run.push(xy(*a));
                    }
                    run.push(xy(*b));
                }
            }
        }
        if let [point] = points.as_slice() {
            if point.elevation >= base && point.elevation <= top {
                run.push(xy(*point));
            } else {
                clipped.push(Clipped { profile: name, range: [point.station, point.station] });
            }
        }
        if !run.is_empty() {
            marks.push(Mark::Polyline { layer, points: run });
        }
    }
    if marks.iter().any(|m| match m {
        Mark::Line { a, b } => a.iter().chain(b).any(|v| !v.is_finite()),
        Mark::Text { at, .. } => at.iter().any(|v| !v.is_finite()),
        Mark::Polyline { points, .. } => points.iter().flatten().any(|v| !v.is_finite()),
    }) {
        return Err(error("INVALID_PROFILE_VIEW", "figure coordinates overflow"));
    }
    Ok(Figure { marks, clipped, window: Window { range, elevation: [base, top], clamped: false } })
}
