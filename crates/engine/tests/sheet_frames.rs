use cadcraft_engine::{
    Session,
    doc::{EntityKind, Handle},
};
use serde_json::{Value, json};
fn road(s: &mut Session) -> Value {
    s.execute("alignmentcreate", &json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap()["handle"]
        .clone()
}
#[test]
fn road_group_profiles_roundtrip_and_each_command_undoes_as_one_step() {
    let mut s = Session::new();
    let h = road(&mut s);
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    let before = s.doc().unwrap().clone();
    let group =
        s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250,"increment":10,"offset":5})).unwrap();
    assert_eq!(group["frames"].as_array().unwrap().len(), 4);
    let with_frames = s.doc().unwrap().clone();
    let p = json!({"group":group["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5});
    let views = s.execute("profileviewframes", &p).unwrap();
    assert_eq!(views["views"].as_array().unwrap().len(), 4);
    assert!(views["views"][1]["at"][1].as_f64().unwrap() < 2000.);
    let count = s.doc().unwrap().model.len();
    let records = s.doc().unwrap().foreign.objects.len();
    s.execute("profileviewframes", &p).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), count, "rerun replaces owned views");
    assert_eq!(s.doc().unwrap().foreign.objects.len(), records, "replacing views must not leak ownership dictionaries");
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(*s.doc().unwrap(), with_frames);
    s.undo().unwrap();
    assert_eq!(*s.doc().unwrap(), before);
    let group = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250,"increment":10})).unwrap();
    for file in ["frames.dxf", "frames.dwg"] {
        let mut drawing = s.doc().unwrap().clone();
        for _ in 0..2 {
            drawing = cadcraft_io::read(&cadcraft_io::write(&drawing, file).unwrap(), file).unwrap();
            let mut reopened = Session::new();
            *reopened.doc_mut().unwrap() = drawing.clone();
            let v = reopened
                .execute(
                    "profileviewframes",
                    &json!({"group":group["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5}),
                )
                .unwrap();
            assert_eq!(v["views"].as_array().unwrap().len(), 4);
        }
    }
}
#[test]
fn copy_of_profiled_alignment_keeps_design_and_drops_ground() {
    let mut s = Session::new();
    let h = road(&mut s);
    let tin = s.execute("tincreate", &json!({"points":[[-10,-10,100],[1010,-10,100],[-10,10,110],[1010,10,110]]})).unwrap();
    s.execute("profilesample", &json!({"alignment":h,"surface":tin["handle"]})).unwrap();
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    let copied = s.execute("copy", &json!({"handles":[h],"delta":[0,100]})).unwrap()["handles"][0].clone();
    let result = s.execute("profileview", &json!({"alignment":copied,"at":[2000,2000]})).unwrap();
    let layers: Vec<_> = result["handles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| s.doc().unwrap().entity(Handle::parse_hex(h.as_str().unwrap()).unwrap()).unwrap().common.layer.clone())
        .collect();
    assert!(layers.iter().any(|l| l == "PROFILE-DESIGN"));
    assert!(!layers.iter().any(|l| l == "PROFILE-GROUND"));
    let result = s.execute("profileview", &json!({"alignment":h,"at":[2000,2000]})).unwrap();
    assert!(
        result["handles"].as_array().unwrap().iter().any(|h| s
            .doc()
            .unwrap()
            .entity(Handle::parse_hex(h.as_str().unwrap()).unwrap())
            .unwrap()
            .common
            .layer
            == "PROFILE-GROUND")
    );
}
#[test]
fn missing_or_copied_anchor_and_invalid_frames_leave_drawing_unchanged() {
    let mut s = Session::new();
    let h = road(&mut s);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[0,100],"scale":1})).is_err());
    assert_eq!(*s.doc().unwrap(), before);
    let group = s.execute("viewframecreate", &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
    let copy = s.execute("copy", &json!({"handles":[group["group"]],"delta":[0,10]})).unwrap();
    let before = s.doc().unwrap().clone();
    assert!(
        s.execute(
            "profileviewframes",
            &json!({"group":copy["handles"][0],"at":[0,0],"height":40,"profile":"design","center":"median","datum_increment":5})
        )
        .is_err()
    );
    assert_eq!(*s.doc().unwrap(), before);
    assert!(s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::LwPolyline(_))).count() >= 5);
}

