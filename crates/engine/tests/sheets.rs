use cadcraft_engine::{
    Session,
    doc::{Block, Common, Drawing, Entity, EntityKind, Insert, Layout, Line, Space, Viewport},
    geom::{Vec2, Vec3},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("cadcraft-sheet-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn hooks() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
}
fn template(temp: &Temp, roles: &[&str]) -> PathBuf {
    let mut d = Drawing::new_metric();
    d.layouts = vec![Layout::new("Template", 1)];
    let mut block = Block::new("North");
    let handle = d.new_handle();
    block.entities.push(Entity { handle, common: Common::default(), kind: EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(0., 5., 0.) }) });
    d.blocks.insert("North".into(), Arc::new(block));
    for (i, role) in roles.iter().enumerate() {
        d.add(
            &Space::Paper("Template".into()),
            Common { layer: (*role).into(), ..Default::default() },
            EntityKind::Viewport(Viewport {
                id: i as u32 + 2,
                center: Vec3::new(160., 60. + 120. * i as f64, 0.),
                width: 300.,
                height: 100.,
                ..Default::default()
            }),
        )
        .unwrap();
        if role.contains("PLAN") {
            let suffix = role.rsplit('-').next().unwrap();
            d.add(
                &Space::Paper("Template".into()),
                Common { layer: format!("SHEET-NORTH-{suffix}"), ..Default::default() },
                EntityKind::Insert(Insert { block: "North".into(), insert: Vec3::new(20., 80., 0.), ..Default::default() }),
            )
            .unwrap();
        }
    }
    let text = serde_json::from_value(
        json!({"insert":[10,10,0],"height":2,"value":"{{SheetNumber}}/{{SheetCount}} {{StationStart}} {{StationEnd}} {{Alignment}} {{Unknown}}"}),
    )
    .unwrap();
    d.add(&Space::Paper("Template".into()), Common::default(), EntityKind::Text(text)).unwrap();
    let p = temp.0.join("template.dxf");
    std::fs::write(&p, cadcraft_io::write(&d, &p).unwrap()).unwrap();
    p
}
fn group(s: &mut Session) -> Value {
    let h=s.execute("alignmentcreate",&json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap()["handle"].clone();
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    let g = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
    s.execute(
        "profileviewframes",
        &json!({"group":g["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5}),
    )
    .unwrap();
    g
}
fn request(g: &Value, t: &std::path::Path, arrangement: &str) -> Value {
    json!({"group":g["group"],"template":t,"template_layout":"Template","arrangement":arrangement,"layout_name":"Road-{n}","number_start":10,"number_increment":2,"number_width":3,"north_arrow":"North","storage":"current"})
}
#[test]
fn current_sheets_copy_explicit_roles_replace_text_and_undo_together() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let before = s.doc().unwrap().clone();
    let result = s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap();
    assert_eq!(result["layouts"].as_array().unwrap().len(), 4);
    let l = s.doc().unwrap().layout("Road-010").unwrap();
    let plan = l
        .entities
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Viewport(v) if e.common.layer == "SHEET-PLAN-1" => Some(v),
            _ => None,
        })
        .unwrap();
    assert_eq!(plan.view_center, Vec2::new(125., 0.));
    assert_eq!(plan.view_height, 100.);
    assert_eq!(plan.twist, 0.);
    assert!(
        l.entities.iter().any(
            |e| matches!(&e.kind,EntityKind::Text(t) if t.value.contains("010/4") && t.value.contains("Road") && t.value.contains("{{Unknown}}"))
        )
    );
    assert!(!result["warnings"].as_array().unwrap().is_empty());
    let after = s.doc().unwrap().clone();
    let update = s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert_eq!(update["changed"], false);
    assert_eq!(*s.doc().unwrap(), after, "unmoved update is idempotent");
    s.undo().unwrap();
    assert_eq!(*s.doc().unwrap(), before);
}
#[test]
fn moved_frame_update_and_deletion_leave_an_unowned_layout_alone() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let result = s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    assert_eq!(result["layouts"].as_array().unwrap().len(), 4);
    let unowned = s.doc().unwrap().layouts.first().unwrap().clone();
    s.execute("move", &json!({"handles":[g["frames"][0]["handle"]],"delta":[10,5]})).unwrap();
    let update = s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert_eq!(update["changed"], true);
    let l = s.doc().unwrap().layout("Road-010").unwrap();
    assert!(
        l.entities.iter().any(|e| matches!(&e.kind,EntityKind::Viewport(v) if e.common.layer=="SHEET-PLAN-1" && v.view_center==Vec2::new(135.,5.)))
    );
    assert!(!update["gaps_overlaps"].as_array().unwrap().is_empty());
    let again = s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(again["gaps_overlaps"], update["gaps_overlaps"], "a no-op update still reports existing gaps and overlaps");
    s.execute("erase", &json!({"handles":[g["frames"][3]["handle"]]})).unwrap();
    s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert!(s.doc().unwrap().layout("Road-016").is_none());
    assert_eq!(s.doc().unwrap().layout(&unowned.name).unwrap(), &unowned);
}
#[test]
fn odd_two_up_removes_unused_viewport_and_arrow_and_collisions_refuse() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PLAN-2"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("erase", &json!({"handles":[g["frames"][3]["handle"]]})).unwrap();
    let r = s.execute("sheetcreate", &request(&g, &t, "plan_plan")).unwrap();
    assert_eq!(r["layouts"].as_array().unwrap().len(), 2);
    let l = s.doc().unwrap().layout("Road-012").unwrap();
    assert!(!l.entities.iter().any(|e| e.common.layer == "SHEET-PLAN-2" || e.common.layer == "SHEET-NORTH-2"));
    assert_eq!(r["unused_slots"].as_array().unwrap().len(), 1);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_plan")).is_err());
    assert_eq!(*s.doc().unwrap(), before);
}
#[test]
fn hostile_or_mismatched_template_refuses_without_changes() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap_err().to_string().contains("TEMPLATE"));
    assert_eq!(*s.doc().unwrap(), before);
    std::fs::write(&t, b"hostile non-drawing input").unwrap();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap_err().to_string().contains("TEMPLATE"));
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn explicitly_added_unowned_frame_gets_a_new_sheet_and_owned_copies_are_not_adopted() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    let copied = s.execute("copy", &json!({"handles":[g["frames"][0]["handle"]],"delta":[100,20]})).unwrap()["handles"][0].clone();
    let result = s.execute("sheetupdate", &json!({"group":g["group"],"add_frames":[copied]})).unwrap();
    assert_eq!(result["layouts"].as_array().unwrap().len(), 5);
    assert!(s.doc().unwrap().layout("Road-018").is_some());
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetupdate", &json!({"group":g["group"],"add_frames":[g["frames"][0]["handle"]]})).is_err());
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn profile_only_and_profile_pair_arrangements_keep_horizontal_scale() {
    hooks();
    for (arrangement, roles, count) in
        [("profile_only", vec!["SHEET-PROFILE-1"], 4), ("profile_profile", vec!["SHEET-PROFILE-1", "SHEET-PROFILE-2"], 2)]
    {
        let temp = Temp::new();
        let t = template(&temp, &roles);
        let mut s = Session::new();
        let g = group(&mut s);
        let r = s.execute("sheetcreate", &request(&g, &t, arrangement)).unwrap();
        assert_eq!(r["layouts"].as_array().unwrap().len(), count);
        for name in r["layouts"].as_array().unwrap() {
            for e in s.doc().unwrap().layout(name.as_str().unwrap()).unwrap().entities.iter() {
                if let EntityKind::Viewport(v) = &e.kind
                    && v.id != 1
                {
                    assert_eq!(v.scale(), 1.);
                }
            }
        }
    }
}
#[test]
fn tall_profile_viewports_plot_only_their_own_figure() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut source = cadcraft_io::read(&std::fs::read(&t).unwrap(), &t).unwrap();
    let h = source.layout("Template").unwrap().entities.iter().find(|e| e.common.layer == "SHEET-PROFILE-1").unwrap().handle;
    source
        .modify_entity(h, |e| {
            if let EntityKind::Viewport(v) = &mut e.kind {
                v.height = 200.;
            }
        })
        .unwrap();
    std::fs::write(&t, cadcraft_io::write(&source, &t).unwrap()).unwrap();
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap();
    // The 200-unit viewport exceeds the 40-unit figure height and 60-unit stack pitch.
    // A neighbouring red design line would therefore bleed into the plotted viewport.
    for file in ["sheets.dxf", "sheets.dwg"] {
        let d = cadcraft_io::read(&cadcraft_io::write(s.doc().unwrap(), file).unwrap(), file).unwrap();
        for name in ["Road-010", "Road-012", "Road-014", "Road-016"] {
            let layout = d.layout(name).unwrap();
            for e in layout.entities.iter() {
                if let EntityKind::Viewport(v) = &e.kind
                    && v.id != 1
                {
                    let hidden = v.frozen_layers.iter().filter(|n| n.starts_with("CADCRAFT-SHEET-PROFILE-")).count();
                    assert_eq!(hidden, if e.common.layer.contains("PLAN") { 4 } else { 3 });
                }
            }
            let svg = cadcraft_io::plotter::plot(&d, &Space::Paper(name.into()), &json!({"output":"svg"})).unwrap();
            let svg = String::from_utf8(svg).unwrap();
            assert_eq!(svg.matches("stroke=\"#ff0000\"").count(), 1, "{file}/{name}: neighbouring figure leaked into plot");
        }
    }
}
#[test]
fn north_arrow_rotation_is_absolute_and_owned_sheets_reopen_in_both_formats() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    s.execute("rotate", &json!({"handles":[g["frames"][0]["handle"]],"base":[125,0],"angle":10})).unwrap();
    s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    for file in ["sheets.dxf", "sheets.dwg"] {
        let drawing = cadcraft_io::read(&cadcraft_io::write(s.doc().unwrap(), file).unwrap(), file).unwrap();
        let mut reopened = Session::new();
        *reopened.doc_mut().unwrap() = drawing;
        for _ in 0..2 {
            reopened.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
            let l = reopened.doc().unwrap().layout("Road-010").unwrap();
            let v = l
                .entities
                .iter()
                .find_map(|e| match &e.kind {
                    EntityKind::Viewport(v) if e.common.layer == "SHEET-PLAN-1" => Some(v),
                    _ => None,
                })
                .unwrap();
            let n = l
                .entities
                .iter()
                .find_map(|e| match &e.kind {
                    EntityKind::Insert(n) if n.block == "North" => Some(n),
                    _ => None,
                })
                .unwrap();
            assert!((v.twist + 10f64.to_radians()).abs() < 1e-9);
            assert!((n.rotation - v.twist).abs() < 1e-9);
        }
    }
}
#[test]
fn three_site_inserts_make_three_plan_sheets_without_an_alignment() {
    use cadcraft_engine::doc::LwPolyline;
    use cadcraft_engine::geom::PolyVertex;
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let d = s.doc_mut().unwrap();
    let mut b = Block::new("Site");
    let handle = d.new_handle();
    b.entities.push(Entity {
        handle,
        common: Common::default(),
        kind: EntityKind::LwPolyline(LwPolyline {
            vertices: [[-150., -50.], [150., -50.], [150., 50.], [-150., 50.]].into_iter().map(|[x, y]| PolyVertex::new(Vec2::new(x, y))).collect(),
            closed: true,
            elevation: 0.,
            const_width: 0.,
            plinegen: false,
        }),
    });
    d.blocks.insert("Site".into(), Arc::new(b));
    let mut handles = Vec::new();
    for x in [0., 400., 800.] {
        handles.push(
            d.add(
                &Space::Model,
                Common::default(),
                EntityKind::Insert(Insert { block: "Site".into(), insert: Vec3::new(x, 0., 0.), ..Default::default() }),
            )
            .unwrap()
            .hex(),
        );
    }
    let g = s.execute("siteviewframes", &json!({"handles":handles,"scale":1})).unwrap();
    let r = s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    assert_eq!(r["layouts"].as_array().unwrap().len(), 3);
    for (i, name) in r["layouts"].as_array().unwrap().iter().enumerate() {
        let l = s.doc().unwrap().layout(name.as_str().unwrap()).unwrap();
        assert!(l.entities.iter().any(|e| matches!(&e.kind,EntityKind::Viewport(v) if v.id!=1 && v.view_center==Vec2::new(i as f64*400.,0.))));
    }
}
#[test]
fn unowned_layout_names_and_user_objects_in_removed_sheets_are_protected() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let mut p = request(&g, &t, "plan_only");
    p["layout_name"] = json!(s.doc().unwrap().layouts.first().unwrap().name);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &p).unwrap_err().to_string().contains("LAYOUT_COLLISION"));
    assert_eq!(*s.doc().unwrap(), before);
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    s.doc_mut()
        .unwrap()
        .add(
            &Space::Paper("Road-016".into()),
            Common::default(),
            EntityKind::Point(cadcraft_engine::doc::Point { p: Vec3::new(1., 1., 0.), angle: 0. }),
        )
        .unwrap();
    s.execute("erase", &json!({"handles":[g["frames"][3]["handle"]]})).unwrap();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap_err().to_string().contains("SHEET_HAS_UNOWNED_OBJECTS"));
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn deleting_the_current_owned_sheet_returns_to_model_space() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    s.execute("erase", &json!({"handles":[g["frames"][3]["handle"]]})).unwrap();
    s.state_mut().unwrap().space = Space::Paper("Road-016".into());
    s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert_eq!(s.space(), Space::Model);
}
#[test]
fn sheet_updates_report_clipped_profile_spans() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("profileviewframes", &json!({"group":g["group"],"at":[2000,2000],"height":1,"profile":"design","center":"median","datum_increment":1}))
        .unwrap();
    s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap();
    s.execute("move", &json!({"handles":[g["frames"][0]["handle"]],"delta":[10,5]})).unwrap();
    let result = s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert!(!result["profiles"]["clipped"][0]["spans"].as_array().unwrap().is_empty());
}

