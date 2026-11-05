//! Subsurface profiles: project borings onto a profile line and lay out their columns.
//! Owner: Codex (see `docs/agents/slice-6-subsurface.md`). Behaviour reference: AutoGIS
//! `subsurface_profile.py`.

use crate::{
    BoringSet,
    sticklog::{self, Log, MAX_ITEMS},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub boring_id: String,
    pub station: f64,
    pub offset: f64,
}

pub fn endpoints(
    set: &BoringSet,
    start: Option<[f64; 2]>,
    end: Option<[f64; 2]>,
    boring_a: Option<&str>,
    boring_b: Option<&str>,
) -> Result<([f64; 2], [f64; 2]), String> {
    if (start.is_some() || end.is_some()) && (boring_a.is_some() || boring_b.is_some()) {
        return Err("specify coordinates or boring ids, not both".into());
    }
    let lookup = |id: &str| -> Result<[f64; 2], String> {
        let b = set.borings.iter().find(|b| b.boring_id == id).ok_or_else(|| format!("unknown boring {id}"))?;
        match (b.easting, b.northing) {
            (Some(x), Some(y)) => Ok([x, y]),
            _ => Err(format!("boring {id} has no coordinates")),
        }
    };
    let pair = match (start, end, boring_a, boring_b) {
        (Some(a), Some(b), None, None) => (a, b),
        (None, None, Some(a), Some(b)) => (lookup(a)?, lookup(b)?),
        _ => return Err("specify both profile endpoints by coordinates or boring ids".into()),
    };
    project(pair.0, pair.0, pair.1)?;
    Ok(pair)
}

pub fn project(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> Result<(f64, f64), String> {
    if point.iter().chain(&a).chain(&b).any(|v| !v.is_finite()) {
        return Err("nonfinite profile coordinate".into());
    }
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if !length.is_finite() || length == 0.0 {
        return Err("coincident or overflowing endpoints".into());
    }
    let station = (point[0] - a[0]) * (dx / length) + (point[1] - a[1]) * (dy / length);
    let nearest = station.clamp(0.0, length);
    let offset = (point[0] - (a[0] + nearest * (dx / length))).hypot(point[1] - (a[1] + nearest * (dy / length)));
    if !station.is_finite() || !offset.is_finite() {
        return Err("projection overflow".into());
    }
    Ok((station, offset))
}

pub fn place(set: &BoringSet, a: [f64; 2], b: [f64; 2], tolerance: f64) -> Result<(Vec<Placement>, Vec<String>), String> {
    project(a, a, b)?;
    if !tolerance.is_finite() || tolerance < 0.0 || set.borings.len() > 1000 {
        return Err("invalid tolerance or too many borings".into());
    }
    let mut placements = Vec::new();
    let mut warnings = Vec::new();
    for boring in &set.borings {
        let (Some(x), Some(y)) = (boring.easting, boring.northing) else {
            warnings.push(format!("missing_coordinates: {}", boring.boring_id));
            continue;
        };
        let (station, offset) = project([x, y], a, b)?;
        if offset > tolerance {
            warnings.push(format!("boring_outside_tolerance: {}", boring.boring_id));
            continue;
        }
        placements.push(Placement { boring_id: boring.boring_id.clone(), station, offset });
    }
    placements.sort_by(|a, b| a.station.total_cmp(&b.station).then(a.boring_id.cmp(&b.boring_id)));
    Ok((placements, warnings))
}

pub fn draw(set: &BoringSet, a: [f64; 2], b: [f64; 2], tolerance: f64, at: [f64; 2], exaggeration: f64) -> Result<Log, String> {
    if !exaggeration.is_finite() || exaggeration <= 0.0 {
        return Err("invalid vertical exaggeration".into());
    }
    let (placements, warnings) = place(set, a, b, tolerance)?;
    let mut output = Log { warnings, ..Log::default() };
    for placement in placements {
        let ground = set.borings.iter().find(|r| r.boring_id == placement.boring_id).and_then(|r| r.ground_elevation);
        let Some(ground) = ground.filter(|g| g.is_finite()) else {
            output.warnings.push(format!("boring_missing_ground_elevation: {}", placement.boring_id));
            continue;
        };
        let log = sticklog::draw(set, &placement.boring_id, [at[0] + placement.station, at[1] + ground * exaggeration], exaggeration)?;
        if output.marks.len().saturating_add(log.marks.len()) > MAX_ITEMS {
            return Err("too many profile marks".into());
        }
        output.marks.extend(log.marks);
        output.warnings.extend(log.warnings);
        output.depth = output.depth.max(log.depth);
    }
    Ok(output)
}
