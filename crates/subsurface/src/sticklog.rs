//! Stick-log geometry per boring: lithology column with USCS hatches, sample lane, well
//! column and water-level marker. Owner: Codex (see `docs/agents/slice-6-subsurface.md`).
//! Behaviour reference: AutoGIS `sticklog.py`.

use crate::boring::{BoringSet, Lithology};

pub const MAX_ITEMS: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Box { layer: &'static str, at: [f64; 2], size: [f64; 2], hatch: Option<&'static str> },
    Text { layer: &'static str, at: [f64; 2], text: String },
    Line { layer: &'static str, a: [f64; 2], b: [f64; 2] },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Log {
    pub marks: Vec<Mark>,
    pub depth: f64,
    pub warnings: Vec<String>,
}

pub fn hatch(uscs: &str) -> &'static str {
    // shortcut: primary USCS fraction only, expand when a deliverable requires full ASTM symbols.
    match uscs.trim().chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('G' | 'S') => "DOTS",
        Some('C') => "ANSI31",
        Some('M') => "ANSI32",
        Some('O' | 'P') => "ANSI37",
        _ => "SOLID",
    }
}

pub fn description(row: &Lithology) -> String {
    let mut text = [&row.primary_material, &row.description].into_iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(", ");
    if let Some(pid) = row.pid_ppm {
        if !text.is_empty() {
            text.push_str(" — ");
        }
        text.push_str(&format!("PID {pid} ppm"));
    }
    text
}

pub fn interval(top: f64, bottom: f64) -> Result<(), String> {
    if !top.is_finite() || !bottom.is_finite() || top < 0.0 || bottom < top {
        return Err("invalid depth interval".into());
    }
    Ok(())
}

pub fn draw(set: &BoringSet, id: &str, at: [f64; 2], scale: f64) -> Result<Log, String> {
    if !scale.is_finite() || scale <= 0.0 || at.iter().any(|v| !v.is_finite()) {
        return Err("invalid log origin or depth scale".into());
    }
    let count = [set.borings.len(), set.lithology.len(), set.samples.len(), set.wells.len(), set.water.len()]
        .into_iter()
        .try_fold(0usize, |a, b| a.checked_add(b))
        .ok_or("too many records")?;
    if count > MAX_ITEMS {
        return Err("too many records".into());
    }
    let boring = set.borings.iter().find(|b| b.boring_id == id).ok_or("unknown boring")?;
    let mut log = Log::default();
    let mut lith: Vec<_> = set.lithology.iter().filter(|r| r.boring_id == id).collect();
    lith.sort_by(|a, b| a.top_depth.total_cmp(&b.top_depth));
    if lith.is_empty() {
        log.warnings.push(format!("sticklog_no_lithology: {id}"));
        return Ok(log);
    }
    let y = |d: f64| at[1] - d * scale;
    log.marks.push(Mark::Text { layer: "BORING-TEXT", at: [at[0], at[1] + 0.5], text: id.into() });
    for row in lith {
        interval(row.top_depth, row.bottom_depth)?;
        if row.pid_ppm.is_some_and(|v| !v.is_finite()) {
            return Err("nonfinite PID".into());
        }
        log.depth = log.depth.max(row.bottom_depth);
        log.marks.push(Mark::Box {
            layer: "BORING-LOG",
            at: [at[0], y(row.bottom_depth)],
            size: [2.0, (row.bottom_depth - row.top_depth) * scale],
            hatch: Some(hatch(&row.uscs)),
        });
        let mid = y(row.top_depth / 2.0 + row.bottom_depth / 2.0);
        log.marks.push(Mark::Text { layer: "BORING-TEXT", at: [at[0] + 0.5, mid], text: row.uscs.clone() });
        log.marks.push(Mark::Text { layer: "BORING-TEXT", at: [at[0] - 2.0, y(row.top_depth)], text: row.top_depth.to_string() });
        log.marks.push(Mark::Text { layer: "BORING-TEXT", at: [at[0] - 2.0, y(row.bottom_depth)], text: row.bottom_depth.to_string() });
        log.marks.push(Mark::Text { layer: "BORING-TEXT", at: [at[0] + 3.25, mid], text: description(row) });
    }
    for row in set.samples.iter().filter(|r| r.boring_id == id) {
        interval(row.top_depth, row.bottom_depth)?;
        log.depth = log.depth.max(row.bottom_depth);
        log.marks.push(Mark::Box {
            layer: "BORING-SAMPLE",
            at: [at[0] - 1.0, y(row.bottom_depth)],
            size: [1.0, (row.bottom_depth - row.top_depth) * scale],
            hatch: None,
        });
        log.marks.push(Mark::Text { layer: "BORING-SAMPLE", at: [at[0] - 1.0, y(row.top_depth)], text: row.sample_id.clone() });
    }
    for row in set.wells.iter().filter(|r| r.boring_id == id) {
        interval(row.top_depth, row.bottom_depth)?;
        log.depth = log.depth.max(row.bottom_depth);
        log.marks.push(Mark::Box {
            layer: "BORING-WELL",
            at: [at[0] + 2.0, y(row.bottom_depth)],
            size: [1.0, (row.bottom_depth - row.top_depth) * scale],
            hatch: row.component.to_ascii_lowercase().contains("screen").then_some("ANSI32"),
        });
        log.marks.push(Mark::Text { layer: "BORING-WELL", at: [at[0] + 2.0, y(row.top_depth)], text: row.component.clone() });
    }
    if let Some(depth) = set.water.iter().filter(|r| r.boring_id == id).find_map(|r| r.depth_to_water) {
        interval(depth, depth)?;
        log.depth = log.depth.max(depth);
        log.marks.push(Mark::Line { layer: "BORING-WATER", a: [at[0] - 1.0, y(depth)], b: [at[0] + 3.0, y(depth)] });
        for x in [at[0] - 1.25, at[0] - 0.75] {
            log.marks.push(Mark::Line { layer: "BORING-WATER", a: [x, y(depth) + 0.25], b: [at[0] - 1.0, y(depth)] });
        }
        log.marks.push(Mark::Text { layer: "BORING-WATER", at: [at[0] + 3.25, y(depth)], text: format!("DTW {depth}") });
    }
    if let Some(depth) = boring.total_depth {
        interval(depth, depth)?;
        log.depth = log.depth.max(depth);
    }
    if !y(log.depth).is_finite()
        || log.marks.iter().any(|m| match m {
            Mark::Box { at, size, .. } => at.iter().chain(size).any(|v| !v.is_finite()),
            Mark::Text { at, .. } => at.iter().any(|v| !v.is_finite()),
            Mark::Line { a, b, .. } => a.iter().chain(b).any(|v| !v.is_finite()),
        })
    {
        return Err("log coordinate overflow".into());
    }
    Ok(log)
}
