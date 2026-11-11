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
    data.definition.length().map_err(|e| bad(CMD, e.to_string()))?;
    // Ordinary command errors do not restore Session's snapshot. Stage fallible additions.
    let mut d = s.doc()?.clone();
    let handle = add(&mut d, &s.space(), s.current_common(), &data)?;
    *s.doc_mut()? = d;
    s.set_selection(vec![handle]);
    Ok(json!({"handle":handle.hex(),"length":data.definition.length().map_err(|e|bad(CMD,e.to_string()))?,"elements":data.definition.elements.len()}))
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
    let data = alignment_store::load(s.doc()?, h)?;
    let a = &data.definition;
    let mut points = Vec::new();
    for v in list(p, "stations", CMD)? {
        let station = if v.is_number() { v.as_f64().ok_or_else(|| bad(CMD, "invalid station"))? } else { scalar(v, "station", f64::NAN, CMD)? };
        let offset = if v.is_object() { scalar(v, "offset", 0., CMD)? } else { 0. };
        let pose = a.at(station, offset).map_err(|e| bad(CMD, e.to_string()))?;
        points.push(json!({"station":station,"offset":offset,"point":pose.point,"direction":pose.direction,"display":a.format.display(station).map_err(|e|bad(CMD,e.to_string()))?}));
    }
    let mut projections = Vec::new();
    for v in list(p, "points", CMD)? {
        let point = super::subsurface_draw::point(&json!({"point":v}), "point", None, CMD)?;
        projections.push(a.project(point).map_err(|e| bad(CMD, e.to_string()))?);
    }
    let mut station = a.start_station;
    let mut elements = Vec::new();
    for (index, e) in a.elements.iter().enumerate() {
        elements.push(json!({"index":index,"start_station":station,"end_station":station+e.length,"element":e,"end":e.at(e.length).map_err(|e|bad(CMD,e.to_string()))?}));
        station += e.length;
    }
    Ok(
        json!({"handle":h.hex(),"definition":a,"length":a.length().map_err(|e|bad(CMD,e.to_string()))?,"elements":elements,"points_at_stations":points,"projections":projections}),
    )
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
    use serde_json::json;
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
        // COPY currently copies Common/kind but does not copy foreign extension dictionaries.
        assert!(super::super::alignment_store::load(s.doc().unwrap(), copied).is_err());
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
}
