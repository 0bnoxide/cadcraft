use super::{
    CommandSpec,
    alignment_cmd::{scalar, selected},
    alignment_store, bad,
};
use crate::{Result, Session};
use cadcraft_alignment::{
    profile::{DesignProfile, Pvi},
    sampling,
    view::{self, Mark},
};
use cadcraft_doc::{Common, EntityKind, Line, LwPolyline};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("profilesample", "Sample Ground Profile", sample).params(
            "{alignment:handle,surface:handle,tolerance?:0.01} -> {profile}; triangle crossings and elevation chord tolerance, gaps retained",
        ),
        CommandSpec::new("profilecreate", "Create Design Profile", create)
            .params("{alignment:handle,name?:Design,pvis:[{station,elevation,curve_length?}]} -> {profile,extrema}; replaces stored design profile"),
        CommandSpec::new("profileview", "Draw Profile View", draw)
            .params("{alignment:handle,at:[x,y],exaggeration?:1} -> {handles}; draws a new figure without erasing any prior one"),
    ]
}
fn sample(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "profilesample";
    let h = selected(s, p, "alignment", CMD)?;
    let mut data = alignment_store::load(s.doc()?, h)?;
    let surface = super::curves::handle_param(p, "surface").ok_or_else(|| bad(CMD, "surface handle required"))?;
    if let Some(e) = s.doc()?.entity(surface)
        && let EntityKind::PolyfaceMesh(mesh) = &e.kind
        && (mesh.vertices.len() > 1_000_000 || mesh.faces.len() > 1_000_000)
    {
        return Err(bad(CMD, "TIN_LIMIT: mesh exceeds 1000000 vertices or faces"));
    }
    let tin = super::surface_build::tin_from_handle(s.doc()?, surface)?;
    let profile =
        sampling::sample(&data.definition, &tin.vertices, &tin.triangles, scalar(p, "tolerance", 0.01, CMD)?).map_err(|e| bad(CMD, e.to_string()))?;
    data.profiles.ground = Some(profile.clone());
    let mut d = s.doc()?.clone();
    alignment_store::save(&mut d, h, &data)?;
    *s.doc_mut()? = d;
    Ok(json!({"profile":profile}))
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "profilecreate";
    let h = selected(s, p, "alignment", CMD)?;
    let mut data = alignment_store::load(s.doc()?, h)?;
    let raw = p
        .get("pvis")
        .filter(|v| v.as_array().is_some_and(|v| v.len() <= cadcraft_alignment::MAX_ELEMENTS))
        .ok_or_else(|| bad(CMD, "pvis array required, at most 4096 entries"))?;
    let pvis = Vec::<Pvi>::deserialize(raw).map_err(|e| bad(CMD, e.to_string()))?;
    let name = match p.get("name") {
        None => "Design",
        Some(Value::String(n)) => n,
        _ => return Err(bad(CMD, "name must be a string")),
    };
    let profile = DesignProfile { name: name.into(), pvis };
    profile.validate(&data.definition).map_err(|e| bad(CMD, e.to_string()))?;
    let extrema = profile.extrema().map_err(|e| bad(CMD, e.to_string()))?;
    data.profiles.design = Some(profile.clone());
    let mut d = s.doc()?.clone();
    alignment_store::save(&mut d, h, &data)?;
    *s.doc_mut()? = d;
    Ok(json!({"profile":profile,"extrema":extrema}))
}
fn draw(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "profileview";
    let h = selected(s, p, "alignment", CMD)?;
    let data = alignment_store::load(s.doc()?, h)?;
    let at = super::subsurface_draw::point(p, "at", None, CMD)?;
    let exaggeration = scalar(p, "exaggeration", 1., CMD)?;
    let marks = view::draw(&data.definition, &data.profiles, at, exaggeration).map_err(|e| bad(CMD, e.to_string()))?;
    let text_height = (data.definition.length().map_err(|e| bad(CMD, e.to_string()))? / 100.).max(0.25);
    let v3 = |p: [f64; 2]| Vec3 { x: p[0], y: p[1], z: 0. };
    let mut d = s.doc()?.clone();
    let mut handles = Vec::new();
    for mark in marks {
        let (layer, kind) = match mark {
            Mark::Line { a, b } => ("PROFILE-GRID", EntityKind::Line(Line { a: v3(a), b: v3(b) })),
            Mark::Text { at, text } => (
                "PROFILE-TEXT",
                EntityKind::MText(super::labels_callout::text(
                    super::labels_callout::literal(&text),
                    cadcraft_labels::Pt { x: at[0], y: at[1] },
                    text_height,
                )),
            ),
            Mark::Polyline { layer, points } if points.len() == 1 => {
                let point = points.first().ok_or_else(|| bad(CMD, "missing contact point"))?;
                (layer, EntityKind::Point(cadcraft_doc::Point { p: v3(*point), angle: 0. }))
            }
            Mark::Polyline { layer, points } => (
                layer,
                EntityKind::LwPolyline(LwPolyline {
                    vertices: points.into_iter().map(|p| PolyVertex::new(Vec2::new(p[0], p[1]))).collect(),
                    closed: false,
                    const_width: 0.,
                    elevation: 0.,
                    plinegen: false,
                }),
            ),
        };
        d.ensure_layer(layer);
        let color = cadcraft_color::Color::from_aci(match layer {
            "PROFILE-GRID" => 8,
            "PROFILE-GROUND" => 3,
            "PROFILE-DESIGN" => 1,
            _ => 256,
        });
        let handle = d.add(&s.space(), Common { layer: layer.into(), color, ..Common::default() }, kind)?;
        handles.push(handle.hex());
    }
    *s.doc_mut()? = d;
    Ok(json!({"handles":handles}))
}
#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;
    #[test]
    fn sample_design_view_undo_and_profile_exchange_reopen() {
        let mut s = Session::new();
        let created =
            s.execute("alignmentcreate", &json!({"name":"Route","elements":[{"start":[0,0],"direction":0,"length":400,"type":"line"}]})).unwrap();
        let h = created["handle"].as_str().unwrap();
        let surface = s.execute("tincreate", &json!({"points":[[-10,-10,0],[410,-10,0],[-10,10,1],[410,10,1]]})).unwrap();
        let sampled = s.execute("profilesample", &json!({"alignment":h,"surface":surface["handle"],"tolerance":0.001})).unwrap();
        assert!(sampled["profile"]["segments"][0].as_array().unwrap().len() >= 3);
        s.execute("profilecreate",&json!({"alignment":h,"pvis":[{"station":0,"elevation":0},{"station":200,"elevation":6,"curve_length":200},{"station":400,"elevation":2}]})).unwrap();
        let before = s.doc().unwrap().clone();
        let view = s.execute("profileview", &json!({"alignment":h,"at":[1000,1000],"exaggeration":2})).unwrap();
        assert!(view["handles"].as_array().unwrap().len() > 10);
        s.undo().unwrap();
        assert_eq!(*s.doc().unwrap(), before);
        for file in ["profile.dxf", "profile.dwg"] {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            let d = cadcraft_io::read(&bytes, file).unwrap();
            let stored = super::super::alignment_store::load(&d, cadcraft_doc::Handle::parse_hex(h).unwrap()).unwrap();
            assert!(stored.profiles.design.is_some());
            assert!(stored.profiles.ground.is_some());
        }
        let xml = s.execute("alignmentexport", &json!({"alignment":h})).unwrap();
        let mut other = Session::new();
        let imported = other.execute("alignmentimport", &json!({"xml":xml["xml"]})).unwrap();
        assert!(imported["refusals"].as_array().unwrap().is_empty());
        let h = imported["handles"][0].as_str().unwrap();
        other.execute("profileview", &json!({"alignment":h,"at":[0,0]})).unwrap();
    }
}
