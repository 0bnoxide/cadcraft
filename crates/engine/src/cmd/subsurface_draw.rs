//! STICKLOG and SUBSURFACEPROFILE: draw stick logs and subsurface profiles. Owner: Codex (see
//! `docs/agents/slice-6-subsurface.md`).

use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use cadcraft_doc::{Common, EntityKind, Hatch, HatchLoop, Line, LwPolyline};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use cadcraft_subsurface::{
    BoringSet, Finding, Severity, profile,
    sticklog::{self, Log, Mark},
};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sticklog", "Boring Stick Logs", run_logs).params("{path,at?:[x,y],scale?:1,borings?:[id]}; input units retained"),
        CommandSpec::new("subsurfaceprofile", "Subsurface Profile", run_profile)
            .params("{path,start:[easting,northing],end:[easting,northing] OR boring_a:id,boring_b:id,at?:[x,y],tolerance?:50,exaggeration?:1}"),
    ]
}

pub(crate) fn load(p: &Value, cmd: &str) -> Result<(BoringSet, Vec<Finding>)> {
    let path = str_param(p, "path").ok_or_else(|| bad(cmd, "path required"))?;
    let (set, findings) = super::subsurface_import::load(path)?;
    if findings.iter().any(|f| f.severity == Severity::Error) {
        return Err(bad(cmd, super::subsurface_import::findings_json(&findings).to_string()));
    }
    Ok((set, findings))
}

pub(crate) fn point(p: &Value, key: &str, default: Option<[f64; 2]>, cmd: &str) -> Result<[f64; 2]> {
    let Some(v) = p.get(key) else {
        return default.ok_or_else(|| bad(cmd, format!("{key} required")));
    };
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(cmd, "expected [x,y]"))?;
    let x = a.first().and_then(Value::as_f64).filter(|v| v.is_finite());
    let y = a.get(1).and_then(Value::as_f64).filter(|v| v.is_finite());
    match (x, y) {
        (Some(x), Some(y)) => Ok([x, y]),
        _ => Err(bad(cmd, "finite coordinates required")),
    }
}

fn draw(s: &mut Session, log: Log) -> Result<Value> {
    let mut entities = Vec::new();
    let v3 = |p: [f64; 2]| Vec3 { x: p[0], y: p[1], z: 0.0 };
    for mark in log.marks {
        match mark {
            Mark::Text { layer, at, text } => entities.push((
                layer,
                EntityKind::MText(super::labels_callout::text(
                    super::labels_callout::literal(&text),
                    cadcraft_labels::Pt { x: at[0], y: at[1] },
                    0.25,
                )),
            )),
            Mark::Line { layer, a, b } => entities.push((layer, EntityKind::Line(Line { a: v3(a), b: v3(b) }))),
            Mark::Box { layer, at, size, hatch } => {
                let vertices: Vec<_> = [[at[0], at[1]], [at[0] + size[0], at[1]], [at[0] + size[0], at[1] + size[1]], [at[0], at[1] + size[1]]]
                    .into_iter()
                    .map(|p| PolyVertex::new(Vec2::new(p[0], p[1])))
                    .collect();
                if vertices.iter().any(|v| !v.p.x.is_finite() || !v.p.y.is_finite()) {
                    return Err(bad("sticklog", "coordinate overflow"));
                }
                if let Some(pattern) = hatch {
                    entities.push((
                        "BORING-HATCH",
                        EntityKind::Hatch(Hatch {
                            pattern: pattern.into(),
                            solid: pattern == "SOLID",
                            loops: vec![HatchLoop { vertices: vertices.clone(), outer: true, sources: Vec::new() }],
                            scale: 1.0,
                            angle: 0.0,
                            associative: false,
                            style: 0,
                            elevation: 0.0,
                            gradient: None,
                            origin: Vec2::ZERO,
                            background: None,
                            pattern_lines: Vec::new(),
                        }),
                    ));
                }
                entities
                    .push((layer, EntityKind::LwPolyline(LwPolyline { vertices, closed: true, const_width: 0.0, elevation: 0.0, plinegen: false })));
            }
        }
    }
    let mut handles = Vec::new();
    let mut drawing = s.doc()?.clone();
    let space = s.space();
    for (layer, kind) in entities {
        drawing.ensure_layer(layer);
        let color = if layer == "BORING-HATCH" {
            cadcraft_color::Color::True(cadcraft_color::Rgb::from_u32(0x9b8966))
        } else {
            cadcraft_color::Color::ByLayer
        };
        let h = drawing.add(&space, Common { layer: layer.into(), color, ..Common::default() }, kind)?;
        handles.push(h.hex());
    }
    *s.doc_mut()? = drawing;
    Ok(json!({"handles":handles,"warnings":log.warnings,"depth":log.depth}))
}

