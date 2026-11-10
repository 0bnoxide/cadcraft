//! DATAWIZARD: extract object properties to a drawing table or workbook. Owner: Codex (see
//! `docs/agents/slice-6-subsurface.md`).

use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use cadcraft_doc::{Entity, EntityKind, Handle, RawTag};
use cadcraft_subsurface::{Cell, Table};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const APP: &str = "CADCRAFT_DATAWIZARD";

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Aggregate {
    Sum,
    Min,
    Max,
    Count,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    property: String,
    title: String,
    decimals: Option<usize>,
    aggregate: Option<Aggregate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    version: u32,
    #[serde(default)]
    handles: Vec<String>,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    layers: Vec<String>,
    columns: Vec<Column>,
    #[serde(default)]
    group_by: Vec<String>,
    #[serde(default)]
    sort_by: Vec<String>,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("datawizard","Data Wizard",run).params("{settings:{version:1,handles?:[],types?:[],layers?:[],columns:[{property,title,decimals?,aggregate?:sum|min|max|count}],group_by?:[property],sort_by?:[property]},at?:[x,y],output?:xlsx_path} or {refresh:owned_table_handle}; properties: handle,type,layer,block,attribute:TAG,length,area,x,y,z,count; count aggregate uses property count")]
}

fn property(e: &Entity, name: &str) -> Result<Cell> {
    let num = |n: f64| if n.is_finite() { Ok(Cell::Number(n)) } else { Err(bad("datawizard", "nonfinite derived value")) };
    match name {
        "count" => num(1.0),
        "handle" => Ok(Cell::Text(e.handle.hex())),
        "type" => Ok(Cell::Text(e.kind.type_name().into())),
        "layer" => Ok(Cell::Text(e.common.layer.clone())),
        "block" => match &e.kind {
            EntityKind::Insert(b) => Ok(Cell::Text(b.block.clone())),
            _ => Err(bad("datawizard", "block property requires block reference")),
        },
        "length" => num(match &e.kind {
            EntityKind::Line(l) => (l.b.x - l.a.x).hypot(l.b.y - l.a.y).hypot(l.b.z - l.a.z),
            EntityKind::Circle(c) => cadcraft_geom::TAU * c.radius,
            EntityKind::Arc(a) => cadcraft_geom::ccw_sweep(a.start, a.end) * a.radius,
            EntityKind::LwPolyline(p) => {
                if p.vertices.len() > 100_000 {
                    return Err(bad("datawizard", "too many vertices"));
                }
                cadcraft_geom::Polyline { vertices: p.vertices.clone(), closed: p.closed }.len()
            }
            _ => return Err(bad("datawizard", format!("length unsupported for {}", e.kind.type_name()))),
        }),
        "area" => num(match &e.kind {
            EntityKind::Circle(c) => cadcraft_geom::PI * c.radius * c.radius,
            EntityKind::LwPolyline(p) if p.closed => {
                if p.vertices.len() > 100_000 {
                    return Err(bad("datawizard", "too many vertices"));
                }
                cadcraft_geom::Polyline { vertices: p.vertices.clone(), closed: true }.area().abs()
            }
            _ => return Err(bad("datawizard", format!("area unsupported for {}", e.kind.type_name()))),
        }),
        "x" | "y" | "z" => {
            let p = match &e.kind {
                EntityKind::Point(p) => p.p,
                EntityKind::Line(l) => l.a,
                EntityKind::Circle(c) => c.center,
                EntityKind::Insert(b) => b.insert,
                _ => return Err(bad("datawizard", "coordinate property unsupported for object")),
            };
            num(match name {
                "x" => p.x,
                "y" => p.y,
                _ => p.z,
            })
        }
        _ => {
            let tag = name.strip_prefix("attribute:").ok_or_else(|| bad("datawizard", format!("unknown property {name}")))?;
            if let EntityKind::Insert(b) = &e.kind {
                let a = b
                    .attribs
                    .iter()
                    .find(|a| a.tag.eq_ignore_ascii_case(tag))
                    .ok_or_else(|| bad("datawizard", format!("missing attribute {tag} on {}", e.handle.hex())))?;
                Ok(Cell::Text(a.text.value.clone()))
            } else {
                Err(bad("datawizard", "attributes require block references"))
            }
        }
    }
}