#[test]
fn site_group_reads_placed_blocks_once_and_refuses_mirrors() {
    use cadcraft_engine::{
        doc::{Block, Common, Entity, Insert, LwPolyline, Space},
        geom::{PolyVertex, Vec2, Vec3},
    };
    let mut s = Session::new();
    let mut b = Block::new("SiteFrame");
    b.base = Vec3::new(10., 20., 0.);
    let d = s.doc_mut().unwrap();
    let handle = d.new_handle();
    b.entities.push(Entity {
        handle,
        common: Common::default(),
        kind: EntityKind::LwPolyline(LwPolyline {
            vertices: [[10., 20.], [40., 20.], [40., 30.], [10., 30.]].into_iter().map(|[x, y]| PolyVertex::new(Vec2::new(x, y))).collect(),
            closed: true,
            elevation: 0.,
            const_width: 0.,
            plinegen: false,
        }),
    });
    d.blocks.insert(b.name.clone(), std::sync::Arc::new(b));
    let mut handles = Vec::new();
    for x in [100., 200., 300.] {
        handles.push(
            d.add(
                &Space::Model,
                Common::default(),
                EntityKind::Insert(Insert {
                    block: "SiteFrame".into(),
                    insert: Vec3::new(x, 200., 0.),
                    scale: Vec3::new(2., 2., 2.),
                    rotation: std::f64::consts::FRAC_PI_2,
                    ..Default::default()
                }),
            )
            .unwrap()
            .hex(),
        );
    }
    let before = s.doc().unwrap().clone();
    let result = s.execute("siteviewframes", &json!({"handles":handles,"scale":2})).unwrap();
    assert_eq!(result["frames"].as_array().unwrap().len(), 3);
    s.undo().unwrap();
    assert_eq!(*s.doc().unwrap(), before);
    s.doc_mut()
        .unwrap()
        .modify_entity(Handle::parse_hex(&handles[0]).unwrap(), |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.scale.x = -2.;
            }
        })
        .unwrap();
    let before = s.doc().unwrap().clone();
    assert!(s.execute("siteviewframes", &json!({"handles":handles,"scale":2})).is_err());
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn named_matchline_attributes_are_written_and_missing_names_refuse() {
    let mut s = Session::new();
    let h = road(&mut s);
    let attr = s.execute("attdef", &json!({"tag":"STATION","at":[0,0],"height":2})).unwrap();
    s.execute("block", &json!({"name":"Match","base":[0,0],"handles":[attr["handle"]],"keep":"delete"})).unwrap();
    let result = s
        .execute(
            "viewframecreate",
            &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250,"matchline":{"block":"Match","attribute":"STATION"}}),
        )
        .unwrap();
    let inserts: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| if let EntityKind::Insert(i) = &e.kind { Some(i) } else { None }).collect();
    assert_eq!(inserts.len(), 8);
    assert_eq!(inserts[0].attribs[0].text.value, "0+000.000");
    assert_eq!(inserts[1].attribs[0].text.value, "0+250.000");
    assert_eq!(result["frames"].as_array().unwrap().len(), 4);
    let before = s.doc().unwrap().clone();
    assert!(
        s.execute(
            "viewframecreate",
            &json!({"alignment":h,"paper_size":[300,100],"scale":1,"spacing":250,"matchline":{"block":"Match","attribute":"MISSING"}})
        )
        .is_err()
    );
    assert_eq!(*s.doc().unwrap(), before);
}

#[test]
fn moved_frames_recover_chord_stations_and_stretched_frames_refuse() {
    let mut s = Session::new();
    let h = road(&mut s);
    s.execute("profilecreate", &json!({"alignment":h,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]})).unwrap();
    let group = s.execute("viewframecreate", &json!({"alignment":h,"range":[100,900],"paper_size":[250,100],"scale":1,"spacing":200})).unwrap();
    let p = json!({"group":group["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5});
    let first = group["frames"][0]["handle"].clone();
    s.execute("move", &json!({"handles":[first],"delta":[10,5]})).unwrap();
    let views = s.execute("profileviewframes", &p).unwrap();
    assert_eq!(views["views"][0]["window"]["range"], json!([110., 310.]));
    // Frame width 250 exceeds its 200-long chord: projecting rectangle edges would yield 85,335.
    assert_eq!(s.execute("profileviewframes", &p).unwrap()["views"][0]["window"]["range"], json!([110., 310.]));
    s.execute("scale", &json!({"handles":[first],"base":[0,0],"factor":2})).unwrap();
    let before = s.doc().unwrap().clone();
    let err = s.execute("profileviewframes", &p).unwrap_err().to_string();
    assert!(err.contains("FRAME_STRETCHED"), "{err}");
    assert_eq!(*s.doc().unwrap(), before);
}
