use cadcraft_engine::{
    Session,
    doc::{Common, Drawing, EntityKind, Layout, Space, Viewport},
    geom::Vec3,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
struct Fixture {
    root: PathBuf,
    session: Session,
    group: Value,
    params: Value,
    alignment: Value,
}
impl Fixture {
    fn new() -> Self {
        cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
            read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
            write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
            plot: None,
        });
        let root = std::env::temp_dir().join(format!(
            "cadcraft-sheet-drawings-{}",
            cadcraft_proposal::approval::sha256(format!("{:?}-{:?}", std::time::SystemTime::now(), std::thread::current().id()).as_bytes())
        ));
        std::fs::create_dir(&root).unwrap();
        let template = root.join("template.dxf");
        let mut d = Drawing::new_metric();
        d.layouts = vec![Layout::new("Template", 1)];
        d.add(
            &Space::Paper("Template".into()),
            Common { layer: "SHEET-PLAN-1".into(), ..Default::default() },
            EntityKind::Viewport(Viewport { id: 2, center: Vec3::new(160., 80., 0.), width: 300., height: 100., ..Default::default() }),
        )
        .unwrap();
        std::fs::write(&template, cadcraft_io::write(&d, &template).unwrap()).unwrap();
        let mut session = Session::new();
        let a = session
            .execute("alignmentcreate", &json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]}))
            .unwrap();
        let group = session.execute("viewframecreate", &json!({"alignment":a["handle"],"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
        let params = json!({"group":group["group"],"template":template,"template_layout":"Template","arrangement":"plan_only","layout_name":"Sheet-{n}","storage":"drawings","output_folder":root,"drawing_name":"Road-{n}","layouts_per_drawing":2,"duplicate_objects":true});
        Self { root, session, group, params, alignment: a["handle"].clone() }
    }
    fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(&self.root)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())
            })
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn drawings_require_confirmation_and_publish_reopened_batches_without_overwrite() {
    let mut f = Fixture::new();
    let before = f.session.doc().unwrap().clone();
    let files = f.bytes();
    let mut missing = f.params.clone();
    missing.as_object_mut().unwrap().remove("duplicate_objects");
    assert!(f.session.execute("sheetcreate", &missing).is_err());
    assert_eq!(f.bytes(), files);
    let r = f.session.execute("sheetcreate", &f.params).unwrap();
    assert_eq!(r["written"].as_array().unwrap().len(), 2);
    assert_eq!(r["undo_removes_files"], false);
    assert_eq!(f.session.doc().unwrap().layouts, before.layouts, "source layouts are untouched");
    for n in [1, 2] {
        let path = f.root.join(format!("Road-{n}.dwg"));
        let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path).unwrap();
        assert_eq!(d.layouts.len(), 2);
        assert_eq!(d.model.len(), f.session.doc().unwrap().model.len());
    }
    let written = f.bytes();
    f.session.undo().unwrap();
    assert_eq!(*f.session.doc().unwrap(), before);
    assert_eq!(f.bytes(), written, "undo leaves published files");
    assert!(f.session.execute("sheetcreate", &f.params).unwrap_err().to_string().contains("COLLISION"));
    assert_eq!(f.bytes(), written);
    assert_eq!(*f.session.doc().unwrap(), before);
}
#[test]
fn drawings_update_versions_owned_files_and_refuses_the_exact_colliding_version() {
    let mut f = Fixture::new();
    f.session.execute("sheetcreate", &f.params).unwrap();
    let old = f.bytes();
    let update = json!({"group":f.group["group"],"duplicate_objects":true});
    assert_eq!(f.session.execute("sheetupdate", &update).unwrap()["changed"], false);
    let mut empty = update.clone();
    empty["add_frames"] = json!([]);
    assert_eq!(f.session.execute("sheetupdate", &empty).unwrap()["changed"], false);
    f.session.execute("move", &json!({"handles":[f.group["frames"][0]["handle"]],"delta":[10,5]})).unwrap();
    std::fs::write(f.root.join("Road-2_v2.dwg"), b"unowned").unwrap();
    let before = f.session.doc().unwrap().clone();
    let files = f.bytes();
    assert!(f.session.execute("sheetupdate", &update).unwrap_err().to_string().contains("COLLISION"));
    assert_eq!(f.bytes(), files);
    assert_eq!(*f.session.doc().unwrap(), before);
    std::fs::remove_file(f.root.join("Road-2_v2.dwg")).unwrap();
    let r = f.session.execute("sheetupdate", &update).unwrap();
    assert_eq!(r["written"].as_array().unwrap().len(), 2);
    assert_eq!(r["superseded"].as_array().unwrap().len(), 2);
    assert!(f.root.join("Road-1_v2.dwg").is_file());
    for (name, bytes) in old {
        assert_eq!(std::fs::read(f.root.join(name)).unwrap(), bytes);
    }
}
#[test]
fn duplicate_drawing_names_and_changed_owned_files_refuse_without_outputs() {
    let mut f = Fixture::new();
    f.params["drawing_name"] = json!("Same");
    let files = f.bytes();
    assert!(f.session.execute("sheetcreate", &f.params).unwrap_err().to_string().contains("COLLISION"));
    assert_eq!(f.bytes(), files);
    f.params["drawing_name"] = json!("Road-{n}");
    f.session.execute("sheetcreate", &f.params).unwrap();
    std::fs::write(f.root.join("Road-1.dwg"), b"changed by user").unwrap();
    let files = f.bytes();
    let before = f.session.doc().unwrap().clone();
    assert!(f.session.execute("sheetupdate", &json!({"group":f.group["group"],"duplicate_objects":true})).is_err());
    assert_eq!(f.bytes(), files);
    assert_eq!(*f.session.doc().unwrap(), before);
}
#[test]
fn profile_drawings_and_source_manifest_survive_both_drawing_formats() {
    let mut f = Fixture::new();
    let template = PathBuf::from(f.params["template"].as_str().unwrap());
    let mut d = cadcraft_io::read(&std::fs::read(&template).unwrap(), &template).unwrap();
    d.add(
        &Space::Paper("Template".into()),
        Common { layer: "SHEET-PROFILE-1".into(), ..Default::default() },
        EntityKind::Viewport(Viewport { id: 3, center: Vec3::new(160., 220., 0.), width: 300., height: 100., ..Default::default() }),
    )
    .unwrap();
    std::fs::write(&template, cadcraft_io::write(&d, &template).unwrap()).unwrap();
    f.session
        .execute("profilecreate", &json!({"alignment":f.alignment,"pvis":[{"station":0,"elevation":100},{"station":1000,"elevation":110}]}))
        .unwrap();
    f.session
        .execute(
            "profileviewframes",
            &json!({"group":f.group["group"],"at":[2000,2000],"height":40,"profile":"design","center":"median","datum_increment":5}),
        )
        .unwrap();
    f.params["arrangement"] = json!("plan_profile");
    f.session.execute("sheetcreate", &f.params).unwrap();
    let files = f.bytes();
    for name in ["source.dxf", "source.dwg"] {
        let d = cadcraft_io::read(&cadcraft_io::write(f.session.doc().unwrap(), name).unwrap(), name).unwrap();
        let mut s = Session::new();
        *s.doc_mut().unwrap() = d;
        let r = s.execute("sheetupdate", &json!({"group":f.group["group"],"duplicate_objects":true})).unwrap();
        assert_eq!(r["changed"], false, "{name}: reopening alone must not publish another version");
        assert_eq!(f.bytes(), files);
    }
    for name in ["Road-1.dwg", "Road-2.dwg"] {
        let path = f.root.join(name);
        let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path).unwrap();
        assert!(d.model.iter().any(|e| e.common.layer.starts_with("CADCRAFT-SHEET-PROFILE-")));
        let mut copy = Session::new();
        *copy.doc_mut().unwrap() = d;
        assert!(copy.execute("sheetupdate", &json!({"group":f.group["group"]})).unwrap_err().to_string().contains("SHEET_COPY"));
    }
}
#[test]
fn hostile_drawing_settings_and_unsupported_references_leave_no_stage_or_files() {
    let mut f = Fixture::new();
    let files = f.bytes();
    let before = f.session.doc().unwrap().clone();
    for value in [json!(0), json!(-1), json!(4097), json!("2"), json!(null)] {
        let mut p = f.params.clone();
        p["layouts_per_drawing"] = value;
        assert!(f.session.execute("sheetcreate", &p).is_err());
        assert_eq!(f.bytes(), files);
        assert_eq!(*f.session.doc().unwrap(), before);
    }
    for name in ["../outside", "CON", "LPT1", "bad/name"] {
        let mut p = f.params.clone();
        p["drawing_name"] = json!(name);
        assert!(f.session.execute("sheetcreate", &p).is_err());
        assert_eq!(f.bytes(), files);
    }
    let mut p = f.params.clone();
    p["duplicate_objects"] = json!(false);
    p["output_folder"] = json!(f.root.join("not-created"));
    assert!(f.session.execute("sheetcreate", &p).is_err());
    assert!(!f.root.join("not-created").exists());
    let mut block = cadcraft_engine::doc::Block::new("External");
    block.xref_path = Some("missing.dwg".into());
    f.session.doc_mut().unwrap().blocks.insert("External".into(), std::sync::Arc::new(block));
    let before = f.session.doc().unwrap().clone();
    assert!(f.session.execute("sheetcreate", &f.params).unwrap_err().to_string().contains("DRAWING_DATA_UNSUPPORTED"));
    assert_eq!(f.bytes(), files);
    assert_eq!(*f.session.doc().unwrap(), before);
}
#[test]
fn corrupt_model_ownership_refuses_even_when_geometry_is_unchanged() {
    let mut f = Fixture::new();
    f.session.execute("sheetcreate", &f.params).unwrap();
    let h = cadcraft_engine::doc::Handle::parse_hex(f.group["frames"][0]["handle"].as_str().unwrap()).unwrap();
    std::sync::Arc::make_mut(&mut f.session.doc_mut().unwrap().foreign).entities.remove(&h);
    let files = f.bytes();
    let before = f.session.doc().unwrap().clone();
    assert!(f.session.execute("sheetupdate", &json!({"group":f.group["group"],"duplicate_objects":true})).is_err());
    assert_eq!(*f.session.doc().unwrap(), before);
    assert_eq!(f.bytes(), files);
}
#[test]
fn site_drawings_copy_the_placed_block_and_keep_an_odd_last_batch() {
    use cadcraft_engine::{
        doc::{Block, Entity, Insert, LwPolyline},
        geom::{PolyVertex, Vec2},
    };
    let mut f = Fixture::new();
    f.session = Session::new();
    let d = f.session.doc_mut().unwrap();
    let mut block = Block::new("SiteFrame");
    block.entities.push(Entity {
        handle: d.new_handle(),
        common: Common::default(),
        kind: EntityKind::LwPolyline(LwPolyline {
            vertices: [[0., 0.], [300., 0.], [300., 100.], [0., 100.]].into_iter().map(|[x, y]| PolyVertex::new(Vec2::new(x, y))).collect(),
            closed: true,
            elevation: 0.,
            const_width: 0.,
            plinegen: false,
        }),
    });
    d.blocks.insert("SiteFrame".into(), std::sync::Arc::new(block));
    let handles = (0..3)
        .map(|i| {
            d.add(
                &Space::Model,
                Common::default(),
                EntityKind::Insert(Insert { block: "SiteFrame".into(), insert: Vec3::new(f64::from(i) * 400., 0., 0.), ..Default::default() }),
            )
            .unwrap()
            .hex()
        })
        .collect::<Vec<_>>();
    let g = f.session.execute("siteviewframes", &json!({"handles":handles,"scale":1})).unwrap();
    f.params["group"] = g["group"].clone();
    f.params["drawing_name"] = json!("Site-{n}");
    let r = f.session.execute("sheetcreate", &f.params).unwrap();
    assert_eq!(r["written"].as_array().unwrap().len(), 2);
    for (name, count) in [("Site-1.dwg", 2), ("Site-2.dwg", 1)] {
        let path = f.root.join(name);
        let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path).unwrap();
        assert_eq!(d.layouts.len(), count);
        assert!(d.block("SiteFrame").is_some());
        assert_eq!(d.model.iter().filter(|e| matches!(e.kind, EntityKind::Insert(_))).count(), 3);
    }
}