fn settings(v: Value) -> Result<Settings> {
    if v.to_string().len() > 16_000 {
        return Err(bad("datawizard", "settings too large"));
    }
    let settings: Settings = serde_json::from_value(v).map_err(|e| bad("datawizard", e.to_string()))?;
    if settings.version != 1
        || settings.columns.is_empty()
        || settings.columns.len() > 200
        || settings.handles.len() > 2000
        || settings.columns.iter().any(|c| c.decimals.is_some_and(|d| d > 12))
    {
        return Err(bad("datawizard", "invalid settings version, columns, handles or decimal precision"));
    }
    let names: BTreeSet<_> = settings.columns.iter().map(|c| c.property.as_str()).collect();
    if names.len() != settings.columns.len() || settings.group_by.iter().chain(&settings.sort_by).any(|n| !names.contains(n.as_str())) {
        return Err(bad("datawizard", "duplicate properties or unknown grouping/sort column"));
    }
    if settings.columns.iter().any(|c| {
        c.aggregate.is_some() && settings.group_by.contains(&c.property) || matches!(c.aggregate, Some(Aggregate::Count)) && c.property != "count"
    }) {
        return Err(bad("datawizard", "aggregate columns cannot be grouping keys; count aggregate requires property count"));
    }
    Ok(settings)
}

