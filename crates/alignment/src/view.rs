use crate::{Alignment, Result, error, profile::Profiles};
#[derive(Clone, Debug)]
pub enum Mark {
    Line { a: [f64; 2], b: [f64; 2] },
    Text { at: [f64; 2], text: String },
    Polyline { layer: &'static str, points: Vec<[f64; 2]> },
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
pub fn draw(a: &Alignment, profiles: &Profiles, at: [f64; 2], exaggeration: f64) -> Result<Vec<Mark>> {
    profiles.validate(a)?;
    let length = a.length()?;
    if at.iter().any(|v| !v.is_finite()) || !exaggeration.is_finite() || exaggeration <= 0. {
        return Err(error("INVALID_PROFILE_VIEW", "finite origin and positive finite exaggeration required"));
    }
    if profiles.design.is_none() && profiles.ground.is_none() {
        return Err(error("NO_PROFILE", "alignment has no stored ground or design profile"));
    }
    let mut lines = Vec::new();
    if let Some(g) = &profiles.ground {
        for segment in &g.segments {
            lines.push(("PROFILE-GROUND", segment.clone()));
        }
    }
    if let Some(d) = &profiles.design {
        lines.push(("PROFILE-DESIGN", d.polyline(0.01 / exaggeration)?));
    }
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (_, points) in &lines {
        for p in points {
            low = low.min(p.elevation);
            high = high.max(p.elevation);
        }
    }
    let x_step = step(length);
    let text_height = (length / 100.).max(0.25);
    let z_step = step((high - low).max(20. * text_height / exaggeration));
    let base = (low / z_step).floor() * z_step;
    let top = (high / z_step).ceil() * z_step + if high == low { z_step } else { 0. };
    let height = (top - base) * exaggeration;
    if !height.is_finite() || height <= 0. {
        return Err(error("INVALID_PROFILE_VIEW", "view extent overflow"));
    }
    let mut marks = Vec::new();
    for i in 0..=10 {
        let x = f64::from(i) * x_step;
        if x > length {
            break;
        }
        marks.push(Mark::Line { a: [at[0] + x, at[1]], b: [at[0] + x, at[1] + height] });
        marks.push(Mark::Text { at: [at[0] + x, at[1] - text_height * 2.], text: a.format.display(a.start_station + x)? });
    }
    for i in 0..=12 {
        let z = base + f64::from(i) * z_step;
        if z > top {
            break;
        }
        let y = at[1] + (z - base) * exaggeration;
        marks.push(Mark::Line { a: [at[0], y], b: [at[0] + length, y] });
        marks.push(Mark::Text { at: [at[0] - text_height * 8., y], text: format!("{z:.3}") });
    }
    for (layer, points) in lines {
        let points =
            points.into_iter().map(|p| [at[0] + (p.station - a.start_station), at[1] + (p.elevation - base) * exaggeration]).collect::<Vec<_>>();
        if points.iter().flatten().any(|v| !v.is_finite()) {
            return Err(error("INVALID_PROFILE_VIEW", "view coordinates overflow"));
        }
        marks.push(Mark::Polyline { layer, points });
    }
    if marks.iter().any(|m| match m {
        Mark::Line { a, b } => a.iter().chain(b).any(|v| !v.is_finite()),
        Mark::Text { at, .. } => at.iter().any(|v| !v.is_finite()),
        _ => false,
    }) {
        return Err(error("INVALID_PROFILE_VIEW", "grid coordinates overflow"));
    }
    Ok(marks)
}
