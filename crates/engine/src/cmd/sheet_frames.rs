//! Sheet Generator's drawing stages; geometry remains in cadcraft-alignment.
use super::{
    CommandSpec,
    alignment_cmd::{scalar, selected},
    alignment_store, bad, profile_cmd,
    sheet_store::{self as store, Group, Matchline, Member, ProfileFigure, ProfileSettings},
};
use crate::{Result, Session};
use cadcraft_alignment::{
    frames::{self, Frame, MAX_FRAMES, Options},
    view::{self, Adjustment, Center, Profile},
};
use cadcraft_doc::{Common, Drawing, EntityKind, Handle, Line, LwPolyline, Space};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde::Deserialize;
use serde_json::{Value, json};
const CMD: &str = "sheets";
pub fn specs() -> Vec<CommandSpec> {
    vec![
    CommandSpec::new("viewframecreate","Create Road View Frames",road).params("{alignment,paper_size:[width,height],scale,range?:[start,end],spacing?:distance|automatic,increment?:1,vertex_buffer?:0,offset?:0,matchline?:{block,attribute}} -> {group,frames}"),
    CommandSpec::new("siteviewframes","Group Site View Frames",site).params("{handles:[insert handles],scale} -> {group,frames}; positive uniform inserts with one rectangular block outline"),
    CommandSpec::new("profileviewframes","Create Frame Profile Views",profiles).params("{group,at:[x,y],height,profile:ground|design,center:highest|median|lowest,datum_increment,start_buffer?:0,end_buffer?:0,exaggeration?:1} -> {views,clipped}; replaces owned views"),
]
}
fn positive(p: &Value, key: &str) -> Result<f64> {
    let n = scalar(p, key, f64::NAN, CMD)?;
    if !n.is_finite() || n <= 0. {
        return Err(bad(CMD, format!("INVALID_FRAME_SETTINGS: {key} must be positive and finite")));
    }
    Ok(n)
}
fn text(d: &mut Drawing, at: [f64; 2], value: &str, height: f64, layer: &str) -> Result<Handle> {
    d.ensure_layer(layer);
    Ok(d.add(
        &Space::Model,
        Common { layer: layer.into(), ..Default::default() },
        EntityKind::MText(super::labels_callout::text(super::labels_callout::literal(value), cadcraft_labels::Pt { x: at[0], y: at[1] }, height)),
    )?)
}
fn anchor(d: &mut Drawing, at: [f64; 2]) -> Result<Handle> {
    let h = text(d, at, "Sheet group", 2.5, "CADCRAFT-SHEET-GROUPS")?;
    d.modify_entity(h, |e| {
        if let EntityKind::MText(t) = &mut e.kind {
            t.contents = format!("Sheet group {}", h.hex());
        }
    })?;
    Ok(h)
}
fn poly(frame: &Frame) -> EntityKind {
    EntityKind::LwPolyline(LwPolyline {
        vertices: frame.corners().into_iter().map(|p| PolyVertex::new(Vec2::new(p[0], p[1]))).collect(),
        closed: true,
        const_width: 0.,
        elevation: 0.,
        plinegen: false,
    })
}
fn matchline(p: &Value, d: &Drawing) -> Result<Option<Matchline>> {
    let m =
        p.get("matchline").map(|v| Matchline::deserialize(v).map_err(|_| bad(CMD, "matchline requires block and attribute names"))).transpose()?;
    if let Some(m) = &m {
        validate_matchline(d, m)?;
    }
    Ok(m)
}
fn validate_matchline(d: &Drawing, m: &Matchline) -> Result<()> {
    let b = d.block(&m.block).ok_or_else(|| bad(CMD, "MISSING_BLOCK: matchline block missing"))?;
    if b.entities.len() > 4096
        || !b.entities.iter().any(|e| matches!(&e.kind,EntityKind::AttDef(a) if a.tag.eq_ignore_ascii_case(&m.attribute) && !a.constant))
    {
        return Err(bad(CMD, "MISSING_ATTRIBUTE: nonconstant station attribute definition required"));
    }
    Ok(())
}
fn road(s: &mut Session, p: &Value) -> Result<Value> {
    let h = selected(s, p, "alignment", CMD)?;
    let a = alignment_store::load(s.doc()?, h)?;
    let size = profile_cmd::pair(p, "paper_size")?.ok_or_else(|| bad(CMD, "paper_size required"))?;
    let range = profile_cmd::pair(p, "range")?
        .unwrap_or([a.definition.start_station, a.definition.start_station + a.definition.length().map_err(|e| bad(CMD, e.to_string()))?]);
    let spacing = match p.get("spacing") {
        None | Some(Value::String(_)) if p.get("spacing").is_none() || p.get("spacing").and_then(Value::as_str) == Some("automatic") => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| bad(CMD, "spacing must be automatic or a positive number"))?),
        None => None,
    };
    let scale = positive(p, "scale")?;
    let f = frames::tile(
        &a.definition,
        &Options {
            start: range[0],
            end: range[1],
            paper_size: size,
            scale,
            spacing,
            increment: scalar(p, "increment", 1., CMD)?,
            vertex_buffer: scalar(p, "vertex_buffer", 0., CMD)?,
            offset: scalar(p, "offset", 0., CMD)?,
        },
    )
    .map_err(|e| bad(CMD, e.to_string()))?;
    let m = matchline(p, s.doc()?)?;
    let mut d = s.doc()?.clone();
    let group = anchor(&mut d, f.first().ok_or_else(|| bad(CMD, "NO_FRAMES"))?.corners()[0])?;
    let mut g =
        Group { version: 1, own: group.hex(), alignment: Some(h.hex()), paper_size: size, scale, frames: Vec::new(), matchline: m, profiles: None };
    d.ensure_layer("CADCRAFT-VIEW-FRAMES");
    for (i, frame) in f.into_iter().enumerate() {
        let handle = d.add(&Space::Model, Common { layer: "CADCRAFT-VIEW-FRAMES".into(), ..Default::default() }, poly(&frame))?;
        store::own(&mut d, handle, &g.own, i as u32, "frame")?;
        g.frames.push(Member { index: i as u32, handle: handle.hex(), geometry: frame, matchlines: Vec::new(), profile: None });
    }
    redraw_matchlines(&mut d, &mut g, &a)?;
    store::save(&mut d, &g)?;
    *s.doc_mut()? = d;
    Ok(json!({"group":g.own,"frames":g.frames.iter().map(|f|json!({"handle":f.handle,"geometry":f.geometry})).collect::<Vec<_>>()}))
}
pub(super) fn redraw_matchlines(d: &mut Drawing, g: &mut Group, a: &alignment_store::Stored) -> Result<()> {
    if let Some(m) = &g.matchline {
        validate_matchline(d, m)?;
    }
    let count = g.frames.len();
    let mut previous_end: Option<f64> = None;
    for (position, f) in g.frames.iter_mut().enumerate() {
        store::erase(d, &f.matchlines, &g.own, f.index, "matchline")?;
        f.matchlines.clear();
        // A shared boundary belongs to the preceding frame; outer ends are not matchlines.
        let start =
            previous_end.is_some_and(|end| (f.geometry.range[0] - end).abs() > cadcraft_alignment::POSITION_TOLERANCE).then_some(f.geometry.range[0]);
        let end = (position + 1 < count).then_some(f.geometry.range[1]);
        previous_end = Some(f.geometry.range[1]);
        for station in [start, end].into_iter().flatten() {
            let pose = a.definition.at(station, 0.).map_err(|e| bad(CMD, e.to_string()))?;
            let at = Vec2::new(pose.point[0], pose.point[1]);
            let label = a.definition.format.display(station).map_err(|e| bad(CMD, e.to_string()))?;
            let handles = if let Some(m) = &g.matchline {
                let mut stage = Session::new();
                *stage.doc_mut()? = d.clone();
                let values = serde_json::Map::from_iter([(m.attribute.clone(), json!(label))]);
                let h = super::blocks::insert(&mut stage, &m.block, at, 1., pose.direction, &values)?;
                *d = stage.doc()?.clone();
                vec![h]
            } else {
                d.ensure_layer("CADCRAFT-MATCHLINES");
                let normal = Vec2::new(-pose.direction.sin(), pose.direction.cos()) * (f.geometry.size[1] / 2.);
                let h = d.add(
                    &Space::Model,
                    Common { layer: "CADCRAFT-MATCHLINES".into(), ..Default::default() },
                    EntityKind::Line(Line { a: (at - normal).to3(0.), b: (at + normal).to3(0.) }),
                )?;
                vec![h, text(d, pose.point, &label, (f.geometry.size[1] / 40.).max(0.25), "CADCRAFT-MATCHLINES")?]
            };
            for h in handles {
                store::own(d, h, &g.own, f.index, "matchline")?;
                f.matchlines.push(h.hex());
            }
        }
    }
    Ok(())
}
pub(super) fn rectangle(points: &[[f64; 2]; 4]) -> Result<([f64; 2], [f64; 2], f64)> {
    let [a, b, c, _] = points;
    let axis = Vec2::new(b[0] - a[0], b[1] - a[1]);
    let side = Vec2::new(c[0] - b[0], c[1] - b[1]);
    let size = [axis.len(), side.len()];
    let angle = axis.y.atan2(axis.x);
    let center = [a[0] + (c[0] - a[0]) / 2., a[1] + (c[1] - a[1]) / 2.];
    let test = Frame { range: [0., 1.], center, size, angle, half_chord: 1., offset: 0. };
    test.validate().map_err(|e| bad(CMD, e.to_string()))?;
    let close = |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= cadcraft_alignment::POSITION_TOLERANCE;
    let corners = test.corners();
    if !corners.iter().zip(points).all(|(a, b)| close(a, b)) && !corners.iter().zip(points.iter().rev()).all(|(a, b)| close(a, b)) {
        return Err(bad(CMD, "INVALID_FRAME: ordered rectangular corners required"));
    }
    Ok((center, size, angle))
}
fn points(poly: &LwPolyline) -> Result<[[f64; 2]; 4]> {
    if !poly.closed
        || poly.vertices.len() != 4
        || poly.elevation != 0.
        || poly.const_width != 0.
        || poly.vertices.iter().any(|p| p.bulge != 0. || p.start_width != 0. || p.end_width != 0.)
    {
        return Err(bad(CMD, "INVALID_FRAME: closed planar four-corner rectangle required"));
    }
    let v = poly.vertices.iter().map(|p| [p.p.x, p.p.y]).collect::<Vec<_>>();
    v.try_into().map_err(|_| bad(CMD, "INVALID_FRAME: four corners required"))
}
pub(super) fn site_geometry(d: &Drawing, h: Handle) -> Result<Frame> {
    let e = d.model.get(h).ok_or_else(|| bad(CMD, "MISSING_FRAME: site insert missing"))?;
    let EntityKind::Insert(i) = &e.kind else {
        return Err(bad(CMD, "INVALID_FRAME: site frame must be a block insert"));
    };
    if e.common.extrusion != Vec3::Z
        || !i.rotation.is_finite()
        || !i.scale.x.is_finite()
        || i.scale.x <= 0.
        || i.scale.x != i.scale.y
        || i.rows != 1
        || i.cols != 1
        || i.clip.is_some()
        || !i.insert.is_finite()
    {
        return Err(bad(CMD, "INVALID_FRAME: finite uniform positive XY insert without clipping or arrays required"));
    }
    let b = d.block(&i.block).ok_or_else(|| bad(CMD, "MISSING_BLOCK: site block missing"))?;
    if b.xref_path.is_some() || b.entities.len() > 4096 {
        return Err(bad(CMD, "INVALID_FRAME: site block is external or oversized"));
    }
    let mut rectangles = Vec::new();
    for e in b.entities.iter() {
        if let EntityKind::LwPolyline(p) = &e.kind {
            let corners = points(p)?;
            rectangle(&corners)?;
            rectangles.push(corners);
        }
    }
    let [corners] = rectangles.as_slice() else {
        return Err(bad(CMD, "INVALID_FRAME: site block needs exactly one rectangle"));
    };
    let transform = i.transform(b.base.xy());
    let transformed = corners.map(|p| {
        let v = transform.apply(Vec2::new(p[0], p[1]));
        [v.x, v.y]
    });
    let (center, size, angle) = rectangle(&transformed)?;
    Ok(Frame { range: [0., 1.], center, size, angle, half_chord: size[0] / 2., offset: 0. })
}
fn site(s: &mut Session, p: &Value) -> Result<Value> {
    let handles = p
        .get("handles")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= MAX_FRAMES)
        .ok_or_else(|| bad(CMD, "handles requires 1..4096 inserts"))?;
    let scale = positive(p, "scale")?;
    let mut members = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for v in handles {
        let h = super::curves::handle_value(v).ok_or_else(|| bad(CMD, "invalid insert handle"))?;
        if !seen.insert(h) || alignment_store::record_handle_for(s.doc()?, h, "CADCRAFT_SHEET_OWNER")?.is_some() {
            return Err(bad(CMD, "FRAME_OWNED: duplicate or already owned site insert"));
        }
        members.push((h, site_geometry(s.doc()?, h)?));
    }
    let first = members.first().ok_or_else(|| bad(CMD, "NO_FRAMES"))?.1.clone();
    if members.iter().any(|(_, f)| f.size.iter().zip(first.size).any(|(a, b)| (a - b).abs() > cadcraft_alignment::POSITION_TOLERANCE)) {
        return Err(bad(CMD, "FRAME_SIZE_MISMATCH: site rectangles need equal size"));
    }
    let mut d = s.doc()?.clone();
    let group = anchor(&mut d, first.corners()[0])?;
    let mut g = Group {
        version: 1,
        own: group.hex(),
        alignment: None,
        paper_size: first.size.map(|v| v / scale),
        scale,
        frames: Vec::new(),
        matchline: None,
        profiles: None,
    };
    for (index, (h, geometry)) in members.into_iter().enumerate() {
        store::own(&mut d, h, &g.own, index as u32, "frame")?;
        g.frames.push(Member { index: index as u32, handle: h.hex(), geometry, matchlines: Vec::new(), profile: None });
    }
    store::save(&mut d, &g)?;
    *s.doc_mut()? = d;
    Ok(json!({"group":g.own,"frames":g.frames.iter().map(|f|&f.handle).collect::<Vec<_>>()}))
}
fn profiles(s: &mut Session, p: &Value) -> Result<Value> {
    let h = selected(s, p, "group", CMD)?;
    let mut g = store::load(s.doc()?, h)?;
    let alignment = g.alignment.as_deref().ok_or_else(|| bad(CMD, "SITE_GROUP: site groups have no profile views"))?;
    let a = alignment_store::load(s.doc()?, store::handle(alignment)?)?;
    let at = profile_cmd::pair(p, "at")?.ok_or_else(|| bad(CMD, "at required"))?;
    let profile = Profile::deserialize(p.get("profile").ok_or_else(|| bad(CMD, "profile required"))?)
        .map_err(|_| bad(CMD, "profile must be ground or design"))?;
    let center = Center::deserialize(p.get("center").ok_or_else(|| bad(CMD, "center required"))?)
        .map_err(|_| bad(CMD, "center must be highest, median or lowest"))?;
    let settings = ProfileSettings {
        at,
        exaggeration: scalar(p, "exaggeration", 1., CMD)?,
        adjustment: Adjustment {
            height: positive(p, "height")?,
            profile,
            center,
            increment: positive(p, "datum_increment")?,
            start_buffer: scalar(p, "start_buffer", 0., CMD)?,
            end_buffer: scalar(p, "end_buffer", 0., CMD)?,
        },
    };
    let mut d = s.doc()?.clone();
    refresh_frames(&d, &mut g, &a)?;
    let report = redraw_profiles(&mut d, &mut g, &a, &settings)?;
    g.profiles = Some(settings);
    store::save(&mut d, &g)?;
    *s.doc_mut()? = d;
    Ok(report)
}
pub(super) fn refresh_frames(d: &Drawing, g: &mut Group, a: &alignment_store::Stored) -> Result<()> {
    for f in &mut g.frames {
        let h = store::handle(&f.handle)?;
        store::check(d, h, &g.own, f.index, "frame")?;
        let e = d.model.get(h).ok_or_else(|| bad(CMD, "MISSING_FRAME: frame was erased"))?;
        let EntityKind::LwPolyline(p) = &e.kind else {
            return Err(bad(CMD, "INVALID_FRAME: road rectangle is not a polyline"));
        };
        if e.common.extrusion != Vec3::Z {
            return Err(bad(CMD, "INVALID_FRAME: nonplanar rectangle"));
        }
        let corners = points(p)?;
        let (center, size, angle) = rectangle(&corners)?;
        if size.iter().zip(f.geometry.size).any(|(a, b)| (a - b).abs() > cadcraft_alignment::POSITION_TOLERANCE) {
            return Err(bad(CMD, "FRAME_STRETCHED: size differs from group viewport"));
        }
        if corners.iter().zip(f.geometry.corners()).all(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) <= cadcraft_alignment::POSITION_TOLERANCE) {
            continue;
        }
        f.geometry.center = center;
        f.geometry.angle = angle;
        let origin = [center[0] + f.geometry.offset * angle.sin(), center[1] - f.geometry.offset * angle.cos()];
        let axis = [angle.cos() * f.geometry.half_chord, angle.sin() * f.geometry.half_chord];
        let view = a.definition.validated().map_err(|e| bad(CMD, e.to_string()))?;
        let start = view.project([origin[0] - axis[0], origin[1] - axis[1]]).map_err(|e| bad(CMD, e.to_string()))?;
        let end = view.project([origin[0] + axis[0], origin[1] + axis[1]]).map_err(|e| bad(CMD, e.to_string()))?;
        if start.beyond.is_some() || end.beyond.is_some() {
            return Err(bad(CMD, "FRAME_RANGE: moved chord exceeds alignment"));
        }
        f.geometry.range = [start.station, end.station];
        frames::contains(&a.definition, &f.geometry).map_err(|e| bad(CMD, e.to_string()))?;
    }
    Ok(())
}
pub(super) fn redraw_profiles(d: &mut Drawing, g: &mut Group, a: &alignment_store::Stored, settings: &ProfileSettings) -> Result<Value> {
    let mut at = settings.at;
    let mut views = Vec::new();
    let mut clipped = Vec::new();
    let mut total = 0usize;
    for f in &mut g.frames {
        if let Some(p) = &f.profile {
            store::erase(d, &p.handles, &g.own, f.index, "profile")?;
        }
        let window = view::adjust(&a.definition, &a.profiles, f.geometry.range, &settings.adjustment).map_err(|e| bad(CMD, e.to_string()))?;
        let figure = view::draw_range(&a.definition, &a.profiles, at, settings.exaggeration, window.range, Some(window.elevation))
            .map_err(|e| bad(CMD, e.to_string()))?;
        let text_height = ((window.range[1] - window.range[0]) / 100.).max(0.25);
        total = total.saturating_add(figure.marks.len());
        if total > store::MAX_MEMBERS {
            return Err(bad(CMD, "PROFILE_LIMIT: too many figure entities"));
        }
        let handles = profile_cmd::add_marks(d, &Space::Model, figure.marks, text_height)?;
        for h in &handles {
            store::own(d, *h, &g.own, f.index, "profile")?;
        }
        clipped.push(json!({"frame":f.index,"spans":figure.clipped}));
        let p = ProfileFigure { handles: handles.iter().map(|h| h.hex()).collect(), at, window, exaggeration: settings.exaggeration };
        views.push(json!({"frame":f.index,"at":at,"window":p.window,"handles":p.handles}));
        at[1] -= settings.adjustment.height * settings.exaggeration + 8. * text_height;
        f.profile = Some(p);
    }
    Ok(json!({"group":g.own,"views":views,"clipped":clipped}))
}
#[cfg(test)]
mod rectangle_tests {
    #[test]
    fn clockwise_site_outline_keeps_the_first_edge_axis() {
        let (center, size, angle) = super::rectangle(&[[0., 0.], [0., 10.], [30., 10.], [30., 0.]]).unwrap();
        assert_eq!(center, [15., 5.]);
        assert_eq!(size, [10., 30.]);
        assert_eq!(angle, std::f64::consts::FRAC_PI_2);
        assert!(super::rectangle(&[[0., 0.], [0., 10.], [30., 0.], [30., 10.]]).is_err());
    }
}
#[cfg(test)]
mod matchline_tests {
    use super::{alignment_store, redraw_matchlines, refresh_frames, store};
    use crate::Session;
    use cadcraft_doc::{Drawing, EntityKind};
    use serde_json::json;
    fn stations(d: &Drawing) -> Vec<f64> {
        let mut out = d
            .model
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Line(l) if e.common.layer == "CADCRAFT-MATCHLINES" => Some((l.a.x + l.b.x) / 2.),
                _ => None,
            })
            .collect::<Vec<_>>();
        out.sort_by(f64::total_cmp);
        out
    }
    #[test]
    fn touching_boundaries_share_one_matchline_and_moved_neighbours_keep_both() {
        let mut s = Session::new();
        let a = s.execute("alignmentcreate", &json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap();
        let created = s.execute("viewframecreate", &json!({"alignment":a["handle"],"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
        let h = store::handle(created["group"].as_str().unwrap()).unwrap();
        let mut g = store::load(s.doc().unwrap(), h).unwrap();
        let a = alignment_store::load(s.doc().unwrap(), store::handle(a["handle"].as_str().unwrap()).unwrap()).unwrap();
        // Half-tolerance station noise at each side of the middle frame still shares both boundaries.
        for station in &mut g.frames[1].geometry.range {
            *station += cadcraft_alignment::POSITION_TOLERANCE / 2.;
        }
        redraw_matchlines(s.doc_mut().unwrap(), &mut g, &a).unwrap();
        let found = stations(s.doc().unwrap());
        assert_eq!(found.len(), 3);
        for (actual, expected) in found.into_iter().zip([250., 500., 750.]) {
            assert!((actual - expected).abs() <= cadcraft_alignment::POSITION_TOLERANCE);
        }
        store::save(s.doc_mut().unwrap(), &g).unwrap();
        s.execute("move", &json!({"handles":[created["frames"][1]["handle"]],"delta":[10,0]})).unwrap();
        refresh_frames(s.doc().unwrap(), &mut g, &a).unwrap();
        redraw_matchlines(s.doc_mut().unwrap(), &mut g, &a).unwrap();
        // Middle range 250..500 moves to 260..510: gap 250/260, overlap 510/500, shared 750.
        assert_eq!(stations(s.doc().unwrap()), [250., 260., 500., 510., 750.]);
    }
}
