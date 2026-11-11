#![cfg(not(target_arch = "wasm32"))]
use cadcraft_doc::{Drawing, EntityKind, Layer, Layout, Space, Viewport};
use cadcraft_engine::{
    Session,
    cmd::file::{IoHooks, set_io},
};
use serde_json::{Value, json};
#[path = "support/pdf_font.rs"]
mod pdf_font;
#[path = "../../io/tests/support/pdf_read.rs"]
mod pdf_read;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn fixture() -> (PathBuf, Value) {
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: Some(|d, s, p| cadcraft_io::plotter::plot(d, s, p).map_err(|e| e.to_string())),
    });
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!("cadcraft-figures-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir(&root).unwrap();
    let mut d = Drawing::new_metric();
    d.layers.push(Layer::new("Boundary"));
    d.layouts.push(Layout::new("Figure", 1));
    let text = serde_json::from_value(json!({"insert":[10,10,0],"height":3,"value":"{{Site}} {{Event}} {{FigureId}}"})).unwrap();
    d.add(&Space::Paper("Figure".into()), Default::default(), EntityKind::Text(text)).unwrap();
    d.add(
        &Space::Paper("Figure".into()),
        Default::default(),
        EntityKind::Viewport(Viewport { width: 100., height: 100., view_height: 10., id: 2, ..Default::default() }),
    )
    .unwrap();
    std::fs::write(root.join("source.dxf"), cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    std::fs::write(root.join("figure.json"), serde_json::to_vec(&json!({"figure_id":"F","source_drawing":"source.dxf","layout":"Figure"})).unwrap())
        .unwrap();
    let p = json!({"action":"plan","spec_paths":[root.join("figure.json")],"sites":["A","B"],"events":["2025","2026"],"mode":"per_site","format":"pdf","output_folder":root.join("out")});
    (root, p)
}

#[test]
fn plans_four_jobs_exports_versions_and_preserves_source_and_current_session() {
    let (root, mut p) = fixture();
    let original = std::fs::read(root.join("source.dxf")).unwrap();
    let mut s = Session::new();
    s.new_drawing(true);
    s.execute("line", &json!({"points":[[0,0],[1,1]]})).unwrap();
    let before = s.doc().unwrap().clone();
    let undo = format!("{:?}", s.docs[s.active].undo);
    let epoch = s.doc_epoch;
    let planned = s.execute("figureexport", &p).unwrap();
    assert_eq!(planned["plan"]["jobs"].as_array().unwrap().len(), 4);
    assert_eq!(serde_json::to_vec(&planned).unwrap(), serde_json::to_vec(&s.execute("figureexport", &p).unwrap()).unwrap());
    assert!(!root.join("out").exists());
    p["action"] = json!("execute");
    p["approved_hash"] = planned["hash"].clone();
    let done = s.execute("figureexport", &p).unwrap();
    assert_eq!(done["receipt"]["outcome"], "Succeeded", "{done}");
    for name in ["A_2025_F.pdf", "A_2026_F.pdf", "B_2025_F.pdf", "B_2026_F.pdf"] {
        assert!(std::fs::read(root.join("out").join(name)).unwrap().starts_with(b"%PDF-"));
    }
    let stale = s.execute("figureexport", &p).unwrap();
    assert_eq!(stale["receipt"]["failed_checks"][0]["code"], "stale_approval");
    p["action"] = json!("plan");
    let next = s.execute("figureexport", &p).unwrap();
    assert_eq!(next["plan"]["jobs"][0]["filename"], "A_2025_F_v2.pdf");
    p["action"] = json!("execute");
    p["approved_hash"] = next["hash"].clone();
    assert_eq!(s.execute("figureexport", &p).unwrap()["receipt"]["outcome"], "Succeeded");
    assert_eq!(std::fs::read(root.join("source.dxf")).unwrap(), original);
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(format!("{:?}", s.docs[s.active].undo), undo);
    assert_eq!(s.doc_epoch, epoch);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exports_png_with_decodable_pixels() {
    let (root, mut p) = fixture();
    let mut s = Session::new();
    p["format"] = json!("png");
    let planned = s.execute("figureexport", &p).unwrap();
    assert_eq!(planned["plan"]["jobs"].as_array().unwrap().len(), 4);
    p["action"] = json!("execute");
    p["approved_hash"] = planned["hash"].clone();
    let done = s.execute("figureexport", &p).unwrap();
    assert_eq!(done["receipt"]["outcome"], "Succeeded", "{done}");
    for name in ["A_2025_F.png", "A_2026_F.png", "B_2025_F.png", "B_2026_F.png"] {
        let bytes = std::fs::read(root.join("out").join(name)).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let image = cadcraft_io::raster::decode(&bytes).unwrap();
        assert!(image.width() > 0 && image.height() > 0);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exported_pdfs_contain_substituted_text() {
    pdf_font::register();
    let (root, mut p) = fixture();
    let mut d = Drawing::new_metric();
    d.text_styles.push(cadcraft_doc::TextStyle { name: "Synthetic".into(), font: "cadcraft-figure-test.ttf".into(), ..Default::default() });
    d.layouts.push(Layout::new("Figure", 1));
    let text = serde_json::from_value(json!({"insert":[10,10,0],"height":3,"style":"Synthetic","value":"{{Site}}{{Event}}{{FigureId}} {{Unknown}}"}))
        .unwrap();
    d.add(&Space::Paper("Figure".into()), Default::default(), EntityKind::Text(text)).unwrap();
    std::fs::write(root.join("source.dxf"), cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    std::fs::write(root.join("figure.json"), br#"{"figure_id":"A","source_drawing":"source.dxf","layout":"Figure"}"#).unwrap();
    p["sites"] = json!(["A", "B"]);
    p["events"] = json!(["A", "B"]);
    let mut s = Session::new();
    let planned = s.execute("figureexport", &p).unwrap();
    for job in planned["plan"]["jobs"].as_array().unwrap() {
        assert!(job["qa"].as_array().unwrap().iter().any(|q| q["code"] == "unresolved_placeholder"), "{job}");
    }
    p["action"] = json!("execute");
    p["approved_hash"] = planned["hash"].clone();
    let done = s.execute("figureexport", &p).unwrap();
    assert_eq!(done["receipt"]["outcome"], "Succeeded", "{done}");
    for (name, expected) in [("A_A_A.pdf", "AAA"), ("A_B_A.pdf", "ABA"), ("B_A_A.pdf", "BAA"), ("B_B_A.pdf", "BBA")] {
        let pdf = pdf_read::read(&std::fs::read(root.join("out").join(name)).unwrap());
        let page = &pdf.pages[0];
        let map: std::collections::BTreeMap<u32, String> =
            page.fonts.iter().flat_map(|f| pdf_read::to_unicode_map(f.to_unicode.as_deref().unwrap_or(""))).collect();
        let extracted: String = pdf_read::shown(&page.content_bytes)
            .iter()
            .flat_map(|codes| codes.as_chunks::<2>().0.iter())
            .filter_map(|code| map.get(&(u32::from(code[0]) << 8 | u32::from(code[1]))))
            .map(String::as_str)
            .collect();
        assert_eq!(extracted, expected, "{name}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn qa_retains_job_records_and_refuses_missing_layout_and_broken_xref() {
    let (root, p) = fixture();
    let mut s = Session::new();
    let source = root.join("source.dxf");
    let mut d = cadcraft_io::read(&std::fs::read(&source).unwrap(), "source.dxf").unwrap();
    d.layouts.clear();
    std::fs::write(&source, cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    let result = s.execute("figureexport", &p).unwrap();
    assert_eq!(result["plan"]["jobs"][0]["qa"][0]["code"], "layout_missing", "{result}");
    assert!(result["hash"].is_null());
    assert!(!root.join("out").exists());
    d.layouts.push(Layout::new("Figure", 1));
    let mut b = cadcraft_doc::Block::new("Missing");
    b.xref_path = Some("gone.dxf".into());
    d.blocks.insert("Missing".into(), std::sync::Arc::new(b));
    std::fs::write(&source, cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    let result = s.execute("figureexport", &p).unwrap();
    assert!(result["plan"]["jobs"][0]["qa"].as_array().unwrap().iter().any(|q| q["code"] == "broken_reference"), "{result}");
    assert!(!root.join("out").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn changed_source_invalidates_approval_before_any_output() {
    let (root, mut p) = fixture();
    let mut s = Session::new();
    let plan = s.execute("figureexport", &p).unwrap();
    let source = root.join("source.dxf");
    let mut d = cadcraft_io::read(&std::fs::read(&source).unwrap(), "source.dxf").unwrap();
    d.layers.push(Layer::new("Changed"));
    std::fs::write(&source, cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    p["action"] = json!("execute");
    p["approved_hash"] = plan["hash"].clone();
    let result = s.execute("figureexport", &p).unwrap();
    assert_eq!(result["receipt"]["failed_checks"][0]["code"], "stale_approval", "{result}");
    assert!(!root.join("out").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn required_xref_layers_are_checked_after_loading_the_reference() {
    let (root, p) = fixture();
    let mut reference = Drawing::new_metric();
    reference.layers.push(Layer::new("Contours"));
    reference
        .add(
            &Space::Model,
            cadcraft_doc::Common { layer: "Contours".into(), ..Default::default() },
            EntityKind::Line(cadcraft_doc::Line { a: cadcraft_geom::Vec3::ZERO, b: cadcraft_geom::Vec3::new(10., 10., 0.) }),
        )
        .unwrap();
    std::fs::write(root.join("ref.dxf"), cadcraft_io::write(&reference, "ref.dxf").unwrap()).unwrap();
    let source = root.join("source.dxf");
    let mut d = cadcraft_io::read(&std::fs::read(&source).unwrap(), "source.dxf").unwrap();
    let mut block = cadcraft_doc::Block::new("Ref");
    block.xref_path = Some("ref.dxf".into());
    d.blocks.insert("Ref".into(), std::sync::Arc::new(block));
    d.add(&Space::Model, Default::default(), EntityKind::Insert(cadcraft_doc::Insert { block: "Ref".into(), ..Default::default() })).unwrap();
    std::fs::write(&source, cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
    std::fs::write(root.join("figure.json"),serde_json::to_vec(&json!({"figure_id":"F","source_drawing":"source.dxf","layout":"Figure","required_layers":["Ref|Contours"],"visible_layers":["Ref|Contours"]})).unwrap()).unwrap();
    let result = Session::new().execute("figureexport", &p).unwrap();
    assert!(result["hash"].is_string(), "{result}");
    assert!(result["plan"]["jobs"][0]["qa"].as_array().unwrap().iter().all(|q| q["code"] != "layer_missing"), "{result}");
    std::fs::remove_dir_all(root).unwrap();
}
