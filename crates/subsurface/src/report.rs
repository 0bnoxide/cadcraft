//! Boring-log reports and the sample summary as tables. Owner: Codex (see
//! `docs/agents/slice-6-subsurface.md`). Behaviour reference: AutoGIS `boring_log_report.py`.

use crate::{BoringSet, Cell, Finding, Severity, Table, sticklog};

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub tables: Vec<Table>,
    pub findings: Vec<Finding>,
}

fn text(value: &str) -> Cell {
    Cell::Text(value.into())
}
fn number(value: Option<f64>) -> Cell {
    value.map(Cell::Number).unwrap_or_default()
}
fn table(name: String, header: &[&str]) -> Table {
    Table { name, rows: vec![header.iter().map(|s| text(s)).collect()] }
}

pub fn build(set: &BoringSet) -> Result<Report, String> {
    let count = [set.borings.len(), set.lithology.len(), set.samples.len(), set.wells.len(), set.water.len()]
        .into_iter()
        .try_fold(0usize, |a, b| a.checked_add(b))
        .ok_or("too many records")?;
    if count > sticklog::MAX_ITEMS || set.borings.len() > 1000 {
        return Err("too many records".into());
    }
    let mut out = Vec::new();
    let mut findings = Vec::new();
    let mut summary = table(
        "Sample summary".into(),
        &["boring_id", "sample_id", "sample_type", "top_depth", "bottom_depth", "recovery", "blow_counts", "lab_submitted", "pid_ppm"],
    );
    for boring in &set.borings {
        let id = &boring.boring_id;
        let mut header = table(format!("{id} header"), &["Property", "Value"]);
        for (key, value) in [
            ("Boring", text(id)),
            ("Site", text(&boring.site_id)),
            ("Ground elevation", number(boring.ground_elevation)),
            ("Total depth", number(boring.total_depth)),
            ("Coordinate system", text(&boring.coordinate_system)),
            ("Vertical datum", text(&boring.vertical_datum)),
            ("Completion", text(&boring.completion_type)),
        ] {
            header.rows.push(vec![text(key), value]);
        }
        out.push(header);
        let mut lith =
            table(format!("{id} lithology"), &["Top", "Bottom", "USCS", "Material", "Color", "Moisture", "Consistency", "PID ppm", "Description"]);
        let mut intervals: Vec<_> = set.lithology.iter().filter(|r| r.boring_id == *id).collect();
        intervals.sort_by(|a, b| a.top_depth.total_cmp(&b.top_depth));
        for row in &intervals {
            sticklog::interval(id, row.top_depth, row.bottom_depth)?;
            lith.rows.push(vec![
                Cell::Number(row.top_depth),
                Cell::Number(row.bottom_depth),
                text(&row.uscs),
                text(&row.primary_material),
                text(&row.color),
                text(&row.moisture),
                text(&row.consistency),
                number(row.pid_ppm),
                text(&row.description),
            ]);
        }
        out.push(lith);
        let mut samples = table(
            format!("{id} samples"),
            &["boring_id", "sample_id", "sample_type", "top_depth", "bottom_depth", "recovery", "blow_counts", "lab_submitted", "pid_ppm"],
        );
        let mut rows: Vec<_> = set.samples.iter().filter(|r| r.boring_id == *id).collect();
        if rows.is_empty() {
            findings.push(Finding {
                severity: Severity::Warning,
                code: "no_samples".into(),
                message: format!("Boring {id} has no sample records"),
                boring_id: Some(id.clone()),
                source: None,
            });
        }
        rows.sort_by(|a, b| a.top_depth.total_cmp(&b.top_depth));
        for row in rows {
            sticklog::interval(id, row.top_depth, row.bottom_depth)?;
            let pid = intervals
                .iter()
                .filter(|r| r.top_depth < row.bottom_depth && r.bottom_depth > row.top_depth)
                .filter_map(|r| r.pid_ppm)
                .max_by(f64::total_cmp);
            let cells = vec![
                text(id),
                text(&row.sample_id),
                text(&row.sample_type),
                Cell::Number(row.top_depth),
                Cell::Number(row.bottom_depth),
                number(row.recovery),
                text(&row.blow_counts),
                Cell::Bool(row.lab_submitted),
                number(pid),
            ];
            summary.rows.push(cells.clone());
            samples.rows.push(cells);
        }
        out.push(samples);
        let mut wells = table(format!("{id} wells"), &["Top", "Bottom", "Component", "Diameter", "Material", "Slot size"]);
        let mut rows: Vec<_> = set.wells.iter().filter(|r| r.boring_id == *id).collect();
        rows.sort_by(|a, b| a.top_depth.total_cmp(&b.top_depth));
        for row in rows {
            sticklog::interval(id, row.top_depth, row.bottom_depth)?;
            wells.rows.push(vec![
                Cell::Number(row.top_depth),
                Cell::Number(row.bottom_depth),
                text(&row.component),
                number(row.diameter),
                text(&row.material),
                text(&row.slot_size),
            ]);
        }
        out.push(wells);
        let mut water = table(format!("{id} water"), &["Observed", "Depth to water", "Kind"]);
        for row in set.water.iter().filter(|r| r.boring_id == *id) {
            water.rows.push(vec![text(&row.observed), number(row.depth_to_water), text(&row.kind)]);
        }
        out.push(water);
        let mut approval = table(format!("{id} approval"), &["Role", "Name", "Signature", "Date"]);
        for role in ["Logged by", "Reviewed by", "Approved by"] {
            approval.rows.push(vec![text(role), Cell::Empty, Cell::Empty, Cell::Empty]);
        }
        out.push(approval);
    }
    out.push(summary);
    if out.iter().flat_map(|t| &t.rows).flatten().any(|c| matches!(c,Cell::Number(n) if !n.is_finite())) {
        return Err("nonfinite report value".into());
    }
    Ok(Report { tables: out, findings })
}
