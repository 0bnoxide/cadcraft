//! BORINGREPORT: boring-log report and sample summary to a table or workbook. Owner: Codex (see
//! `docs/agents/slice-6-subsurface.md`).

use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use cadcraft_doc::{EntityKind, Handle};
use cadcraft_geom::Vec2;
use cadcraft_subsurface::Table;
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("boringreport", "Boring Log Report", run)
            .params("{path,output?:xlsx_path,at?:[x,y]}; per-boring tables and combined sample summary"),
    ]
}

pub(crate) fn drawing_table(table: &Table, at: [f64; 2]) -> Result<cadcraft_doc::Table> {
    let cols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if table.rows.len() > 2001 || cols == 0 || cols > 200 || at.iter().any(|v| !v.is_finite()) {
        return Err(bad("boringreport", "drawing table exceeds 2000 data rows/200 columns or has invalid origin; export workbook instead"));
    }
    let rows: Vec<Vec<String>> = table.rows.iter().skip(1).map(|r| r.iter().map(|c| super::labels_callout::literal(&c.text())).collect()).collect();
    Ok(super::table::make_table(
        &cadcraft_doc::TableStyle::default(),
        Vec2::new(at[0], at[1]),
        rows.len(),
        cols,
        None,
        None,
        Some(&super::labels_callout::literal(&table.name)),
        Some(table.header().iter().map(|v| super::labels_callout::literal(v)).collect()),
        &rows,
    ))
}

pub(crate) fn output(s: &mut Session, p: &Value, tables: &[Table], xdata: &[cadcraft_doc::RawTag]) -> Result<Vec<Handle>> {
    if let Some(path) = str_param(p, "output") {
        let bytes = cadcraft_subsurface::sheet::write_xlsx(tables).map_err(|e| bad("boringreport", e.to_string()))?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::atomic_write::write_atomic(std::path::Path::new(path), &bytes).map_err(|e| bad("boringreport", e.to_string()))?;
            return Ok(Vec::new());
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (path, bytes);
            return Err(bad("boringreport", "workbook file output not available on web"));
        }
    }
    let at = super::subsurface_draw::point(p, "at", Some([0.0, 0.0]), "boringreport")?;
    let mut entities = Vec::new();
    let mut y = at[1];
    for table in tables {
        let entity = drawing_table(table, [at[0], y])?;
        y -= entity.row_heights.iter().sum::<f64>() + 2.0;
        if !y.is_finite() {
            return Err(bad("boringreport", "table layout overflow"));
        }
        entities.push(entity);
    }
    let mut drawing = s.doc()?.clone();
    let space = s.space();
    let mut handles = Vec::new();
    for t in entities {
        handles.push(drawing.add(&space, cadcraft_doc::Common { xdata: xdata.to_vec(), ..Default::default() }, EntityKind::Table(t))?);
    }
    *s.doc_mut()? = drawing;
    Ok(handles)
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let (set, mut findings) = super::subsurface_draw::load(p, "boringreport")?;
    let report = cadcraft_subsurface::report::build(&set).map_err(|e| bad("boringreport", e))?;
    findings.extend(report.findings);
    let handles = output(s, p, &report.tables, &[])?;
    Ok(
        json!({"handles":handles.iter().map(|h|h.hex()).collect::<Vec<_>>(),"tables":report.tables.len(),"findings":super::subsurface_import::findings_json(&findings),"output":p.get("output")}),
    )
}