fn extract(s: &Session, cfg: &Settings, exclude: Option<Handle>) -> Result<Table> {
    let wanted: BTreeSet<_> =
        cfg.handles.iter().map(|h| Handle::parse_hex(h).ok_or_else(|| bad("datawizard", "invalid source handle"))).collect::<Result<_>>()?;
    for h in &wanted {
        if s.doc()?.model.get(*h).is_none() {
            return Err(bad("datawizard", format!("stale source {}", h.hex())));
        }
    }
    let mut rows = Vec::new();
    for e in s.doc()?.model.iter() {
        if Some(e.handle) == exclude
            || (!wanted.is_empty() && !wanted.contains(&e.handle))
            || (!cfg.types.is_empty() && !cfg.types.iter().any(|t| t.eq_ignore_ascii_case(e.kind.type_name())))
            || (!cfg.layers.is_empty() && !cfg.layers.contains(&e.common.layer))
        {
            continue;
        }
        if rows.len() >= 2000 {
            return Err(bad("datawizard", "more than 2000 source objects"));
        }
        rows.push(cfg.columns.iter().map(|c| property(e, &c.property)).collect::<Result<Vec<_>>>()?);
    }
    if rows.is_empty() {
        return Err(bad("datawizard", "no matching objects"));
    }
    let index = |name: &str| cfg.columns.iter().position(|c| c.property == name);
    if !cfg.group_by.is_empty() || cfg.columns.iter().any(|c| c.aggregate.is_some()) {
        let mut groups: BTreeMap<Vec<String>, Vec<Cell>> = BTreeMap::new();
        for mut row in rows {
            for (cell, col) in row.iter_mut().zip(&cfg.columns) {
                if let Some(aggregate) = col.aggregate {
                    let n = if matches!(aggregate, Aggregate::Count) {
                        1.0
                    } else {
                        cell.number().ok_or_else(|| bad("datawizard", "aggregate requires numeric values"))?
                    };
                    *cell = Cell::Number(n);
                }
            }
            let key: Vec<_> =
                cfg.group_by.iter().filter_map(|n| index(n).and_then(|i| row.get(i))).map(|c| serde_json::to_string(c).unwrap_or_default()).collect();
            if let Some(existing) = groups.get_mut(&key) {
                for ((dest, source), col) in existing.iter_mut().zip(&row).zip(&cfg.columns) {
                    if let Some(aggregate) = col.aggregate {
                        let a = dest.number().ok_or_else(|| bad("datawizard", "invalid aggregate value"))?;
                        let b = source.number().ok_or_else(|| bad("datawizard", "invalid aggregate value"))?;
                        let value = match aggregate {
                            Aggregate::Sum | Aggregate::Count => a + b,
                            Aggregate::Min => a.min(b),
                            Aggregate::Max => a.max(b),
                        };
                        if !value.is_finite() {
                            return Err(bad("datawizard", "aggregate overflow"));
                        }
                        *dest = Cell::Number(value);
                    } else if dest != source {
                        return Err(bad("datawizard", "group contains different non-group values; include those columns in group_by"));
                    }
                }
            } else {
                groups.insert(key, row);
            }
        }
        rows = groups.into_values().collect();
    }
    rows.sort_by(|a, b| {
        for name in &cfg.sort_by {
            if let Some(i) = index(name) {
                let x = a.get(i);
                let y = b.get(i);
                let order = match (x, y) {
                    (Some(Cell::Number(x)), Some(Cell::Number(y))) => x.total_cmp(y),
                    (Some(x), Some(y)) => x.text().cmp(&y.text()),
                    _ => std::cmp::Ordering::Equal,
                };
                if !order.is_eq() {
                    return order;
                }
            }
        }
        std::cmp::Ordering::Equal
    });
    for row in &mut rows {
        for (cell, col) in row.iter_mut().zip(&cfg.columns) {
            if let (Cell::Number(n), Some(d)) = (&*cell, col.decimals) {
                *cell = Cell::Text(format!("{n:.d$}"));
            }
        }
    }
    rows.insert(0, cfg.columns.iter().map(|c| Cell::Text(c.title.clone())).collect());
    Ok(Table { name: "Data extraction".into(), rows })
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let refresh = str_param(p, "refresh").map(|h| Handle::parse_hex(h).ok_or_else(|| bad("datawizard", "invalid refresh handle"))).transpose()?;
    let (cfg, at) = if let Some(h) = refresh {
        if p.get("settings").is_some() || p.get("output").is_some() {
            return Err(bad("datawizard", "refresh uses the table's saved settings"));
        }
        let e = s.doc()?.entity(h).ok_or_else(|| bad("datawizard", "refresh table missing"))?;
        let EntityKind::Table(t) = &e.kind else {
            return Err(bad("datawizard", "refresh target is not a table"));
        };
        let i = e
            .common
            .xdata
            .iter()
            .position(|t| t.code == 1001 && t.value == APP)
            .ok_or_else(|| bad("datawizard", "table is not owned by datawizard"))?;
        let chunks: Vec<_> = e.common.xdata.iter().skip(i + 1).take_while(|t| t.code != 1001).collect();
        if chunks.len() > 80 || chunks.iter().any(|t| t.code != 1000 || t.value.len() > 240) {
            return Err(bad("datawizard", "invalid ownership metadata"));
        }
        let payload = chunks.iter().map(|t| t.value.as_str()).collect::<String>();
        (settings(serde_json::from_str(&payload).map_err(|e| bad("datawizard", e.to_string()))?)?, [t.insert.x, t.insert.y])
    } else {
        (
            settings(p.get("settings").cloned().ok_or_else(|| bad("datawizard", "settings required"))?)?,
            super::subsurface_draw::point(p, "at", Some([0.0, 0.0]), "datawizard")?,
        )
    };
    let table = extract(s, &cfg, refresh)?;
    let data = serde_json::to_string(&cfg).map_err(|e| bad("datawizard", e.to_string()))?;
    if data.len() > 16_000 {
        return Err(bad("datawizard", "saved settings too large"));
    }
    let mut tags = vec![RawTag { code: 1001, value: APP.into() }];
    let mut chunk = String::new();
    for c in data.chars() {
        if chunk.len() + c.len_utf8() > 240 {
            tags.push(RawTag { code: 1000, value: std::mem::take(&mut chunk) });
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        tags.push(RawTag { code: 1000, value: chunk });
    }
    if let Some(h) = refresh {
        let entity = super::subsurface_report::drawing_table(&table, at)?;
        s.doc_mut()?.modify_entity(h, |e| e.kind = EntityKind::Table(entity))?;
        return Ok(json!({"handle":h.hex(),"refreshed":true,"rows":table.rows.len()-1}));
    }
    let handles = super::subsurface_report::output(s, p, &[table], &tags)?;
    Ok(json!({"handles":handles.iter().map(|h|h.hex()).collect::<Vec<_>>(),"refreshed":false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_geometry_and_block_attributes_are_resolved_explicitly() {
        let line = Entity::new(
            Handle(1),
            EntityKind::Line(cadcraft_doc::Line { a: cadcraft_geom::Vec3::ZERO, b: cadcraft_geom::Vec3 { x: 3.0, y: 4.0, z: 12.0 } }),
        );
        assert_eq!(property(&line, "length").unwrap(), Cell::Number(13.0));
        assert!(property(&line, "area").is_err());
        let circle = Entity::new(Handle(2), EntityKind::Circle(cadcraft_doc::Circle { center: cadcraft_geom::Vec3::ZERO, radius: 2.0 }));
        assert_eq!(property(&circle, "area").unwrap(), Cell::Number(4.0 * cadcraft_geom::PI));
        let text: cadcraft_doc::Text = serde_json::from_value(json!({"insert":{"x":0,"y":0,"z":0},"height":1,"value":"MW-1"})).unwrap();
        let insert = Entity::new(
            Handle(3),
            EntityKind::Insert(cadcraft_doc::Insert {
                block: "WELL".into(),
                insert: cadcraft_geom::Vec3::ZERO,
                scale: cadcraft_geom::Vec3 { x: 1.0, y: 1.0, z: 1.0 },
                rotation: 0.0,
                attribs: vec![cadcraft_doc::Attrib {
                    tag: "ID".into(),
                    text,
                    invisible: false,
                    constant: false,
                    prompt: String::new(),
                    props: Default::default(),
                    multiline: None,
                    modes: Default::default(),
                }],
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
                clip: None,
            }),
        );
        assert_eq!(property(&insert, "block").unwrap().text(), "WELL");
        assert_eq!(property(&insert, "attribute:id").unwrap().text(), "MW-1");
        assert!(property(&insert, "attribute:MISSING").is_err());
        assert!(property(&line, "attribute:ID").is_err());
    }

    #[test]
    fn overflowing_aggregate_leaves_the_drawing_unchanged() {
        let mut s = Session::new();
        for _ in 0..2 {
            s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3 { x: f64::MAX, y: 0.0, z: 0.0 }, angle: 0.0 })).unwrap();
        }
        let before = s.doc().unwrap().model.clone();
        assert!(s.execute("datawizard", &json!({"settings":{"version":1,"columns":[{"property":"x","title":"X","aggregate":"sum"}]}})).is_err());
        assert_eq!(s.doc().unwrap().model, before);
    }
    #[test]
    fn grouped_takeoffs_aggregate_before_format_and_keep_keys() {
        let mut s = Session::new();
        for length in [2.0, 3.0] {
            s.add_entity(EntityKind::Line(cadcraft_doc::Line { a: cadcraft_geom::Vec3::ZERO, b: cadcraft_geom::Vec3 { x: length, y: 0.0, z: 0.0 } }))
                .unwrap();
        }
        for (aggregate, want) in [("sum", 5.0), ("min", 2.0), ("max", 3.0)] {
            let cfg=settings(json!({"version":1,"columns":[{"property":"layer","title":"Layer"},{"property":"length","title":"Length","aggregate":aggregate},{"property":"count","title":"Objects","aggregate":"count"}],"group_by":["layer"]})).unwrap();
            let table = extract(&s, &cfg, None).unwrap();
            assert_eq!(table.rows.len(), 2);
            assert_eq!(table.cell(1, 1), &Cell::Number(want));
            assert_eq!(table.cell(1, 2), &Cell::Number(2.0));
        }
        let cfg = settings(json!({"version":1,"columns":[{"property":"length","title":"Total","aggregate":"sum","decimals":1}]})).unwrap();
        assert_eq!(extract(&s, &cfg, None).unwrap().cell(1, 0).text(), "5.0");
    }

    #[test]
    fn aggregate_errors_do_not_create_output() {
        let mut s = Session::new();
        for x in [1.0, 2.0] {
            s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3 { x, y: 0.0, z: 0.0 }, angle: 0.0 })).unwrap();
        }
        let count = s.doc().unwrap().model.len();
        for columns in [
            json!([{"property":"type","title":"Bad sum","aggregate":"sum"}]),
            json!([{"property":"x","title":"X"},{"property":"count","title":"Count","aggregate":"count"}]),
        ] {
            assert!(s.execute("datawizard", &json!({"settings":{"version":1,"columns":columns}})).is_err());
            assert_eq!(s.doc().unwrap().model.len(), count);
        }
        assert!(settings(json!({"version":1,"columns":[{"property":"count","title":"N","aggregate":"sum"}],"group_by":["count"]})).is_err());
    }
    #[test]
    fn refresh_preserves_handle_and_position_and_reloads_sources() {
        let mut s = Session::new();
        let source = s.add_entity(EntityKind::Circle(cadcraft_doc::Circle { center: cadcraft_geom::Vec3::ZERO, radius: 2.0 })).unwrap();
        let cfg = json!({"version":1,"handles":[source.hex()],"columns":[{"property":"area","title":"Area","decimals":2}],"sort_by":["area"]});
        let first = s.execute("datawizard", &json!({"settings":cfg,"at":[12,34]})).unwrap();
        let h = first["handles"][0].as_str().unwrap();
        let count = s.doc().unwrap().model.len();
        s.doc_mut()
            .unwrap()
            .modify_entity(source, |e| {
                if let EntityKind::Circle(c) = &mut e.kind {
                    c.radius = 3.0;
                }
            })
            .unwrap();
        let refreshed = s.execute("datawizard", &json!({"refresh":h})).unwrap();
        assert_eq!(refreshed["handle"], h);
        assert_eq!(s.doc().unwrap().model.len(), count);
        let e = s.doc().unwrap().model.get(Handle::parse_hex(h).unwrap()).unwrap();
        let EntityKind::Table(t) = &e.kind else { panic!("not table") };
        assert_eq!(t.insert.x, 12.0);
        assert_eq!(t.insert.y, 34.0);
        assert_eq!(t.cells.last().unwrap()[0].text, "28.27");
        let bytes = cadcraft_io::write(s.doc().unwrap(), "owned.dxf").unwrap();
        let drawing = cadcraft_io::read(&bytes, "owned.dxf").unwrap();
        assert!(drawing.model.iter().any(|e| e.common.xdata.iter().any(|t| t.value == APP)));
    }

    fn owned_paper_table() -> (Session, Handle, Handle) {
        let mut s = Session::new();
        let source = s.add_entity(EntityKind::Circle(cadcraft_doc::Circle { center: cadcraft_geom::Vec3::ZERO, radius: 2.0 })).unwrap();
        s.execute("layout.set", &json!({"name":"Layout1"})).unwrap();
        let result = s
            .execute(
                "datawizard",
                &json!({
                    "settings":{"version":1,"handles":[source.hex()],"columns":[{"property":"area","title":"Area","decimals":2}]},
                    "at":[12,34]
                }),
            )
            .unwrap();
        let table = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
        (s, source, table)
    }

    #[test]
    fn paper_table_refresh_reloads_sources_from_any_active_space_and_is_undoable() {
        for active in ["Layout1", "Model", "Layout2"] {
            let (mut s, source, table) = owned_paper_table();
            let original = s.doc().unwrap().entity(table).unwrap().clone();
            s.doc_mut()
                .unwrap()
                .modify_entity(source, |e| {
                    if let EntityKind::Circle(c) = &mut e.kind {
                        c.radius = 3.0;
                    }
                })
                .unwrap();
            s.execute("layout.set", &json!({"name":active})).unwrap();
            let before = s.doc().unwrap().clone();
            let result = s.execute("datawizard", &json!({"refresh":table.hex()})).unwrap();
            assert_eq!(result, json!({"handle":table.hex(),"refreshed":true,"rows":1}));
            assert_eq!(s.layout_space().tab_name(), active);
            let doc = s.doc().unwrap();
            assert_eq!(doc.entity_count(), before.entity_count());
            assert_eq!(doc.model, before.model);
            assert_eq!(doc.layout("Layout2"), before.layout("Layout2"));
            assert_eq!(doc.space_of(table), Some(cadcraft_doc::Space::Paper("Layout1".into())));
            let entity = doc.entity(table).unwrap();
            assert_eq!(entity.common, original.common);
            let EntityKind::Table(updated) = &entity.kind else { panic!("not a table") };
            assert_eq!((updated.insert.x, updated.insert.y), (12.0, 34.0));
            assert_eq!(updated.cells.last().unwrap()[0].text, "28.27");
            let after = doc.clone();
            s.undo().unwrap();
            assert_eq!(s.doc().unwrap(), &before);
            s.redo().unwrap();
            assert_eq!(s.doc().unwrap(), &after);
        }
    }

    #[test]
    fn paper_table_refresh_keeps_ownership_through_dxf_roundtrip() {
        let (mut s, source, table) = owned_paper_table();
        let bytes = cadcraft_io::write(s.doc().unwrap(), "paper.dxf").unwrap();
        *s.doc_mut().unwrap() = cadcraft_io::read(&bytes, "paper.dxf").unwrap();
        s.execute("layout.set", &json!({"name":"Model"})).unwrap();
        s.execute("scale", &json!({"handles":[source.hex()],"base":[0,0],"factor":1.5})).unwrap();
        let original = s.doc().unwrap().entity(table).unwrap().clone();
        s.execute("datawizard", &json!({"refresh":table.hex()})).unwrap();
        let doc = s.doc().unwrap();
        let updated = doc.layout("Layout1").unwrap().entities.get(table).unwrap();
        assert_eq!(updated.common, original.common);
        let EntityKind::Table(updated) = &updated.kind else { panic!("not a table") };
        assert_eq!((updated.insert.x, updated.insert.y), (12.0, 34.0));
        assert_eq!(updated.cells.last().unwrap()[0].text, "28.27");
    }

    #[test]
    fn paper_table_refresh_errors_preserve_drawing_selection_undo_and_redo() {
        for reason in ["stale source", "not owned", "invalid ownership metadata", "not a table"] {
            let (mut s, source, table) = owned_paper_table();
            match reason {
                "stale source" => {
                    s.doc_mut().unwrap().remove_entity(source);
                }
                "not owned" => {
                    s.doc_mut().unwrap().modify_entity(table, |e| e.common.xdata.clear()).unwrap();
                }
                "invalid ownership metadata" => {
                    s.doc_mut()
                        .unwrap()
                        .modify_entity(table, |e| {
                            e.common.xdata = vec![RawTag { code: 1001, value: APP.into() }, RawTag { code: 1070, value: "1".into() }];
                        })
                        .unwrap();
                }
                "not a table" => {
                    s.doc_mut()
                        .unwrap()
                        .modify_entity(table, |e| {
                            e.kind = EntityKind::Circle(cadcraft_doc::Circle { center: cadcraft_geom::Vec3::ZERO, radius: 1.0 });
                        })
                        .unwrap();
                }
                _ => panic!("unexpected refresh rejection case: {reason}"),
            }
            // Keep a real redo entry, so a rejected refresh cannot silently discard it.
            s.execute("circle", &json!({"center":[5,5],"radius":1})).unwrap();
            s.undo().unwrap();
            s.set_selection(vec![table]);
            let before = s.state().unwrap().clone();
            let error = s.execute("datawizard", &json!({"refresh":table.hex()})).unwrap_err();
            assert!(error.to_string().contains(reason), "{reason}: {error}");
            let after = s.state().unwrap();
            assert_eq!(after.doc, before.doc);
            assert_eq!(after.selection, before.selection);
            assert_eq!(after.undo.len(), before.undo.len());
            assert_eq!(after.redo.len(), before.redo.len());
            for (actual, expected) in after.undo.iter().chain(&after.redo).zip(before.undo.iter().chain(&before.redo)) {
                assert_eq!(actual.label, expected.label);
                assert_eq!(actual.doc, expected.doc);
                assert_eq!(actual.selection, expected.selection);
            }
        }
    }

    #[test]
    fn bad_sources_and_unowned_refresh_do_not_mutate() {
        let mut s = Session::new();
        s.execute("table", &json!({"at":[0,0],"rows":1,"cols":1})).unwrap();
        let h = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
        let before = s.doc().unwrap().clone();
        assert!(s.execute("datawizard", &json!({"refresh":h})).is_err());
        assert!(
            s.execute("datawizard", &json!({"settings":{"version":1,"handles":["FFFFFF"],"columns":[{"property":"length","title":"L"}]}})).is_err()
        );
        assert_eq!(s.doc().unwrap().model, before.model);
    }

    #[test]
    fn grouping_and_numeric_sort_before_display_format() {
        let mut s = Session::new();
        for x in [10.0, 2.0, 2.0] {
            s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3 { x, y: 0.0, z: 0.0 }, angle: 0.0 })).unwrap();
        }
        let cfg = settings(json!({"version":1,"columns":[{"property":"x","title":"X","decimals":1}],"group_by":["x"],"sort_by":["x"]})).unwrap();
        let t = extract(&s, &cfg, None).unwrap();
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.cell(1, 0).text(), "2.0");
        assert_eq!(t.cell(2, 0).text(), "10.0");
    }
}
