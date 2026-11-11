//! Rich-editor serialization must preserve literal private-use Unicode beside opaque codes.
use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::mtext_edit::{CharFmt, Formatted};
use cadcraft_engine::{Input, Session};
use serde_json::json;

fn verify_rich_edit(command: &str) {
    let original = "A\u{e000}\\S1/2;\u{e001}B\u{e3e7}";
    let mut s = Session::new();
    let params = if command == "mtext" {
        json!({"at":[0,0], "text":original, "height":2.5, "width":100})
    } else {
        json!({"points":[[0,0],[20,20]], "text":original})
    };
    s.execute(command, &params).unwrap();
    let (handle, at) = s
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::MText(m) => Some((e.handle, m.insert.xy())),
            EntityKind::MLeader(m) => m.text.as_ref().map(|m| (e.handle, m.insert.xy())),
            _ => None,
        })
        .unwrap();
    s.cmdline("TEXTEDIT").unwrap();
    // The multileader uses the default 0.18-unit height; pick inside its first glyph.
    s.input(Input::Point(at + Vec2::new(0.02, -0.02))).unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter new text");
    let mt = s.editing_mtext().expect("literal Unicode remains rich-editable");
    let mut f = Formatted::from_contents_with_height(&mt.contents, Some(mt.height));
    assert!(f.replace_text(f.len()..f.len(), "!\u{e000}", &CharFmt::default()));
    let edited = format!("{original}!\u{e000}");
    assert_eq!(f.to_contents(), edited);
    s.input(Input::Text(f.to_contents())).unwrap();
    s.cancel();
    let contents = |s: &Session| match &s.doc().unwrap().entity(handle).unwrap().kind {
        EntityKind::MText(m) => m.contents.clone(),
        EntityKind::MLeader(m) => m.text.as_ref().unwrap().contents.clone(),
        _ => unreachable!(),
    };
    assert_eq!(contents(&s), edited);
    let reopened = cadcraft_io::read_dxf(
        &cadcraft_io::write_with(
            s.doc().unwrap(),
            "unicode.dxf",
            &cadcraft_io::SaveOptions { version: Some(cadcraft_io::Version::R2007), binary: false },
        )
        .unwrap(),
    )
    .unwrap();
    let saved = reopened.model.iter().find_map(|e| match &e.kind {
        EntityKind::MText(m) => Some(m.contents.as_str()),
        EntityKind::MLeader(m) => m.text.as_ref().map(|m| m.contents.as_str()),
        _ => None,
    });
    assert_eq!(saved, Some(edited.as_str()));
    s.undo().unwrap();
    assert_eq!(contents(&s), original);
    s.redo().unwrap();
    assert_eq!(contents(&s), edited);
}

#[test]
fn private_use_mtext_survives_rich_edit_save_and_undo() {
    verify_rich_edit("mtext");
}

#[test]
fn private_use_multileader_survives_rich_edit_save_and_undo() {
    verify_rich_edit("mleader");
}