fn run_logs(s: &mut Session, p: &Value) -> Result<Value> {
    let (set, findings) = load(p, "sticklog")?;
    if set.borings.len() > 1000 {
        return Err(bad("sticklog", "more than 1000 borings"));
    }
    let at = point(p, "at", Some([0.0, 0.0]), "sticklog")?;
    let scale = p.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
    let selected: Option<Vec<String>> =
        p.get("borings").map(|v| serde_json::from_value(v.clone()).map_err(|e| bad("sticklog", e.to_string()))).transpose()?;
    if let Some(ids) = &selected {
        for id in ids {
            if !set.borings.iter().any(|b| b.boring_id == *id) {
                return Err(bad("sticklog", format!("unknown boring {id}")));
            }
        }
    }
    let mut output = Log::default();
    let mut x = at[0];
    for boring in &set.borings {
        if selected.as_ref().is_some_and(|ids| !ids.contains(&boring.boring_id)) {
            continue;
        }
        let log = sticklog::draw(&set, &boring.boring_id, [x, at[1]], scale).map_err(|e| bad("sticklog", e))?;
        if output.marks.len().saturating_add(log.marks.len()) > sticklog::MAX_ITEMS {
            return Err(bad("sticklog", "too many marks"));
        }
        output.marks.extend(log.marks);
        output.warnings.extend(log.warnings);
        output.depth = output.depth.max(log.depth);
        x += 12.0;
    }
    if output.marks.is_empty() {
        return Err(bad("sticklog", "no stick logs to draw"));
    }
    let mut result = draw(s, output)?;
    result["findings"] = super::subsurface_import::findings_json(&findings);
    Ok(result)
}

fn run_profile(s: &mut Session, p: &Value) -> Result<Value> {
    let (set, findings) = load(p, "subsurfaceprofile")?;
    let start = p.get("start").map(|_| point(p, "start", None, "subsurfaceprofile")).transpose()?;
    let end = p.get("end").map(|_| point(p, "end", None, "subsurfaceprofile")).transpose()?;
    let (a, b) = profile::endpoints(&set, start, end, str_param(p, "boring_a"), str_param(p, "boring_b")).map_err(|e| bad("subsurfaceprofile", e))?;
    let at = point(p, "at", Some([0.0, 0.0]), "subsurfaceprofile")?;
    let log = profile::draw(
        &set,
        a,
        b,
        p.get("tolerance").and_then(Value::as_f64).unwrap_or(50.0),
        at,
        p.get("exaggeration").and_then(Value::as_f64).unwrap_or(1.0),
    )
    .map_err(|e| bad("subsurfaceprofile", e))?;
    if log.marks.is_empty() {
        return Err(bad("subsurfaceprofile", "no drawable borings"));
    }
    let mut result = draw(s, log)?;
    result["findings"] = super::subsurface_import::findings_json(&findings);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_commands_draw_entities_and_workbook_round_trips() {
        let dir = std::env::temp_dir().join(format!("cadcraft-slice6-flow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("boring_locations.csv"),
            "BoringID,Easting,Northing,GroundElevation_ft,TotalDepth_ft\nB-1,0,0,100,20\nB-2,10,0,95,15\n",
        )
        .unwrap();
        std::fs::write(dir.join("lithology.csv"),"BoringID,TopDepth_ft,BottomDepth_ft,USCS,PID_ppm,Description\nB-1,0,5,CL,3.2,stiff clay\nB-1,5,20,SM,0,silty sand\nB-2,0,15,GW,,gravel\n").unwrap();
        std::fs::write(dir.join("samples.csv"), "BoringID,SampleID,TopDepth_ft,BottomDepth_ft\nB-1,S-1,2,4\n").unwrap();
        let mut s = Session::new();
        let path = dir.to_string_lossy();
        let imported = s.execute("boringimport", &json!({"path":path})).unwrap();
        assert_eq!(imported["borings"], 2);
        let logs = s.execute("sticklog", &json!({"path":path,"at":[100,100]})).unwrap();
        assert!(!logs["handles"].as_array().unwrap().is_empty());
        assert!(s.doc().unwrap().model.iter().any(|e| matches!(&e.kind,EntityKind::Hatch(h) if h.pattern=="ANSI31")));
        let profile = s.execute("subsurfaceprofile", &json!({"path":path,"start":[0,0],"end":[10,0],"tolerance":1,"exaggeration":2})).unwrap();
        assert!(!profile["handles"].as_array().unwrap().is_empty());
        let report = s.execute("boringreport", &json!({"path":path})).unwrap();
        assert_eq!(report["tables"], 11);
        let output = dir.join("report.xlsx");
        s.execute("boringreport", &json!({"path":path,"output":output.to_string_lossy()})).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let tables = cadcraft_subsurface::sheet::read(&bytes, "report.xlsx").unwrap();
        assert_eq!(tables.len(), 11);
        assert!(tables.iter().any(|t| t.name == "Sample summary" && t.cell(1, 8).number() == Some(3.2)));
        let before = s.doc().unwrap().model.len();
        let query = json!({"version":1,"layers":["BORING-LOC"],"types":["Point"],"columns":[{"property":"handle","title":"Handle"},{"property":"z","title":"Ground","decimals":1}]});
        let extraction = s.execute("datawizard", &json!({"settings":query})).unwrap();
        let h = extraction["handles"][0].as_str().unwrap();
        s.execute("datawizard", &json!({"refresh":h})).unwrap();
        assert_eq!(s.doc().unwrap().model.len(), before + 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_package_does_not_draw_partial_logs() {
        let mut s = Session::new();
        let before = s.doc().unwrap().model.len();
        assert!(s.execute("sticklog", &json!({"path":"no/such/package"})).is_err());
        assert_eq!(s.doc().unwrap().model.len(), before);
    }
}
