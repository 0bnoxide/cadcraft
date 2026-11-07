//! LABELBATCH: label drawing objects from a template. Owner: Codex (see
//! `docs/agents/slice-4-labels.md`).

use super::CommandSpec;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("labelbatch", "Batch Labels", run).params("{template:{id,source,content,height,leader?,boundary?},properties?:{point_handle:{feature_code,...}}} or {delete:template_id}; model space; bearing clockwise from north in degrees")]
}

use super::{
    bad,
    labels_callout::{literal, measure, text},
};
use crate::{Result, Session};
use cadcraft_doc::{EntityKind, Handle, MLeader, RawTag};
use cadcraft_geom::{Polyline, Vec3};
use cadcraft_labels::{
    Pt,
    format::{Props, format},
    placement::{MAX_LABELS, Request, place},
    template::{APP_ID, Owner, Source, Template, decode_owner, encode_owner},
};
use serde_json::{Value, json};

fn owner(tags: &[RawTag]) -> Option<Owner> {
    let start = tags.iter().position(|t| t.code == 1001 && t.value == APP_ID)?;
    let group = tags.get(start + 1..)?.iter().take_while(|t| t.code != 1001);
    let mut chunks = Vec::new();
    for t in group {
        if t.code != 1000 || chunks.len() >= 8 {
            return None;
        }
        chunks.push(t.value.clone());
    }
    decode_owner(&chunks)
}

fn tags(o: &Owner) -> Result<Vec<RawTag>> {
    let mut tags = vec![RawTag { code: 1001, value: APP_ID.into() }];
    tags.extend(encode_owner(o).map_err(|e| bad("labelbatch", e))?.into_iter().map(|value| RawTag { code: 1000, value }));
    Ok(tags)
}

struct Sample {
    key: String,
    at: Pt,
    props: Props,
}

