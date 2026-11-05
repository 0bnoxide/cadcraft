//! Callout tables: rows from a record, cell layout, box size, grid lines and leader attachment.
//! Reference: AutoGIS `callout_templates.py`, `callout_geometry.py`. Owner: Codex (see
//! `docs/agents/slice-4-labels.md`).

use crate::{Pt, Rect};

#[derive(Clone, Debug)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub span: usize,
    pub text: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug)]
pub struct Layout {
    pub bounds: Rect,
    pub columns: Vec<f64>,
    pub rows: Vec<f64>,
    pub anchors: Vec<Pt>,
    pub lines: Vec<(Pt, Pt)>,
}

pub fn layout(cells: &[Cell], padding: f64) -> Result<Layout, String> {
    if cells.is_empty() || cells.len() > 4096 || !padding.is_finite() || padding < 0.0 {
        return Err("invalid cells or padding".into());
    }
    let mut nr = 0;
    let mut nc = 0;
    for c in cells {
        if c.span == 0
            || c.row >= 256
            || c.col.checked_add(c.span).is_none_or(|n| n > 64)
            || !c.width.is_finite()
            || !c.height.is_finite()
            || c.width < 0.0
            || c.height <= 0.0
            || c.text.len() > 4096
        {
            return Err("invalid cell position, size or content".into());
        }
        nr = nr.max(c.row + 1);
        nc = nc.max(c.col + c.span);
    }
    let mut rows = vec![0.0_f64; nr];
    let mut columns = vec![padding * 2.0; nc];
    let mut occupied = std::collections::BTreeSet::new();
    for c in cells {
        for col in c.col..c.col + c.span {
            if !occupied.insert((c.row, col)) {
                return Err("overlapping cells".into());
            }
        }
        if let Some(h) = rows.get_mut(c.row) {
            *h = h.max(c.height + 2.0 * padding);
        }
        if c.span == 1
            && let Some(w) = columns.get_mut(c.col)
        {
            *w = w.max(c.width + 2.0 * padding);
        }
    }
    for c in cells.iter().filter(|c| c.span > 1) {
        let total: f64 = columns.iter().skip(c.col).take(c.span).sum();
        if let Some(w) = columns.get_mut(c.col) {
            *w += (c.width + 2.0 * padding - total).max(0.0);
        }
    }
    let w = columns.iter().sum();
    let h = rows.iter().sum();
    let bounds = Rect::from_corner(Pt::default(), w, h).ok_or("layout overflow")?;
    let mut lines = vec![
        (Pt { x: 0.0, y: 0.0 }, Pt { x: w, y: 0.0 }),
        (Pt { x: w, y: 0.0 }, Pt { x: w, y: h }),
        (Pt { x: w, y: h }, Pt { x: 0.0, y: h }),
        (Pt { x: 0.0, y: h }, Pt { x: 0.0, y: 0.0 }),
    ];
    let mut y = h;
    for rh in rows.iter().take(nr.saturating_sub(1)) {
        y -= rh;
        lines.push((Pt { x: 0.0, y }, Pt { x: w, y }));
    }
    let mut x = 0.0;
    for (col, cw) in columns.iter().enumerate().take(nc.saturating_sub(1)) {
        x += cw;
        let mut top = h;
        for (row, rh) in rows.iter().enumerate() {
            if cells.iter().any(|c| c.row == row && c.col + c.span == col + 1) {
                lines.push((Pt { x, y: top - rh }, Pt { x, y: top }));
            }
            top -= rh;
        }
    }
    let anchors = cells
        .iter()
        .map(|c| Pt { x: columns.iter().take(c.col).sum::<f64>() + padding, y: h - rows.iter().take(c.row).sum::<f64>() - padding })
        .collect();
    Ok(Layout { bounds, columns, rows, anchors, lines })
}

pub fn leader_attachment(b: Rect, source: Pt) -> Pt {
    let x = b.min_x / 2.0 + b.max_x / 2.0;
    let y = b.min_y / 2.0 + b.max_y / 2.0;
    [Pt { x, y: b.min_y }, Pt { x, y: b.max_y }, Pt { x: b.min_x, y }, Pt { x: b.max_x, y }]
        .into_iter()
        .min_by(|a, b| (a.x - source.x).hypot(a.y - source.y).total_cmp(&(b.x - source.x).hypot(b.y - source.y)))
        .unwrap_or_default()
}
