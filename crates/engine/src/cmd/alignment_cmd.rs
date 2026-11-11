//! Horizontal alignment commands. Geometry lives in cadcraft-alignment.
use super::{
    CommandSpec,
    alignment_store::{self, Stored},
    bad, str_param,
};
use crate::{Result, Session};
use cadcraft_alignment::{
    Alignment, Element, Kind, MAX_ELEMENTS, StationFormat, Turn,
    landxml::{self, Unit},
};
use cadcraft_doc::{Drawing, EntityKind, Handle, LwPolyline};
use cadcraft_geom::{Vec3, bulge_to_arc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, TAU};

pub fn specs() -> Vec<CommandSpec> {
    vec![
    CommandSpec::new("alignmentcreate","Create Alignment",create).params("{name,start_station?:0,format?:thousands|hundreds,chord_tolerance?:0.01,elements:[{start:[x,y],direction:radians,length,type:line|arc|spiral,radius?,turn?:left|right,radius_start?,radius_end?}] OR polyline:handle (else selection)}"),
    CommandSpec::new("alignmentreport","Alignment Report",report).noundo().params("{alignment:handle,stations?:[station|{station,offset}],points?:[[x,y]]} -> {definition,length,elements,points_at_stations,projections}"),
    CommandSpec::new("alignmentimport","Import Alignments from LandXML",import).params("{xml OR path,unit?:meter|foot|USSurveyFoot (drawing coordinate unit; default meter),chord_tolerance?:0.01} -> {handles,refusals,warnings}"),
    CommandSpec::new("alignmentexport","Export Alignments to LandXML",export).noundo().params("{alignment:handle OR alignments:[handle] (else selection),unit?:meter|foot|USSurveyFoot (default meter),path?} -> {xml OR path}"),
]
}
pub(super) fn scalar(p: &Value, key: &str, default: f64, cmd: &str) -> Result<f64> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad(cmd, format!("{key} requires a finite number"))),
    }
}
pub(super) fn selected(s: &Session, p: &Value, key: &str, cmd: &str) -> Result<Handle> {
    if p.get(key).is_some() {
        return super::curves::handle_param(p, key).ok_or_else(|| bad(cmd, format!("{key} requires a handle")));
    }
    match s.selection().as_slice() {
        [h] => Ok(*h),
        _ => Err(bad(cmd, format!("give {key} or select exactly one object"))),
    }
}
fn format(p: &Value) -> Result<StationFormat> {
    match p.get("format").and_then(Value::as_str) {
        None if p.get("format").is_none() => Ok(StationFormat::Thousands),
        Some("thousands") => Ok(StationFormat::Thousands),
        Some("hundreds") => Ok(StationFormat::Hundreds),
        _ => Err(bad("alignmentcreate", "format must be thousands or hundreds")),
    }
}
pub(super) fn add(d: &mut Drawing, space: &cadcraft_doc::Space, common: cadcraft_doc::Common, data: &Stored) -> Result<Handle> {
    let kind = EntityKind::LwPolyline(alignment_store::carrier(data)?);
    let h = d.add(space, common, kind)?;
    alignment_store::save(d, h, data)?;
    Ok(h)
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "alignmentcreate";
    let name = p
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty() && n.len() <= 256)
        .ok_or_else(|| bad(CMD, "name required, at most 256 bytes"))?;
    let elements = if let Some(raw) = p.get("elements") {
        if p.get("polyline").is_some() {
            return Err(bad(CMD, "specify elements or polyline, not both"));
        }
        let list =
            raw.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_ELEMENTS).ok_or_else(|| bad(CMD, "elements must contain 1..4096 entries"))?;
        for v in list {
            if v.as_object().is_none_or(|o| o.len() > 12) {
                return Err(bad(CMD, "invalid element object"));
            }
        }
        Vec::<Element>::deserialize(raw).map_err(|e| bad(CMD, e.to_string()))?
    } else {
        let h = selected(s, p, "polyline", CMD)?;
        let e = s.doc()?.entity(h).ok_or_else(|| bad(CMD, "polyline missing"))?;
        let EntityKind::LwPolyline(polyline) = &e.kind else { return Err(bad(CMD, "a lightweight polyline of lines/arcs is required")) };
        if e.common.extrusion != Vec3::Z {
            return Err(bad(CMD, "polyline must lie in the XY plane"));
        }
        from_polyline(polyline)?
    };
    let data = Stored {
        version: 1,
        definition: Alignment { name: name.into(), start_station: scalar(p, "start_station", 0., CMD)?, format: format(p)?, elements },
        chord_tolerance: scalar(p, "chord_tolerance", 0.01, CMD)?,
    };
    // Ordinary command errors do not restore Session's snapshot. Stage fallible additions.
    let mut d = s.doc()?.clone();
    let handle = add(&mut d, &s.space(), s.current_common(), &data)?;
    *s.doc_mut()? = d;
    s.set_selection(vec![handle]);
    let length: f64 = data.definition.elements.iter().map(|e| e.length).sum();
    Ok(json!({"handle":handle.hex(),"length":length,"elements":data.definition.elements.len()}))
}
fn from_polyline(p: &LwPolyline) -> Result<Vec<Element>> {
    const CMD: &str = "alignmentcreate";
    if p.elevation != 0. || p.vertices.len() < 2 || p.vertices.len() > MAX_ELEMENTS + 1 {
        return Err(bad(CMD, "polyline must have 2..4097 XY vertices at zero elevation"));
    }
    let count = p.vertices.len() - usize::from(!p.closed);
    if count > MAX_ELEMENTS {
        return Err(bad(CMD, "more than 4096 segments"));
    }
    let mut out = Vec::new();
    for i in 0..count {
        let a = p.vertices.get(i).ok_or_else(|| bad(CMD, "missing vertex"))?;
        let b = p.vertices.get((i + 1) % p.vertices.len()).ok_or_else(|| bad(CMD, "missing next vertex"))?;
        if !a.p.x.is_finite() || !a.p.y.is_finite() || !b.p.x.is_finite() || !b.p.y.is_finite() || !a.bulge.is_finite() {
            return Err(bad(CMD, "nonfinite polyline geometry"));
        }
        let chord = a.p.dist(b.p);
        let (direction, length, kind) = if a.bulge == 0. {
            ((b.p.y - a.p.y).atan2(b.p.x - a.p.x), chord, Kind::Line)
        } else {
            let (arc, left) = bulge_to_arc(a.p, b.p, a.bulge).ok_or_else(|| bad(CMD, "unrepresentable or degenerate arc segment"))?;
            let turn = if left { Turn::Left } else { Turn::Right };
            let sign = if left { 1. } else { -1. };
            let d = (a.p.y - arc.center.y).atan2(a.p.x - arc.center.x) + sign * FRAC_PI_2;
            (d, 4. * a.bulge.atan().abs() * arc.radius, Kind::Arc { radius: arc.radius, turn })
        };
        out.push(Element { start: [a.p.x, a.p.y], direction: direction.rem_euclid(TAU), length, kind });
    }
    Ok(out)
}
pub(super) fn list<'a>(p: &'a Value, key: &str, cmd: &str) -> Result<&'a [Value]> {
    match p.get(key) {
        None => Ok(&[]),
        Some(Value::Array(a)) if a.len() <= 4096 => Ok(a),
        _ => Err(bad(cmd, format!("{key} must be an array of at most 4096 values"))),
    }
}
fn report(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "alignmentreport";
    let h = selected(s, p, "alignment", CMD)?;
    let data = alignment_store::load_data(s.doc()?, h)?;
    let view = alignment_store::validated(s.doc()?, h, &data)?;
    let a = &data.definition;
    let mut points = Vec::new();
    for v in list(p, "stations", CMD)? {
        let station = if v.is_number() { v.as_f64().ok_or_else(|| bad(CMD, "invalid station"))? } else { scalar(v, "station", f64::NAN, CMD)? };
        let offset = if v.is_object() { scalar(v, "offset", 0., CMD)? } else { 0. };
        let pose = view.at(station, offset).map_err(|e| bad(CMD, e.to_string()))?;
        points.push(json!({"station":station,"offset":offset,"point":pose.point,"direction":pose.direction,"display":a.format.display(station).map_err(|e|bad(CMD,e.to_string()))?}));
    }
    let mut projections = Vec::new();
    for v in list(p, "points", CMD)? {
        let point = super::subsurface_draw::point(&json!({"point":v}), "point", None, CMD)?;
        projections.push(view.project(point).map_err(|e| bad(CMD, e.to_string()))?);
    }
    // Use the same local length sum as station queries, adding the origin only once.
    let mut distance_along = 0.;
    let mut elements = Vec::new();
    for (index, e) in a.elements.iter().enumerate() {
        elements.push(json!({"index":index,"start_station":a.start_station+distance_along,"end_station":a.start_station+(distance_along+e.length),"element":e,"end":e.at(e.length).map_err(|e|bad(CMD,e.to_string()))?}));
        distance_along += e.length;
    }
    Ok(json!({"handle":h.hex(),"definition":a,"length":view.length(),"elements":elements,"points_at_stations":points,"projections":projections}))
}
fn unit(p: &Value, cmd: &str) -> Result<Unit> {
    match p.get("unit") {
        None => Ok(Unit::Meter),
        Some(Value::String(u)) => Unit::parse(u).ok_or_else(|| bad(cmd, "unit must be meter, foot or USSurveyFoot")),
        _ => Err(bad(cmd, "invalid unit")),
    }
}
fn import(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "alignmentimport";
    if p.get("xml").is_some() && p.get("path").is_some() {
        return Err(bad(CMD, "give xml or path, not both"));
    }
    let bytes = match p.get("xml") {
        Some(Value::String(xml)) if xml.len() <= landxml::MAX_XML_BYTES => xml.as_bytes().to_vec(),
        Some(_) => return Err(bad(CMD, "xml must be a string of at most 16MiB")),
        None => read_xml(p)?,
    };
    let result = landxml::import(&bytes, unit(p, CMD)?).map_err(|e| bad(CMD, e.to_string()))?;
    let tolerance = scalar(p, "chord_tolerance", 0.01, CMD)?;
    let mut d = s.doc()?.clone();
    let mut handles = Vec::new();
    for a in result.alignments {
        let data = Stored { version: 1, definition: a, chord_tolerance: tolerance };
        handles.push(add(&mut d, &s.space(), s.current_common(), &data)?);
    }
    if !handles.is_empty() {
        *s.doc_mut()? = d;
        s.set_selection(handles.clone());
    }
    Ok(json!({"handles":handles.iter().map(|h|h.hex()).collect::<Vec<_>>(),"refusals":result.refusals,"warnings":result.warnings}))
}
fn read_xml(p: &Value) -> Result<Vec<u8>> {
    let path = str_param(p, "path").ok_or_else(|| bad("alignmentimport", "xml or path required"))?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::capped_read::read_capped(std::path::Path::new(path), landxml::MAX_XML_BYTES).map_err(|e| bad("alignmentimport", e.to_string()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(bad("alignmentimport", format!("{path}: paths unavailable on web; supply xml")))
    }
}
fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "alignmentexport";
    let handles = if p.get("alignment").is_some() {
        vec![selected(s, p, "alignment", CMD)?]
    } else if p.get("alignments").is_some() {
        list(p, "alignments", CMD)?
            .iter()
            .map(|v| super::curves::handle_value(v).ok_or_else(|| bad(CMD, "invalid handle")))
            .collect::<Result<Vec<_>>>()?
    } else {
        s.selection()
    };
    if handles.is_empty() || handles.len() > 128 {
        return Err(bad(CMD, "select 1..128 alignments"));
    }
    let definitions = handles.into_iter().map(|h| alignment_store::load(s.doc()?, h).map(|d| d.definition)).collect::<Result<Vec<_>>>()?;
    let xml = landxml::export(&definitions, unit(p, CMD)?).map_err(|e| bad(CMD, e.to_string()))?;
    match p.get("path") {
        None => Ok(json!({"xml":xml})),
        Some(Value::String(path)) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                crate::atomic_write::write_atomic_making_folders(std::path::Path::new(path), xml.as_bytes()).map_err(|e| bad(CMD, e.to_string()))?;
                Ok(json!({"path":path}))
            }
            #[cfg(target_arch = "wasm32")]
            {
                Err(bad(CMD, format!("{path}: paths unavailable on web; omit path")))
            }
        }
        _ => Err(bad(CMD, "path must be a string")),
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use cadcraft_doc::{EntityKind, Handle, RawTag, foreign::ForeignObject};
    use serde_json::json;
    use std::sync::Arc;

    fn line(s: &mut Session, name: &str) -> Handle {
        let r = s.execute("alignmentcreate", &json!({"name":name,"elements":[{"start":[0,0],"direction":0,"length":10,"type":"line"}]})).unwrap();
        Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn copy_translates_exact_definition_and_duplicates_owned_extension_tree() {
        let mut s = Session::new();
        let h = line(&mut s, "route");
        let external = s.add_entity(cadcraft_doc::EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3::ZERO, angle: 0. })).unwrap();
        let dict = s.doc().unwrap().foreign.entities.get(&h).unwrap().xdict.unwrap();
        let other = s.doc_mut().unwrap().new_handle();
        let tag = |code, value: String| RawTag { code, value };
        let f = Arc::make_mut(&mut s.doc_mut().unwrap().foreign);
        f.objects.iter_mut().find(|o| o.handle == dict).unwrap().tags.extend([tag(3, "OTHER_APP".into()), tag(360, other.hex())]);
        f.objects.push(ForeignObject {
            handle: other,
            kind: "XRECORD".into(),
            tags: vec![
                tag(5, other.hex()),
                tag(330, dict.hex()),
                tag(100, "AcDbXrecord".into()),
                tag(340, external.hex()),
                tag(1, "untouched".into()),
            ],
        });
        let original = s.doc().unwrap().clone();
        let r = s.execute("copy", &json!({"handles":[h.hex()],"delta":[3,4],"count":2})).unwrap();
        let copies: Vec<_> = r["handles"].as_array().unwrap().iter().map(|v| Handle::parse_hex(v.as_str().unwrap()).unwrap()).collect();
        assert_eq!(copies.len(), 2);
        for (i, copy) in copies.iter().enumerate() {
            let d = s.doc().unwrap();
            let data = super::super::alignment_store::load(d, *copy).unwrap();
            assert_eq!(data.definition.elements[0].start, [3. * (i + 1) as f64, 4. * (i + 1) as f64]);
            let cd = d.foreign.entities.get(copy).unwrap().xdict.unwrap();
            assert_ne!(cd, dict);
            let owned = d.foreign.objects.iter().find(|o| o.handle == cd).unwrap();
            let foreign = owned.tags.windows(2).find(|p| p[0].code == 3 && p[0].value == "OTHER_APP").unwrap();
            let child = Handle::parse_hex(&foreign[1].value).unwrap();
            assert_ne!(child, other);
            assert!(d.foreign.objects.iter().any(|o| o.handle == child
                && o.tags.iter().any(|t| t.code == 1 && t.value == "untouched")
                && o.tags.iter().any(|t| t.code == 340 && t.value == external.hex())));
        }
        assert_eq!(super::super::alignment_store::load(s.doc().unwrap(), h).unwrap().definition.elements[0].start, [0., 0.]);
        for file in ["route.dxf", "route.dwg"] {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            let reopened = cadcraft_io::read(&bytes, file).unwrap();
            for copy in &copies {
                assert!(super::super::alignment_store::load(&reopened, *copy).is_ok(), "{file}");
                let cd = reopened.foreign.entities.get(copy).unwrap().xdict.unwrap();
                let owned = reopened.foreign.objects.iter().find(|o| o.handle == cd).unwrap();
                let foreign = owned.tags.windows(2).find(|p| p[0].code == 3 && p[0].value == "OTHER_APP").unwrap();
                let child = Handle::parse_hex(&foreign[1].value).unwrap();
                assert!(
                    reopened.foreign.objects.iter().any(|o| o.handle == child
                        && o.tags.iter().any(|t| t.code == 1 && t.value == "untouched")
                        && o.tags.iter().any(|t| t.code == 340 && t.value == external.hex())),
                    "{file}"
                );
            }
        }
        s.undo().unwrap();
        assert_eq!(*s.doc().unwrap(), original);
        s.redo().unwrap();
        for copy in &copies {
            assert!(super::super::alignment_store::load(s.doc().unwrap(), *copy).is_ok());
        }
        assert_eq!(
            s.doc().unwrap().foreign.objects.iter().find(|o| o.handle == other).unwrap(),
            original.foreign.objects.iter().find(|o| o.handle == other).unwrap()
        );
    }

    #[test]
    fn copy_keeps_reactors_distinct_from_the_extension_dictionary_owner() {
        fn owner_and_reactors(tags: &[RawTag]) -> (Option<Handle>, Vec<Handle>) {
            let mut group = None;
            let mut owner = None;
            let mut reactors = Vec::new();
            for tag in tags.iter().take_while(|tag| tag.code < 1000) {
                if tag.code == 102 {
                    group = tag.value.trim().strip_prefix('{');
                } else if tag.code == 330 {
                    let handle = Handle::parse_hex(tag.value.trim());
                    if group == Some("ACAD_REACTORS") {
                        reactors.extend(handle);
                    } else if group.is_none() && owner.is_none() {
                        owner = handle;
                    }
                }
            }
            (owner, reactors)
        }

        let mut s = Session::new();
        let source = line(&mut s, "reactors");
        let external = s.add_entity(EntityKind::Point(cadcraft_doc::Point { p: cadcraft_geom::Vec3::ZERO, angle: 0. })).unwrap();
        let dict = s.doc().unwrap().foreign.entities.get(&source).unwrap().xdict.unwrap();
        let record = {
            let root = s.doc().unwrap().foreign.objects.iter().find(|o| o.handle == dict).unwrap();
            let entry = root.tags.windows(2).find(|pair| pair[0].code == 3 && pair[0].value == "CADCRAFT_ALIGNMENT").unwrap();
            Handle::parse_hex(&entry[1].value).unwrap()
        };
        let root = Arc::make_mut(&mut s.doc_mut().unwrap().foreign).objects.iter_mut().find(|o| o.handle == dict).unwrap();
        root.tags.splice(
            1..1,
            [
                RawTag { code: 102, value: "{ACAD_REACTORS".into() },
                RawTag { code: 330, value: external.hex() },
                RawTag { code: 330, value: record.hex() },
                RawTag { code: 102, value: "}".into() },
            ],
        );
        let before = s.doc().unwrap().clone();
        let result = s.execute("copy", &json!({"handles":[source.hex()],"delta":[3,4]})).unwrap();
        let copy = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
        for (label, drawing) in std::iter::once(("memory", s.doc().unwrap().clone())).chain(["reactors.dxf", "reactors.dwg"].map(|file| {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            (file, cadcraft_io::read(&bytes, file).unwrap())
        })) {
            let source_dict = drawing.foreign.entities.get(&source).unwrap().xdict.unwrap();
            let copy_dict = drawing.foreign.entities.get(&copy).unwrap().xdict.unwrap();
            assert_ne!(source_dict, copy_dict, "{label}");
            for (carrier, dict) in [(source, source_dict), (copy, copy_dict)] {
                let root = drawing.foreign.objects.iter().find(|o| o.handle == dict).unwrap();
                let entry = root.tags.windows(2).find(|pair| pair[0].code == 3 && pair[0].value == "CADCRAFT_ALIGNMENT").unwrap();
                let expected_record = Handle::parse_hex(&entry[1].value).unwrap();
                let (owner, reactors) = owner_and_reactors(&root.tags);
                assert_eq!(owner, Some(carrier), "{label}");
                assert!(reactors.contains(&external), "{label}: external reactor missing");
                assert!(reactors.contains(&expected_record), "{label}: owned reactor missing");
                assert_eq!(reactors.len(), 2, "{label}");
                super::super::alignment_store::load(&drawing, carrier).unwrap();
            }
        }
        assert_eq!(
            *before.foreign.objects.iter().find(|o| o.handle == dict).unwrap(),
            *s.doc().unwrap().foreign.objects.iter().find(|o| o.handle == dict).unwrap()
        );
    }

    #[test]
    fn copy_refuses_capped_owned_tree_without_partial_batch() {
        let mut s = Session::new();
        let h = line(&mut s, "large");
        let dict = s.doc().unwrap().foreign.entities.get(&h).unwrap().xdict.unwrap();
        let children: Vec<_> = (0..4095).map(|_| s.doc_mut().unwrap().new_handle()).collect();
        let f = Arc::make_mut(&mut s.doc_mut().unwrap().foreign);
        let owned = f.objects.iter_mut().find(|o| o.handle == dict).unwrap();
        for (i, child) in children.iter().enumerate() {
            owned.tags.push(RawTag { code: 3, value: format!("OTHER_{i}") });
            owned.tags.push(RawTag { code: 360, value: child.hex() });
        }
        for child in &children {
            f.objects.push(ForeignObject {
                handle: *child,
                kind: "XRECORD".into(),
                tags: vec![RawTag { code: 5, value: child.hex() }, RawTag { code: 330, value: dict.hex() }],
            });
        }
        s.set_selection(vec![h]);
        let before = s.doc().unwrap().clone();
        let undo = s.state().unwrap().undo.len();
        let err = s.execute("copy", &json!({"handles":[h.hex()],"delta":[0,0],"count":2})).unwrap_err();
        assert!(err.to_string().contains("dictionary"), "{err}");
        assert_eq!(*s.doc().unwrap(), before);
        assert_eq!(s.selection(), vec![h]);
        assert_eq!(s.state().unwrap().undo.len(), undo);
    }

    #[test]
    fn copy_refuses_trailing_alignment_dictionary_marker_without_mutation() {
        let mut s = Session::new();
        let h = line(&mut s, "malformed");
        let dict = s.doc().unwrap().foreign.entities.get(&h).unwrap().xdict.unwrap();
        Arc::make_mut(&mut s.doc_mut().unwrap().foreign)
            .objects
            .iter_mut()
            .find(|o| o.handle == dict)
            .unwrap()
            .tags
            .push(RawTag { code: 3, value: "CADCRAFT_ALIGNMENT".into() });
        s.set_selection(vec![h]);
        let before = s.doc().unwrap().clone();
        let undo = s.state().unwrap().undo.len();
        let err = s.execute("copy", &json!({"handles":[h.hex()],"delta":[0,0]})).unwrap_err();
        assert!(err.to_string().contains("alignment entry"), "{err}");
        assert_eq!(*s.doc().unwrap(), before);
        assert_eq!(s.selection(), vec![h]);
        assert_eq!(s.state().unwrap().undo.len(), undo);
    }

    #[test]
    fn copy_refuses_missing_owned_extension_child_without_mutation() {
        let mut s = Session::new();
        let h = line(&mut s, "missing child");
        let dict = s.doc().unwrap().foreign.entities.get(&h).unwrap().xdict.unwrap();
        let missing = s.doc_mut().unwrap().new_handle();
        Arc::make_mut(&mut s.doc_mut().unwrap().foreign)
            .objects
            .iter_mut()
            .find(|o| o.handle == dict)
            .unwrap()
            .tags
            .extend([RawTag { code: 3, value: "OTHER_APP".into() }, RawTag { code: 360, value: missing.hex() }]);
        s.set_selection(vec![h]);
        let before = s.doc().unwrap().clone();
        let undo = s.state().unwrap().undo.len();
        let err = s.execute("copy", &json!({"handles":[h.hex()],"delta":[0,0]})).unwrap_err();
        assert!(err.to_string().contains("dictionary"), "{err}");
        assert_eq!(*s.doc().unwrap(), before);
        assert_eq!(s.selection(), vec![h]);
        assert_eq!(s.state().unwrap().undo.len(), undo);
    }

    #[test]
    fn copy_refuses_stale_source_without_partial_batch_or_selection_change() {
        let mut s = Session::new();
        let good = line(&mut s, "good");
        let stale = line(&mut s, "stale");
        s.doc_mut()
            .unwrap()
            .modify_entity(stale, |e| {
                if let cadcraft_doc::EntityKind::LwPolyline(p) = &mut e.kind {
                    p.vertices[0].p.x += 1.;
                }
            })
            .unwrap();
        s.set_selection(vec![good, stale]);
        let before = s.doc().unwrap().clone();
        let selected = s.selection();
        let err = s.execute("copy", &json!({"handles":[good.hex(),stale.hex()],"delta":[2,0],"count":2})).unwrap_err();
        assert!(err.to_string().contains("STALE_ALIGNMENT"), "{err}");
        assert_eq!(*s.doc().unwrap(), before);
        assert_eq!(s.selection(), selected);
    }

    #[test]
    fn copy_refuses_later_out_of_range_translation_atomically() {
        let mut s = Session::new();
        let r = s
            .execute("alignmentcreate", &json!({"name":"edge","elements":[{"start":[999999990,0],"direction":0,"length":1,"type":"line"}]}))
            .unwrap();
        let before = s.doc().unwrap().clone();
        let err = s.execute("copy", &json!({"handles":[r["handle"]],"delta":[5,0],"count":3})).unwrap_err();
        assert!(err.to_string().contains("1e9"), "{err}");
        assert_eq!(*s.doc().unwrap(), before);
    }

    #[test]
    fn interactive_copy_repeats_valid_alignment_then_cancel_keeps_placed_copies() {
        use crate::Input;
        use cadcraft_geom::Vec2;
        let mut s = Session::new();
        let h = line(&mut s, "interactive");
        s.set_selection(vec![h]);
        s.start("copy").unwrap();
        s.input(Input::Point(Vec2::ZERO)).unwrap();
        s.input(Input::Point(Vec2::new(5., 0.))).unwrap();
        s.input(Input::Point(Vec2::new(0., 7.))).unwrap();
        s.input(Input::Cancel).unwrap();
        let starts: Vec<_> = s
            .doc()
            .unwrap()
            .model
            .iter()
            .map(|e| super::super::alignment_store::load(s.doc().unwrap(), e.handle).unwrap().definition.elements[0].start)
            .collect();
        assert_eq!(starts, [[0., 0.], [5., 0.], [0., 7.]]);
    }

    #[test]
    fn copy_preserves_both_signed_major_arc_carriers_through_reopen() {
        use super::super::alignment_store;
        for (turn, sign) in [("left", 1.), ("right", -1.)] {
            let mut s = Session::new();
            let created = s
                .execute(
                    "alignmentcreate",
                    &json!({"name":"major arc","elements":[{"start":[0,0],"direction":0,"length":15. * std::f64::consts::PI,"type":"arc","radius":10,"turn":turn}]}),
                )
                .unwrap();
            let source = Handle::parse_hex(created["handle"].as_str().unwrap()).unwrap();
            let original = alignment_store::load(s.doc().unwrap(), source).unwrap();
            let zero = s.execute("copy", &json!({"handles":[source.hex()],"delta":[0,0]})).unwrap();
            let translated = s.execute("copy", &json!({"handles":[source.hex()],"delta":[7,3]})).unwrap();
            let copies = [
                (Handle::parse_hex(zero["handles"][0].as_str().unwrap()).unwrap(), [0., 0.]),
                (Handle::parse_hex(translated["handles"][0].as_str().unwrap()).unwrap(), [7., 3.]),
            ];
            assert_eq!(alignment_store::load(s.doc().unwrap(), source).unwrap().definition.elements[0].start, [0., 0.]);
            for drawing in std::iter::once(s.doc().unwrap().clone()).chain(["arc.dxf", "arc.dwg"].map(|file| {
                let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
                cadcraft_io::read(&bytes, file).unwrap()
            })) {
                let EntityKind::LwPolyline(source_poly) = &drawing.entity(source).unwrap().kind else { panic!("source carrier") };
                assert_eq!(source_poly.vertices.len(), 2);
                for (copy, offset) in &copies {
                    let data = alignment_store::load(&drawing, *copy).unwrap();
                    assert_eq!(data.definition.name, original.definition.name);
                    assert_eq!(data.definition.elements[0].start, *offset);
                    assert_ne!(drawing.foreign.entities.get(&source).unwrap().xdict, drawing.foreign.entities.get(copy).unwrap().xdict);
                    let EntityKind::LwPolyline(poly) = &drawing.entity(*copy).unwrap().kind else { panic!("copied carrier") };
                    assert_eq!(poly.vertices.len(), 2);
                    assert!((poly.vertices[0].bulge - sign * (3. * std::f64::consts::PI / 8.).tan()).abs() < 1e-9);
                    for (actual, before) in poly.vertices.iter().zip(&source_poly.vertices) {
                        assert!((actual.p.x - before.p.x - offset[0]).abs() < 1e-7);
                        assert!((actual.p.y - before.p.y - offset[1]).abs() < 1e-7);
                    }
                }
            }
        }
    }

    #[test]
    fn copy_of_spiral_preserves_exact_definition_and_chorded_carrier() {
        use super::super::alignment_store;
        let mut s = Session::new();
        let created = s
            .execute(
                "alignmentcreate",
                &json!({"name":"spiral","elements":[{"start":[0,0],"direction":0,"length":60,"type":"spiral","radius_start":null,"radius_end":300,"turn":"left"}]}),
            )
            .unwrap();
        let source = Handle::parse_hex(created["handle"].as_str().unwrap()).unwrap();
        let result = s.execute("copy", &json!({"handles":[source.hex()],"delta":[7,3]})).unwrap();
        let copy = Handle::parse_hex(result["handles"][0].as_str().unwrap()).unwrap();
        for drawing in std::iter::once(s.doc().unwrap().clone()).chain(["spiral.dxf", "spiral.dwg"].map(|file| {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            cadcraft_io::read(&bytes, file).unwrap()
        })) {
            let data = alignment_store::load(&drawing, copy).unwrap();
            assert_eq!(data.definition.elements[0].start, [7., 3.]);
            let EntityKind::LwPolyline(before) = &drawing.entity(source).unwrap().kind else { panic!("source carrier") };
            let EntityKind::LwPolyline(after) = &drawing.entity(copy).unwrap().kind else { panic!("copied carrier") };
            assert!(before.vertices.len() > 2);
            assert_eq!(after.vertices.len(), before.vertices.len());
            for (a, b) in after.vertices.iter().zip(&before.vertices) {
                assert_eq!(a.bulge, 0.);
                assert!((a.p.x - b.p.x - 7.).abs() < 1e-7);
                assert!((a.p.y - b.p.y - 3.).abs() < 1e-7);
            }
        }
    }
    #[test]
    fn reported_endpoint_is_usable_as_an_exact_station_query() {
        let mut s = Session::new();
        let created = s
            .execute(
                "alignmentcreate",
                &json!({
                    "name":"fractional", "start_station":1., "elements":[
                        {"start":[0.,0.],"direction":0.,"length":0.1,"type":"line"},
                        {"start":[0.1,0.],"direction":0.,"length":0.1,"type":"line"}
                    ]
                }),
            )
            .unwrap();
        let h = created["handle"].as_str().unwrap();
        let report = s.execute("alignmentreport", &json!({"alignment":h})).unwrap();
        let end = report["elements"][1]["end_station"].as_f64().unwrap();
        assert_eq!(end, 1.2);
        let queried = s.execute("alignmentreport", &json!({"alignment":h,"stations":[end]})).unwrap();
        assert_eq!(queried["points_at_stations"][0]["point"], json!([0.2, 0.]));
    }
    #[test]
    fn selected_polyline_arc_copy_and_move_are_explicit() {
        use cadcraft_doc::{EntityKind, LwPolyline};
        use cadcraft_geom::{PolyVertex, Vec2};
        let mut s = Session::new();
        let source = s
            .add_entity(EntityKind::LwPolyline(LwPolyline {
                vertices: vec![
                    PolyVertex::new(Vec2::ZERO),
                    PolyVertex::with_bulge(Vec2::new(10., 0.), (std::f64::consts::PI / 8.).tan()),
                    PolyVertex::new(Vec2::new(20., 10.)),
                    PolyVertex::new(Vec2::new(20., 20.)),
                ],
                closed: false,
                const_width: 0.,
                elevation: 0.,
                plinegen: false,
            }))
            .unwrap();
        s.set_selection(vec![source]);
        let created = s.execute("alignmentcreate", &json!({"name":"from polyline"})).unwrap();
        let h = created["handle"].as_str().unwrap();
        let report = s.execute("alignmentreport", &json!({"alignment":h})).unwrap();
        assert_eq!(report["elements"].as_array().unwrap().len(), 3);
        assert!((report["length"].as_f64().unwrap() - (20. + 5. * std::f64::consts::PI)).abs() < 1e-6);
        let before = s.doc().unwrap().model.len();
        s.execute("copy", &json!({"handles":[h],"delta":[0,0]})).unwrap();
        assert_eq!(s.doc().unwrap().model.len(), before + 1);
        let original = cadcraft_doc::Handle::parse_hex(h).unwrap();
        let copied = s.doc().unwrap().model.iter().find(|e| e.handle != original && e.handle != source).unwrap().handle;
        let copied_data = super::super::alignment_store::load(s.doc().unwrap(), copied).unwrap();
        assert_eq!(copied_data.definition.name, "from polyline");
        assert_eq!(copied_data.definition.elements[0].start, [0., 0.]);
        s.execute("move", &json!({"handles":[h],"delta":[1,0]})).unwrap();
        assert!(s.execute("alignmentreport", &json!({"alignment":h})).unwrap_err().to_string().contains("STALE_ALIGNMENT"));
        s.undo().unwrap();
        assert!(s.execute("alignmentreport", &json!({"alignment":h})).is_ok());
        s.doc_mut()
            .unwrap()
            .modify_entity(original, |e| {
                if let EntityKind::LwPolyline(p) = &mut e.kind {
                    p.vertices[0].start_width = 1.;
                }
            })
            .unwrap();
        assert!(s.execute("alignmentreport", &json!({"alignment":h})).unwrap_err().to_string().contains("STALE_ALIGNMENT"));
    }
    #[test]
    fn commands_create_report_exchange_reopen_and_reject_stale_carrier() {
        let mut s = Session::new();
        let p = json!({"name":"Route 路线","start_station":100.,"elements":[{"start":[0,0],"direction":0,"length":10,"type":"line"},{"start":[10,0],"direction":0,"length":15.707963267948966,"type":"arc","radius":10,"turn":"left"}]});
        let created = s.execute("alignmentcreate", &p).unwrap();
        let h = created["handle"].as_str().unwrap();
        let report = s.execute("alignmentreport", &json!({"alignment":h,"stations":[105],"points":[[5,2]]})).unwrap();
        assert_eq!(report["points_at_stations"][0]["point"], json!([5., 0.]));
        assert!((report["projections"][0]["offset"].as_f64().unwrap() - 2.).abs() < 1e-6);
        let exported = s.execute("alignmentexport", &json!({"alignment":h})).unwrap();
        let mut other = Session::new();
        let imported = other.execute("alignmentimport", &json!({"xml":exported["xml"]})).unwrap();
        assert_eq!(imported["handles"].as_array().unwrap().len(), 1);
        for file in ["route.dxf", "route.dwg"] {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            let d = cadcraft_io::read(&bytes, file).unwrap();
            let carrier = d.model.iter().find(|e| matches!(e.kind, cadcraft_doc::EntityKind::LwPolyline(_))).unwrap().handle;
            let data = super::super::alignment_store::load(&d, carrier).unwrap();
            assert_eq!(data.definition.name, "Route 路线");
        }
        let handle = cadcraft_doc::Handle::parse_hex(h).unwrap();
        s.doc_mut()
            .unwrap()
            .modify_entity(handle, |e| {
                if let cadcraft_doc::EntityKind::LwPolyline(p) = &mut e.kind {
                    p.vertices[0].p.x += 1.;
                }
            })
            .unwrap();
        assert!(s.execute("alignmentreport", &json!({"alignment":h})).unwrap_err().to_string().contains("STALE_ALIGNMENT"));
    }
    #[test]
    fn failed_creation_changes_nothing_and_success_is_undoable() {
        let mut s = Session::new();
        let before = s.doc().unwrap().clone();
        assert!(s.execute("alignmentcreate", &json!({"name":"bad","elements":[{"start":[0,0],"direction":0,"length":0,"type":"line"}]})).is_err());
        assert_eq!(*s.doc().unwrap(), before);
        s.execute("alignmentcreate", &json!({"name":"ok","elements":[{"start":[0,0],"direction":0,"length":5,"type":"line"}]})).unwrap();
        assert!(s.doc().unwrap().model.len() > before.model.len());
        s.undo().unwrap();
        assert_eq!(*s.doc().unwrap(), before);
    }

    #[test]
    fn major_arc_carrier_survives_both_formats_and_checks_bulge_edits() {
        use super::super::alignment_store;
        use cadcraft_doc::EntityKind;
        for turn in ["left", "right"] {
            let mut s = Session::new();
            let created = s
                .execute(
                    "alignmentcreate",
                    &json!({
                        "name":"major arc", "elements":[{
                            "start":[0.,0.],"direction":0.,"length":15.*std::f64::consts::PI,
                            "type":"arc","radius":10.,"turn":turn
                        }]
                    }),
                )
                .unwrap();
            let h = cadcraft_doc::Handle::parse_hex(created["handle"].as_str().unwrap()).unwrap();
            let sign = if turn == "left" { 1. } else { -1. };
            let expected = sign * (1. + 2f64.sqrt());
            for file in ["major.dxf", "major.dwg"] {
                let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
                let mut d = cadcraft_io::read(&bytes, file).unwrap();
                let e = d.entity(h).unwrap();
                let EntityKind::LwPolyline(p) = &e.kind else { panic!("carrier is not a polyline") };
                assert_eq!(p.vertices.len(), 2);
                assert!((p.vertices[0].bulge - expected).abs() < 1e-9);
                alignment_store::load(&d, h).unwrap();
                d.modify_entity(h, |e| {
                    if let EntityKind::LwPolyline(p) = &mut e.kind {
                        p.vertices[0].bulge += 5e-10;
                    }
                })
                .unwrap();
                alignment_store::load(&d, h).unwrap();
                d.modify_entity(h, |e| {
                    if let EntityKind::LwPolyline(p) = &mut e.kind {
                        p.vertices[0].bulge += 1e-6;
                    }
                })
                .unwrap();
                assert!(alignment_store::load(&d, h).unwrap_err().to_string().contains("STALE_ALIGNMENT"));
            }
        }
    }
}
