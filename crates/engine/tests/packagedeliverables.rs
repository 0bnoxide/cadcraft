#![cfg(not(target_arch = "wasm32"))]
use cadcraft_doc::{Block, Drawing, Entity, EntityKind, Handle};
use cadcraft_engine::{
    Session,
    cmd::file::{IoHooks, set_io},
};
use serde_json::{Value, json};
use std::path::PathBuf;
mod common {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../proposal/tests/common/mod.rs"));
}

fn project(travel: bool) -> (PathBuf, Value) {
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: Some(|d, s, p| cadcraft_io::plotter::plot(d, s, p).map_err(|e| e.to_string())),
    });
    let mut manifest = common::fixture();
    let root = PathBuf::from(manifest["base_roots"][0]["path"].as_str().unwrap());
    std::fs::create_dir(&root).unwrap();
    manifest["version"] = json!(2);
    manifest["source_roles"] = json!(["survey", "record_drawing", "gis_export", "imagery", "other"]);
    manifest["drawing_settings"] = json!({});
    std::fs::write(root.join("standards.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut d = Drawing::new_metric();
    let mut title = Block::new("Title");
    title.entities.push(Entity {
        handle: Handle(900),
        common: Default::default(),
        kind: EntityKind::AttDef(serde_json::from_value(json!({"tag":"CLIENT","text":{"insert":[0,0,0],"height":2.5,"value":""}})).unwrap()),
    });
    d.blocks.insert("Title".into(), std::sync::Arc::new(title));
    for name in ["model.dxf", "sheet.dxf"] {
        std::fs::write(root.join(name), cadcraft_io::write(&d, name).unwrap()).unwrap();
    }
    let mut request = json!({"action":"plan","manifest_path":root.join("standards.json"),"inputs":{"client_name":"Client","site_name":"Site","proposal_year":2026,"orientation":"Landscape","sheet_size":"A3","client_number":"C1"}});
    let mut s = Session::new();
    let planned = s.execute("newproposal", &request).unwrap();
    request["action"] = json!("create");
    request["approved_hash"] = planned["hash"].clone();
    assert_eq!(s.execute("newproposal", &request).unwrap()["receipt"]["outcome"], "Succeeded");
    std::fs::write(root.join("qa.txt"), b"QA passed").unwrap();
    std::fs::write(
        root.join("profile.json"),
        serde_json::to_vec(&json!({"include_drawings":travel,"extra_files":[{"path":"qa.txt","role":"qa_report"}]})).unwrap(),
    )
    .unwrap();
    let p = json!({"action":"plan","manifest_path":root.join("standards.json"),"proposal_root":root.join("Client - Site"),"profile_path":root.join("profile.json")});
    (root, p)
}
#[test]
fn package_has_one_pdf_per_sheet_exact_zip_entries_and_version_pair() {
    let (root, mut p) = project(true);
    let mut s = Session::new();
    let plan = s.execute("packagedeliverables", &p).unwrap();
    assert!(plan["hash"].is_string(), "{plan}");
    assert_eq!(plan, s.execute("packagedeliverables", &p).unwrap());
    assert_eq!(plan["plan"]["sheets"].as_array().unwrap().len(), 3);
    p["action"] = json!("execute");
    p["approved_hash"] = plan["hash"].clone();
    let done = s.execute("packagedeliverables", &p).unwrap();
    assert_eq!(done["receipt"]["outcome"], "Succeeded", "{done}");
    let package = PathBuf::from(done["receipt"]["package_path"].as_str().unwrap());
    let manifest: Value = serde_json::from_slice(&std::fs::read(package.join("checksums.json")).unwrap()).unwrap();
    let rows = manifest["files"].as_array().unwrap();
    assert_eq!(rows.iter().filter(|r| r["role"] == "sheet_pdf").count(), 3);
    assert!(package.join("qa/qa.txt").exists());
    assert!(package.join("drawings/Models/base.dxf").exists());
    for row in rows {
        let bytes = std::fs::read(package.join(row["path"].as_str().unwrap())).unwrap();
        assert_eq!(cadcraft_proposal::approval::sha256(&bytes), row["sha256"].as_str().unwrap());
    }
    assert_eq!(done["receipt"]["verified_zip_entries"].as_array().unwrap().len(), rows.len() + 1);
    let zip_path = PathBuf::from(done["receipt"]["zip_path"].as_str().unwrap());
    assert_eq!(cadcraft_proposal::approval::sha256(&std::fs::read(&zip_path).unwrap()), done["receipt"]["zip_sha256"].as_str().unwrap());
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let actual: std::collections::BTreeSet<_> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned()).collect();
    let expected: std::collections::BTreeSet<_> = plan["plan"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap().to_string())
        .chain(std::iter::once("checksums.json".into()))
        .collect();
    assert_eq!(actual, expected);
    for row in rows {
        let mut entry = zip.by_name(row["path"].as_str().unwrap()).unwrap();
        assert_eq!(cadcraft_proposal::approval::reader_hash(&mut entry).unwrap(), row["sha256"].as_str().unwrap());
    }
    drop(zip);
    p["action"] = json!("plan");
    let next = s.execute("packagedeliverables", &p).unwrap();
    assert!(next["plan"]["package_path"].as_str().unwrap().ends_with("Package_v2"));
    assert!(next["plan"]["zip_path"].as_str().unwrap().ends_with("Package_v2.zip"));
    std::fs::create_dir(root.join("Client - Site/Deliverables/Package_v2")).unwrap();
    assert!(s.execute("packagedeliverables", &p).unwrap()["plan"]["zip_path"].as_str().unwrap().ends_with("Package_v3.zip"));
    std::fs::write(root.join("Client - Site/Deliverables/Package_v3.zip"), b"earlier ZIP").unwrap();
    assert!(s.execute("packagedeliverables", &p).unwrap()["plan"]["package_path"].as_str().unwrap().ends_with("Package_v4"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn colliding_extras_and_reserved_manifest_refuse_before_output() {
    let (root, p) = project(false);
    std::fs::create_dir(root.join("other")).unwrap();
    std::fs::write(root.join("other/qa.txt"), b"another").unwrap();
    let profile = root.join("profile.json");
    let mut s = Session::new();
    std::fs::write(
        &profile,
        serde_json::to_vec(&json!({"extra_files":[{"path":"qa.txt","role":"qa_report"},{"path":"other/qa.txt","role":"qa_report"}]})).unwrap(),
    )
    .unwrap();
    assert_eq!(s.execute("packagedeliverables", &p).unwrap()["issues"][0]["code"], "output_collision");
    std::fs::write(root.join("checksums.json"), b"unrelated").unwrap();
    std::fs::write(&profile, serde_json::to_vec(&json!({"extra_files":[{"path":"checksums.json","role":"other"}]})).unwrap()).unwrap();
    assert_eq!(s.execute("packagedeliverables", &p).unwrap()["issues"][0]["code"], "output_collision");
    assert!(!root.join("Client - Site/Deliverables").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn linked_extra_is_refused_without_output() {
    let (root, p) = project(false);
    std::os::unix::fs::symlink(root.join("qa.txt"), root.join("linked.txt")).unwrap();
    std::fs::write(root.join("profile.json"), serde_json::to_vec(&json!({"extra_files":[{"path":"linked.txt","role":"qa_report"}]})).unwrap())
        .unwrap();
    assert_eq!(Session::new().execute("packagedeliverables", &p).unwrap()["issues"][0]["code"], "unsafe_path");
    assert!(!root.join("Client - Site/Deliverables").exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn missing_layout_and_missing_extra_fail_before_plotting_and_changed_sheet_is_stale() {
    let (root, mut p) = project(false);
    let mut s = Session::new();
    let plan = s.execute("packagedeliverables", &p).unwrap();
    let sheet = root.join("Client - Site/Sheets/0.dxf");
    let original = std::fs::read(&sheet).unwrap();
    let mut d = cadcraft_io::read(&original, &sheet).unwrap();
    d.header.set_i64("LUPREC", 7);
    std::fs::write(&sheet, cadcraft_io::write(&d, &sheet).unwrap()).unwrap();
    p["action"] = json!("execute");
    p["approved_hash"] = plan["hash"].clone();
    let result = s.execute("packagedeliverables", &p).unwrap();
    assert_eq!(result["receipt"]["failed_checks"][0]["code"], "stale_approval", "{result}");
    d.layouts.clear();
    std::fs::write(&sheet, cadcraft_io::write(&d, &sheet).unwrap()).unwrap();
    p["action"] = json!("plan");
    assert_eq!(s.execute("packagedeliverables", &p).unwrap()["issues"][0]["code"], "layout_missing");
    assert!(!root.join("Client - Site/Deliverables").exists());
    std::fs::write(&sheet, original).unwrap();
    std::fs::remove_file(root.join("qa.txt")).unwrap();
    assert!(s.execute("packagedeliverables", &p).unwrap()["hash"].is_null());
    assert!(!root.join("Client - Site/Deliverables").exists());
    std::fs::remove_dir_all(root).unwrap();
}