#[test]
fn missing_and_oversized_profile_views_refuse_before_changing_any_layout() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let h=s.execute("alignmentcreate",&json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap()["handle"].clone();
    let g = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap_err().to_string().contains("MISSING_PROFILE_VIEW"));
    assert_eq!(*s.doc().unwrap(), before);
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    s.execute(
        "profileviewframes",
        &json!({"group":g["group"],"at":[2000,2000],"height":200,"profile":"design","center":"median","datum_increment":5}),
    )
    .unwrap();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap_err().to_string().contains("PROFILE_VIEW_OUTSIDE"));
    assert_eq!(*s.doc().unwrap(), before);
}
#[test]
fn table_conflicts_keep_current_definitions_but_unequal_blocks_refuse() {
    use cadcraft_color::Color;
    use cadcraft_engine::doc::Layer;
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let mut layer = Layer::new("SHEET-PLAN-1");
    layer.color = Color::from_aci(1);
    s.doc_mut().unwrap().layers.push(layer.clone());
    let result = s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    assert_eq!(s.doc().unwrap().layer("SHEET-PLAN-1").unwrap(), &layer);
    assert!(result["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "table_definition_kept"));
    let mut other = Session::new();
    let g = group(&mut other);
    other.doc_mut().unwrap().blocks.insert("North".into(), Arc::new(Block::new("North")));
    let before = other.doc().unwrap().clone();
    assert!(other.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap_err().to_string().contains("BLOCK_COLLISION"));
    assert_eq!(*other.doc().unwrap(), before);
}
#[test]
fn cyclic_template_blocks_are_refused() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut d = cadcraft_io::read(&std::fs::read(&t).unwrap(), &t).unwrap();
    let h = d.new_handle();
    let b = Arc::make_mut(d.blocks.get_mut("North").unwrap());
    b.entities.push(Entity {
        handle: h,
        common: Common::default(),
        kind: EntityKind::Insert(Insert { block: "North".into(), ..Default::default() }),
    });
    std::fs::write(&t, cadcraft_io::write(&d, &t).unwrap()).unwrap();
    let mut s = Session::new();
    let g = group(&mut s);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap_err().to_string().contains("TEMPLATE_CYCLE"));
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn viewport_scale_overflow_refuses_without_leaving_nonfinite_state() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let h=s.execute("alignmentcreate",&json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap()["handle"].clone();
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    let g = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[1e-306,1e-306],"scale":1e308,"spacing":50})).unwrap();
    s.execute(
        "profileviewframes",
        &json!({"group":g["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5}),
    )
    .unwrap();
    let before = s.doc().unwrap().clone();
    let err = s.execute("sheetcreate", &request(&g, &t, "profile_only")).unwrap_err().to_string();
    assert!(err.contains("VIEWPORT_OVERFLOW"), "{err}");
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn block_metadata_is_part_of_the_collision_contract() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let source = cadcraft_io::read(&std::fs::read(&t).unwrap(), &t).unwrap();
    let mut block = (**source.block("North").unwrap()).clone();
    block.units = 4;
    let mut s = Session::new();
    let g = group(&mut s);
    s.doc_mut().unwrap().blocks.insert("North".into(), Arc::new(block));
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap_err().to_string().contains("BLOCK_COLLISION"));
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn plan_only_updates_matchlines_without_any_profile_stage() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let h=s.execute("alignmentcreate",&json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap()["handle"].clone();
    let g = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    s.execute("move", &json!({"handles":[g["frames"][0]["handle"]],"delta":[10,5]})).unwrap();
    s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    let mut labels = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::MText(t) if e.common.layer == "CADCRAFT-MATCHLINES" => Some(t.contents.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    labels.sort();
    assert_eq!(labels, ["0+250.000", "0+260.000", "0+500.000", "0+750.000"]);
}
#[test]
fn an_erased_owned_profile_component_refuses_sheet_creation() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    let h = s
        .doc()
        .unwrap()
        .model
        .iter()
        .find(|e| e.common.layer.starts_with("CADCRAFT-SHEET-PROFILE-") && matches!(e.kind, EntityKind::LwPolyline(_)))
        .unwrap()
        .handle
        .hex();
    s.execute("erase", &json!({"handles":[h]})).unwrap();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap_err().to_string().contains("MISSING_PROFILE_VIEW"));
    assert_eq!(*s.doc().unwrap(), before);
}
#[test]
fn model_output_edits_invalidate_the_unchanged_shortcut() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1", "SHEET-PROFILE-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_profile")).unwrap();
    let matchline = s.doc().unwrap().model.iter().find(|e| e.common.layer == "CADCRAFT-MATCHLINES").unwrap().handle;
    s.execute("erase", &json!({"handles":[matchline.hex()]})).unwrap();
    assert_eq!(s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap()["changed"], true);
    let h = s
        .doc()
        .unwrap()
        .model
        .iter()
        .find(|e| e.common.layer.starts_with("CADCRAFT-SHEET-PROFILE-") && matches!(e.kind, EntityKind::LwPolyline(_)))
        .unwrap()
        .handle;
    s.execute("move", &json!({"handles":[h.hex()],"delta":[0,100]})).unwrap();
    assert_eq!(s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap()["changed"], true);
    assert!(s.doc().unwrap().entity(h).is_none(), "moved generated component is replaced");
    assert_eq!(s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap()["changed"], false);
}
#[test]
fn rebuilding_the_active_sheet_clears_an_erased_mspace_viewport() {
    hooks();
    let temp = Temp::new();
    let t = template(&temp, &["SHEET-PLAN-1"]);
    let mut s = Session::new();
    let g = group(&mut s);
    s.execute("sheetcreate", &request(&g, &t, "plan_only")).unwrap();
    let h =
        s.doc().unwrap().layout("Road-010").unwrap().entities.iter().find(|e| matches!(&e.kind,EntityKind::Viewport(v) if v.id!=1)).unwrap().handle;
    s.execute("move", &json!({"handles":[g["frames"][0]["handle"]],"delta":[10,5]})).unwrap();
    s.state_mut().unwrap().space = Space::Paper("Road-010".into());
    s.state_mut().unwrap().mspace = Some(h);
    s.execute("sheetupdate", &json!({"group":g["group"]})).unwrap();
    assert_eq!(s.space(), Space::Paper("Road-010".into()));
    assert_eq!(s.state().unwrap().mspace, None);
}
