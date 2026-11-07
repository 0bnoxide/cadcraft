//! CALLOUT: measured tables and leaders. Owner: Codex.
use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use cadcraft_doc::{EntityKind, Leader, MText, Table, TableCell};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_labels::{
    Pt, Rect,
    callout::{Cell, layout, leader_attachment},
    format::{Part, Props, format},
    placement::{Request, place},
};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("callout", "Place Callouts", run)
            .params("{anchor:[x,y],record:{},rows:[[format parts]],height?:1,padding?:0.35,boundary?:Rect,kind?:table|mtext,template?:override template key,source?:override source key} -> {handles,overlap}"),
    ]
}

pub(crate) fn point(v: &Value) -> Result<Pt> {
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad("callout", "expected [x,y]"))?;
    let x = a.first().and_then(Value::as_f64).filter(|v| v.is_finite());
    let y = a.get(1).and_then(Value::as_f64).filter(|v| v.is_finite());
    match (x, y) {
        (Some(x), Some(y)) => Ok(Pt { x, y }),
        _ => Err(bad("callout", "point must be finite")),
    }
}

pub(crate) fn text(contents: String, at: Pt, height: f64) -> MText {
    MText {
        insert: Vec3 { x: at.x, y: at.y, z: 0.0 },
        height,
        width: 0.0,
        attach: 1,
        rotation: 0.0,
        style: "Standard".into(),
        contents,
        line_spacing: 1.0,
        line_spacing_exact: false,
        columns: None,
    }
}

pub(crate) fn literal(s: &str) -> String {
    cadcraft_doc::mtext_literal(s).replace('\n', "\\P")
}

