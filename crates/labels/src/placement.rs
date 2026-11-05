//! Collision-aware placement of labels and callouts around their anchors. Reference: AutoGIS
//! `callout_collision.py`. Owner: Codex (see `docs/agents/slice-4-labels.md`).

use crate::{Pt, Rect};
use serde::{Deserialize, Serialize};

pub const MAX_LABELS: usize = 2048;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub anchor: Pt,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub origin: Option<Pt>,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Placed {
    pub index: usize,
    pub rect: Rect,
    pub overlap: f64,
    pub hits: Vec<usize>,
}

pub fn intersection(a: Rect, b: Rect) -> f64 {
    (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0.0) * (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0.0)
}

pub fn offsets(w: f64, h: f64, gap: f64) -> [Pt; 8] {
    [(gap, gap), (-w - gap, gap), (gap, -h - gap), (-w - gap, -h - gap), (gap, -h / 2.0), (-w - gap, -h / 2.0), (-w / 2.0, gap), (-w / 2.0, -h - gap)]
        .map(|(x, y)| Pt { x, y })
}

/// Locked origins claim space first; ties retain input order. Boundary containment is mandatory.
pub fn place(requests: &[Request], obstacles: &[Rect], boundary: Option<Rect>, gap: f64, point_buffer: f64) -> Result<Vec<Placed>, String> {
    if requests.len() > MAX_LABELS
        || obstacles.len() > MAX_LABELS
        || !gap.is_finite()
        || gap < 0.0
        || !point_buffer.is_finite()
        || point_buffer < 0.0
        || boundary.is_some_and(|b| !b.is_valid())
        || obstacles.iter().any(|b| !b.is_valid())
    {
        return Err("invalid placement options or too many labels/obstacles".into());
    }
    let mut buffers = Vec::new();
    for r in requests {
        if Rect::from_corner(r.anchor, r.width, r.height).is_none()
            || r.width <= 0.0
            || r.height <= 0.0
            || r.origin.is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
            || (r.locked && r.origin.is_none())
        {
            return Err("invalid label dimensions, anchor or locked origin".into());
        }
        let rect = Rect::from_corner(Pt { x: r.anchor.x - point_buffer, y: r.anchor.y - point_buffer }, point_buffer * 2.0, point_buffer * 2.0)
            .ok_or("point buffer overflow")?;
        buffers.push(rect);
    }
    let mut order: Vec<_> = requests.iter().enumerate().collect();
    order.sort_by(|(a, ar), (b, br)| br.locked.cmp(&ar.locked).then_with(|| br.anchor.y.total_cmp(&ar.anchor.y)).then(a.cmp(b)));
    let mut out: Vec<Placed> = Vec::new();
    for (index, r) in order {
        let mut candidates: Vec<Pt> = r.origin.into_iter().collect();
        if !r.locked {
            candidates.extend(offsets(r.width, r.height, gap).map(|p| Pt { x: r.anchor.x + p.x, y: r.anchor.y + p.y }));
        }
        let mut best: Option<Placed> = None;
        for mut at in candidates {
            if let Some(b) = boundary {
                if r.width > b.width() || r.height > b.height() {
                    return Err("label does not fit boundary".into());
                }
                if !r.locked {
                    at.x = at.x.clamp(b.min_x, b.max_x - r.width);
                    at.y = at.y.clamp(b.min_y, b.max_y - r.height);
                }
            }
            let rect = Rect::from_corner(at, r.width, r.height).ok_or("placement overflow")?;
            if boundary.is_some_and(|b| rect.min_x < b.min_x || rect.min_y < b.min_y || rect.max_x > b.max_x || rect.max_y > b.max_y) {
                return Err("locked label outside boundary".into());
            }
            let hits: Vec<_> = out.iter().filter(|p| intersection(rect, p.rect) > 0.0).map(|p| p.index).collect();
            let overlap = out
                .iter()
                .map(|p| intersection(rect, p.rect))
                .chain(obstacles.iter().chain(buffers.iter()).map(|&b| intersection(rect, b)))
                .sum::<f64>();
            if !overlap.is_finite() {
                return Err("collision score overflow".into());
            }
            if best.as_ref().is_none_or(|p| overlap < p.overlap) {
                best = Some(Placed { index, rect, overlap, hits });
            }
            if overlap == 0.0 {
                break;
            }
        }
        out.push(best.ok_or("no placement candidates")?);
    }
    out.sort_by_key(|p| p.index);
    Ok(out)
}