fn samples(s: &Session, t: &Template, p: &Value) -> Result<Vec<Sample>> {
    let mut out = Vec::new();
    let extra = p.get("properties").and_then(Value::as_object);
    let mut push = |key: String, at: Vec3, mut props: Props| -> Result<()> {
        if out.len() >= MAX_LABELS || !at.x.is_finite() || !at.y.is_finite() || !at.z.is_finite() {
            return Err(bad("labelbatch", "source limit or nonfinite coordinate"));
        }
        props.insert("x".into(), json!(at.x));
        props.insert("y".into(), json!(at.y));
        props.insert("z".into(), json!(at.z));
        out.push(Sample { key, at: Pt { x: at.x, y: at.y }, props });
        Ok(())
    };
    match &t.source {
        Source::Points { layers, codes } => {
            for e in s.doc()?.model.iter() {
                if let EntityKind::Point(pt) = &e.kind {
                    if !layers.is_empty() && !layers.contains(&e.common.layer) {
                        continue;
                    }
                    let mut props = extra.and_then(|m| m.get(&e.handle.hex())).and_then(Value::as_object).cloned().unwrap_or_default();
                    if !codes.is_empty() && !props.get("feature_code").and_then(Value::as_str).is_some_and(|c| codes.iter().any(|x| x == c)) {
                        continue;
                    }
                    props.insert("handle".into(), json!(e.handle.hex()));
                    props.insert("layer".into(), json!(e.common.layer));
                    push(e.handle.hex(), pt.p, props)?;
                }
            }
        }
        Source::Segments { layers } => {
            for e in s.doc()?.model.iter() {
                if !layers.is_empty() && !layers.contains(&e.common.layer) {
                    continue;
                }
                if let EntityKind::Polyline3d(pl) = &e.kind {
                    if pl.points.len() > MAX_LABELS {
                        return Err(bad("labelbatch", "too many polyline vertices"));
                    }
                    let mut pairs: Vec<_> = pl.points.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?))).collect();
                    if pl.closed
                        && let (Some(a), Some(b)) = (pl.points.last(), pl.points.first())
                    {
                        pairs.push((*a, *b));
                    }
                    for (i, (a, b)) in pairs.into_iter().enumerate() {
                        let mut props = Props::new();
                        props.insert("length".into(), json!((b.x - a.x).hypot(b.y - a.y).hypot(b.z - a.z)));
                        props.insert("bearing".into(), json!((b.x - a.x).atan2(b.y - a.y).to_degrees().rem_euclid(360.0)));
                        push(
                            format!("{}:segment:{i}", e.handle.hex()),
                            Vec3 { x: a.x / 2.0 + b.x / 2.0, y: a.y / 2.0 + b.y / 2.0, z: a.z / 2.0 + b.z / 2.0 },
                            props,
                        )?;
                    }
                    continue;
                }
                let (segments, elevation) = match &e.kind {
                    EntityKind::LwPolyline(pl) => {
                        if pl.vertices.len() > MAX_LABELS {
                            return Err(bad("labelbatch", "too many polyline vertices"));
                        }
                        (Polyline { vertices: pl.vertices.clone(), closed: pl.closed }.segments(), pl.elevation)
                    }
                    _ => continue,
                };
                for (i, seg) in segments.iter().enumerate() {
                    let a = seg.start();
                    let b = seg.end();
                    let at = seg.mid();
                    let mut props = Props::new();
                    props.insert("length".into(), json!(seg.len()));
                    props.insert("bearing".into(), json!((b.x - a.x).atan2(b.y - a.y).to_degrees().rem_euclid(360.0)));
                    push(format!("{}:segment:{i}", e.handle.hex()), Vec3 { x: at.x, y: at.y, z: elevation }, props)?;
                }
            }
        }
        Source::SurfaceVertices { handle } | Source::SurfaceGrid { handle, .. } => {
            let h = Handle::parse_hex(handle).ok_or_else(|| bad("labelbatch", "invalid surface handle"))?;
            let tin = super::surface_build::tin_from_handle(s.doc()?, h)?;
            match &t.source {
                Source::SurfaceVertices { .. } => {
                    for (i, v) in tin.vertices.iter().enumerate() {
                        let [x, y, z] = *v;
                        push(format!("{handle}:vertex:{i}"), Vec3 { x, y, z }, Props::new())?;
                    }
                }
                Source::SurfaceGrid { spacing, .. } => {
                    let min_x = tin.vertices.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min);
                    let max_x = tin.vertices.iter().map(|v| v[0]).fold(f64::NEG_INFINITY, f64::max);
                    let min_y = tin.vertices.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min);
                    let max_y = tin.vertices.iter().map(|v| v[1]).fold(f64::NEG_INFINITY, f64::max);
                    let nx = ((max_x - min_x) / spacing).floor();
                    let ny = ((max_y - min_y) / spacing).floor();
                    if !nx.is_finite() || !ny.is_finite() || nx < 0.0 || ny < 0.0 || (nx + 1.0) * (ny + 1.0) > MAX_LABELS as f64 {
                        return Err(bad("labelbatch", "surface grid exceeds label limit"));
                    }
                    let mut points = Vec::new();
                    let mut keys = Vec::new();
                    for iy in 0..=ny as usize {
                        for ix in 0..=nx as usize {
                            points.push([min_x + ix as f64 * spacing, min_y + iy as f64 * spacing]);
                            keys.push(format!("{handle}:grid:{ix}:{iy}"));
                        }
                    }
                    let heights = cadcraft_surface::compare::elevations(&tin, &points).map_err(|e| bad("labelbatch", e.to_string()))?;
                    for ((xy, key), z) in points.iter().zip(keys).zip(heights) {
                        if let Some(z) = z {
                            push(key, Vec3 { x: xy[0], y: xy[1], z }, Props::new())?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    if s.space() != cadcraft_doc::Space::Model {
        return Err(bad("labelbatch", "run this command in model space"));
    }
    if let Some(id) = p.get("delete").and_then(Value::as_str) {
        if id.is_empty() || id.len() > 64 {
            return Err(bad("labelbatch", "invalid template id"));
        }
        let handles: Vec<_> = s.doc()?.model.iter().filter(|e| owner(&e.common.xdata).is_some_and(|o| o.template == id)).map(|e| e.handle).collect();
        for h in &handles {
            s.doc_mut()?.remove_entity(*h);
        }
        return Ok(json!({"deleted":handles.len()}));
    }
    let t: Template = serde_json::from_value(p.get("template").cloned().ok_or_else(|| bad("labelbatch", "template required"))?)
        .map_err(|e| bad("labelbatch", e.to_string()))?;
    t.validate().map_err(|e| bad("labelbatch", e))?;
    let sources = samples(s, &t, p)?;
    let overrides = super::callout_override::index(s.doc()?, &t.id)?;
    let mut texts = Vec::new();
    let mut requests = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let contents = literal(&format(&t.content, &source.props, i as u64));
        let (width, height) = measure(s, &contents, t.height)?;
        texts.push(contents);
        let mut request = Request { anchor: source.at, width, height, origin: None, locked: false };
        if let Some(o) = overrides.get(&source.key) {
            o.apply(&mut request, t.height);
        }
        requests.push(request);
    }
    let placed = place(&requests, &[], t.boundary, t.height, 0.0).map_err(|e| bad("labelbatch", e))?;
    let mut existing: std::collections::BTreeMap<String, Vec<Handle>> = std::collections::BTreeMap::new();
    for e in s.doc()?.model.iter() {
        if let Some(o) = owner(&e.common.xdata)
            && o.template == t.id
            && o.role == "label"
        {
            existing.entry(o.source).or_default().push(e.handle);
        }
    }
    let mut handles = Vec::new();
    let mut updated = 0;
    let mut overlaps = 0;
    for ((source, contents), position) in sources.iter().zip(texts).zip(placed) {
        let anchor = requests.get(position.index).ok_or_else(|| bad("labelbatch", "missing placement request"))?.anchor;
        let at = Pt { x: position.rect.min_x, y: position.rect.max_y };
        let label = text(contents, at, t.height);
        let kind = if t.leader {
            EntityKind::MLeader(MLeader {
                leaders: vec![vec![Vec3 { x: anchor.x, y: anchor.y, z: 0.0 }]],
                landing: label.insert,
                dogleg: 0.0,
                text: Some(label),
                style: "Standard".into(),
                arrow_size: t.height,
                spline: false,
                block: None,
            })
        } else {
            EntityKind::MText(label)
        };
        let o = Owner { version: 1, template: t.id.clone(), source: source.key.clone(), role: "label".into() };
        let matching = existing.remove(&source.key).unwrap_or_default();
        let h = if let Some(&h) = matching.first() {
            s.doc_mut()?.modify_entity(h, |e| e.kind = kind)?;
            for duplicate in matching.iter().skip(1) {
                s.doc_mut()?.remove_entity(*duplicate);
            }
            updated += 1;
            h
        } else {
            let h = s.add_entity(kind)?;
            let own_tags = tags(&o)?;
            s.doc_mut()?.modify_entity(h, |e| e.common.xdata.extend(own_tags))?;
            h
        };
        if position.overlap > 0.0 {
            overlaps += 1;
        }
        handles.push(h.hex());
    }
    let mut deleted = 0;
    for h in existing.into_values().flatten() {
        s.doc_mut()?.remove_entity(h);
        deleted += 1;
    }
    Ok(json!({"handles":handles,"updated":updated,"overlaps":overlaps,"deleted":deleted}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;
    #[test]
    fn override_survives_rerun() {
        let mut s = Session::new();
        s.execute("point", &json!({"at":[1,2,3]})).unwrap();
        let source = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
        s.execute("calloutoverride", &json!({"action":"set","template":"manual","source":source,"anchor":[10,20],"offset":[3,4],"locked":true}))
            .unwrap();
        let p = json!({"template":{"id":"manual","source":{"kind":"points"},"content":[],"height":1.0,"leader":true}});
        let a = s.execute("labelbatch", &p).unwrap();
        let b = s.execute("labelbatch", &p).unwrap();
        assert_eq!(a["handles"], b["handles"]);
        s.execute("purge", &json!({"type":"blocks"})).unwrap();
        assert_eq!(s.execute("calloutoverride", &json!({"action":"list"})).unwrap()["overrides"].as_array().unwrap().len(), 1);
        let h = cadcraft_doc::Handle::parse_hex(a["handles"][0].as_str().unwrap()).unwrap();
        let e = s.doc().unwrap().entity(h).unwrap();
        match &e.kind {
            cadcraft_doc::EntityKind::MLeader(m) => {
                assert!((m.text.as_ref().unwrap().insert.x - 13.0).abs() < 1e-8);
                assert_eq!(m.leaders[0][0].x, 10.0);
                assert_eq!(m.leaders[0][0].y, 20.0);
            }
            _ => panic!("expected label"),
        }
        for name in ["overrides.dxf", "overrides.dwg"] {
            let bytes = cadcraft_io::write(s.doc().unwrap(), name).unwrap();
            let drawing = cadcraft_io::read(&bytes, name).unwrap();
            assert!(super::super::callout_override::find(&drawing, "manual", &source).is_some(), "{name}");
            *s.doc_mut().unwrap() = drawing;
            let rerun = s.execute("labelbatch", &p).unwrap();
            let h = cadcraft_doc::Handle::parse_hex(rerun["handles"][0].as_str().unwrap()).unwrap();
            assert!(
                matches!(&s.doc().unwrap().entity(h).unwrap().kind, cadcraft_doc::EntityKind::MLeader(m) if m.text.as_ref().is_some_and(|t| (t.insert.x-13.0).abs()<1e-8))
            );
        }
    }
    #[test]
    fn batch_rerun_updates_handles_and_delete_keeps_source() {
        let mut s = Session::new();
        s.execute("point", &json!({"at":[1,2,3]})).unwrap();
        let id = "\u{6e2c}".repeat(20);
        let p = json!({"template":{"id":id,"source":{"kind":"points"},"content":[{"kind":"prop","key":"z"}],"height":1.0}});
        let a = s.execute("labelbatch", &p).unwrap();
        let b = s.execute("labelbatch", &p).unwrap();
        assert_eq!(a["handles"], b["handles"]);
        assert_eq!(s.doc().unwrap().model.len(), 2);
        let bytes = cadcraft_io::write(s.doc().unwrap(), "labels.dxf").unwrap();
        let drawing = cadcraft_io::read(&bytes, "labels.dxf").unwrap();
        assert!(drawing.model.iter().any(|e| super::owner(&e.common.xdata).is_some_and(|o| o.template == id)));
        assert!(String::from_utf8_lossy(&bytes).contains("CADCRAFT_LABELS"));
        s.execute("labelbatch", &json!({"delete":id})).unwrap();
        assert_eq!(s.doc().unwrap().model.len(), 1);
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn segment_length_bearing_and_surface_grid() {
        let mut s = Session::new();
        s.add_entity(EntityKind::LwPolyline(cadcraft_doc::LwPolyline {
            vertices: Polyline::from_points(&[cadcraft_geom::Vec2 { x: 0.0, y: 0.0 }, cadcraft_geom::Vec2 { x: 3.0, y: 4.0 }], false).vertices,
            closed: false,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        }))
        .unwrap();
        let mut t: Template = serde_json::from_value(json!({"id":"test","source":{"kind":"segments"},"content":[],"height":1.0})).unwrap();
        let seg = samples(&s, &t, &json!({})).unwrap();
        assert_eq!(seg[0].props["length"], 5.0);
        assert!((seg[0].props["bearing"].as_f64().unwrap() - 36.86989764584402).abs() < 1e-9);
        let h = s
            .add_entity(EntityKind::PolyfaceMesh(cadcraft_doc::PolyfaceMesh {
                vertices: vec![Vec3 { x: 0.0, y: 0.0, z: 1.0 }, Vec3 { x: 4.0, y: 0.0, z: 1.0 }, Vec3 { x: 0.0, y: 4.0, z: 1.0 }],
                faces: vec![[1, 2, 3, 0]],
            }))
            .unwrap();
        t.source = Source::SurfaceGrid { handle: h.hex(), spacing: 2.0 };
        let spots = samples(&s, &t, &json!({})).unwrap();
        assert_eq!(spots.len(), 6);
        assert!(spots.iter().all(|s| s.props["z"] == 1.0));
        t.source = Source::SurfaceVertices { handle: h.hex() };
        assert_eq!(samples(&s, &t, &json!({})).unwrap().len(), 3);
    }

    #[test]
    fn codes_filter_explicit_point_properties() {
        let mut s = Session::new();
        let h = s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: Vec3 { x: 0.0, y: 0.0, z: 0.0 }, angle: 0.0 })).unwrap();
        let t: Template = serde_json::from_value(json!({"id":"test","source":{"kind":"points","codes":["MW"]},"content":[],"height":1.0})).unwrap();
        assert!(samples(&s, &t, &json!({})).unwrap().is_empty());
        let mut properties = serde_json::Map::new();
        properties.insert(h.hex(), json!({"feature_code":"MW"}));
        assert_eq!(samples(&s, &t, &json!({"properties":properties})).unwrap().len(), 1);
    }

    #[test]
    fn three_dimensional_segment_uses_spatial_length() {
        let mut s = Session::new();
        s.add_entity(EntityKind::Polyline3d(cadcraft_doc::Polyline3d {
            points: vec![Vec3 { x: 0.0, y: 0.0, z: 0.0 }, Vec3 { x: 0.0, y: 3.0, z: 4.0 }],
            closed: false,
        }))
        .unwrap();
        let t: Template = serde_json::from_value(json!({"id":"test","source":{"kind":"segments"},"content":[],"height":1.0})).unwrap();
        let rows = samples(&s, &t, &json!({})).unwrap();
        assert_eq!(rows[0].props["length"], 5.0);
        assert_eq!(rows[0].props["z"], 2.0);
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn vanished_sources_are_removed_and_paper_space_is_rejected() {
        let mut s = Session::new();
        let h = s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: Vec3 { x: 0.0, y: 0.0, z: 0.0 }, angle: 0.0 })).unwrap();
        let p = json!({"template":{"id":"test","source":{"kind":"points"},"content":[],"height":1.0}});
        s.execute("labelbatch", &p).unwrap();
        s.doc_mut().unwrap().remove_entity(h);
        assert_eq!(s.execute("labelbatch", &p).unwrap()["deleted"], 1);
        assert!(s.doc().unwrap().model.is_empty());
        s.state_mut().unwrap().space = cadcraft_doc::Space::Paper("Layout1".into());
        assert!(s.execute("labelbatch", &p).is_err());
    }
    #[test]
    fn lightweight_polyline_elevation_reaches_label_properties() {
        let mut s = Session::new();
        s.add_entity(EntityKind::LwPolyline(cadcraft_doc::LwPolyline {
            vertices: Polyline::from_points(&[cadcraft_geom::Vec2 { x: 0.0, y: 0.0 }, cadcraft_geom::Vec2 { x: 3.0, y: 4.0 }], false).vertices,
            closed: false,
            const_width: 0.0,
            elevation: 42.0,
            plinegen: false,
        }))
        .unwrap();
        let t: Template = serde_json::from_value(json!({"id":"test","source":{"kind":"segments"},"content":[],"height":1.0})).unwrap();
        assert_eq!(samples(&s, &t, &json!({})).unwrap()[0].props["z"], 42.0);
    }
}