pub(crate) fn measure(s: &Session, contents: &str, height: f64) -> Result<(f64, f64)> {
    let mut p = cadcraft_fonts::MTextParams::new(Vec2::ZERO, height);
    p.attach = 1;
    if let Some(style) = s.doc()?.text_style("Standard") {
        p.font = cadcraft_fonts::TextFont::resolve(&style.font);
        p.width_factor = style.width_factor;
        p.oblique = style.oblique;
    }
    let b = cadcraft_fonts::layout_mtext_with(contents, &p).bounds;
    let w = (b.max.x - b.min.x).max(height * 0.25);
    let h = (b.max.y - b.min.y).max(height);
    if !w.is_finite() || !h.is_finite() {
        return Err(bad("callout", "nonfinite text bounds"));
    }
    Ok((w, h))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let anchor = point(p.get("anchor").ok_or_else(|| bad("callout", "anchor required"))?)?;
    let height = p.get("height").and_then(Value::as_f64).unwrap_or(1.0);
    let padding = p.get("padding").and_then(Value::as_f64).unwrap_or(height * 0.35);
    if !height.is_finite() || height <= 0.0 {
        return Err(bad("callout", "height must be positive"));
    }
    let props: Props = serde_json::from_value(p.get("record").cloned().unwrap_or(json!({}))).map_err(|e| bad("callout", e.to_string()))?;
    let rows = p
        .get("rows")
        .and_then(Value::as_array)
        .filter(|r| !r.is_empty() && r.len() <= 256)
        .ok_or_else(|| bad("callout", "rows required (max 256)"))?;
    let mut cells = Vec::new();
    for (row, cols) in rows.iter().enumerate() {
        let cols = cols.as_array().filter(|a| !a.is_empty() && a.len() <= 64).ok_or_else(|| bad("callout", "row requires at most 64 cells"))?;
        for (col, parts) in cols.iter().enumerate() {
            let parts: Vec<Part> = serde_json::from_value(parts.clone()).map_err(|e| bad("callout", e.to_string()))?;
            if parts.len() > 256 {
                return Err(bad("callout", "too many format parts"));
            }
            let value = format(&parts, &props, 0);
            let (width, h) = measure(s, &literal(&value), height)?;
            cells.push(Cell { row, col, span: 1, text: value, width, height: h });
        }
    }
    let columns = cells.iter().map(|c| c.col + 1).max().unwrap_or(0);
    let title = columns > 1 && cells.iter().filter(|c| c.row == 0).count() == 1;
    if title && let Some(c) = cells.first_mut() {
        c.span = columns;
    }
    let g = layout(&cells, padding).map_err(|e| bad("callout", e))?;
    let boundary: Option<Rect> =
        p.get("boundary").map(|v| serde_json::from_value(v.clone())).transpose().map_err(|e| bad("callout", e.to_string()))?;
    let flat = cells.iter().map(|c| literal(&c.text)).collect::<Vec<_>>().join("\\P");
    let (width, box_height) = if str_param(p, "kind") == Some("mtext") { measure(s, &flat, height)? } else { (g.bounds.width(), g.bounds.height()) };
    let mut request = Request { anchor, width, height: box_height, origin: None, locked: false };
    if let (Some(template), Some(source)) = (str_param(p, "template"), str_param(p, "source"))
        && let Some(o) = super::callout_override::find(s.doc()?, template, source)
    {
        o.apply(&mut request, height);
    }
    let anchor = request.anchor;
    let d = s.doc()?;
    let obstacles: Vec<_> = d
        .space(&s.space())
        .into_iter()
        .flat_map(|store| store.iter())
        .filter(|e| matches!(e.kind, EntityKind::Table(_) | EntityKind::MText(_)))
        .filter_map(|e| {
            let b = cadcraft_doc::entity_bounds(d, e, 0);
            let rect = Rect { min_x: b.min.x, min_y: b.min.y, max_x: b.max.x, max_y: b.max.y };
            rect.is_valid().then_some(rect)
        })
        .take(cadcraft_labels::placement::MAX_LABELS + 1)
        .collect();
    let placed = place(&[request], &obstacles, boundary, height, 0.0).map_err(|e| bad("callout", e))?;
    let at = placed.first().ok_or_else(|| bad("callout", "no placement"))?;
    let origin = Pt { x: at.rect.min_x, y: at.rect.min_y };
    let kind = match str_param(p, "kind").unwrap_or("table") {
        "table" => {
            let table_rows = rows
                .iter()
                .enumerate()
                .map(|(r, _)| cells.iter().filter(|c| c.row == r).map(|c| TableCell { text: literal(&c.text), ..Default::default() }).collect())
                .collect();
            EntityKind::Table(Table {
                insert: Vec3 { x: origin.x, y: at.rect.max_y, z: 0.0 },
                col_widths: g.columns,
                row_heights: g.rows,
                cells: table_rows,
                style: "Standard".into(),
                text_height: height,
                title,
                header: false,
                row_styles: Vec::new(),
                breaks: None,
                data_link: None,
            })
        }
        "mtext" => EntityKind::MText(text(
            cells.iter().map(|c| literal(&c.text)).collect::<Vec<_>>().join("\\P"),
            Pt { x: origin.x, y: at.rect.max_y },
            height,
        )),
        _ => return Err(bad("callout", "kind must be table or mtext")),
    };
    let mut handles = vec![s.add_entity(kind)?.hex()];
    let end = leader_attachment(at.rect, anchor);
    handles.push(
        s.add_entity(EntityKind::Leader(Leader {
            vertices: vec![Vec3 { x: anchor.x, y: anchor.y, z: 0.0 }, Vec3 { x: end.x, y: end.y, z: 0.0 }],
            arrow: true,
            spline: false,
            style: "Standard".into(),
        }))?
        .hex(),
    );
    Ok(json!({"handles":handles,"overlap":at.overlap}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analytical_record_has_title_and_one_row_per_analyte() {
        let mut s = Session::new();
        s.execute(
            "callout",
            &json!({"anchor":[0,0],"record":{"location":"MW-5","benzene":"6.4","toluene":"<50"},
            "rows":[[[{"kind":"prop","key":"location"}]],
            [[{"kind":"text","text":"Benzene"}],[{"kind":"prop","key":"benzene"}]],
            [[{"kind":"text","text":"Toluene"}],[{"kind":"prop","key":"toluene"}]]]}),
        )
        .unwrap();
        let drawing = s.doc().unwrap();
        let t = drawing.model.iter().find_map(|e| if let EntityKind::Table(t) = &e.kind { Some(t) } else { None }).unwrap();
        assert_eq!(t.cells.len(), 3);
        assert!(t.title);
        assert_eq!(t.cells[0][0].text, "MW-5");
        assert_eq!(t.cells[1][1].text, "6.4");
        assert_eq!(t.cells[2][1].text, "<50");
    }

    #[test]
    fn successive_callouts_avoid_existing_table() {
        let mut s = Session::new();
        let p = json!({"anchor":[0,0],"rows":[[[{"kind":"text","text":"Test"}]] ]});
        s.execute("callout", &p).unwrap();
        s.execute("callout", &p).unwrap();
        let d = s.doc().unwrap();
        let tables: Vec<_> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Table(_))).collect();
        assert_ne!(tables[0].kind, tables[1].kind);
    }
    #[test]
    fn record_formats_into_table_with_leader_and_mtext() {
        for kind in ["table", "mtext"] {
            let mut s = Session::new();
            let result = s
                .execute(
                    "callout",
                    &json!({"anchor":[0,0],"kind":kind,"record":{"well":"MW-5","result":6.4},
                "rows":[[[{"kind":"prop","key":"well"}]],[[{"kind":"number","key":"result","decimals":1}]]]}),
                )
                .unwrap();
            assert_eq!(result["handles"].as_array().unwrap().len(), 2);
            assert_eq!(s.doc().unwrap().model.len(), 2);
        }
    }
}
