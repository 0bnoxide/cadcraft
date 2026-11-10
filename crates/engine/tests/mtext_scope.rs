//! Opaque MTEXT codes cannot lose their brace scope when TEXTEDIT saves an edit.
use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use serde_json::json;

fn verify_fallback(command: &str, original: &str) {
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
    s.input(Input::Point(at + Vec2::new(1.0, -0.05))).unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Enter new text");
    assert!(s.editing_mtext().is_none(), "unsupported scopes use command-line editing");
    assert_eq!(
        s.log.last().map(String::as_str),
        Some("This text cannot be fully represented by the editor; enter the new text at the command line.")
    );
    s.input(Input::Enter).unwrap();
    let current = |s: &Session| match &s.doc().unwrap().model.get(handle).unwrap().kind {
        EntityKind::MText(m) => m.contents.clone(),
        EntityKind::MLeader(m) => m.text.as_ref().unwrap().contents.clone(),
        _ => unreachable!(),
    };
    assert_eq!(current(&s), original);
    s.input(Input::Point(at + Vec2::new(1.0, -0.05))).unwrap();
    let edited = format!("{original}!");
    s.input(Input::Text(edited.clone())).unwrap();
    s.cancel();
    assert_eq!(current(&s), edited);
    let reopened = cadcraft_io::read_dxf(
        &cadcraft_io::write_with(
            s.doc().unwrap(),
            "scope.dxf",
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
    assert_eq!(current(&s), original);
}

#[test]
fn scoped_mtext_uses_lossless_fallback() {
    verify_fallback("mtext", r"{\W2;wide}normal");
}
#[test]
fn scoped_multileader_uses_lossless_fallback() {
    verify_fallback("mleader", r"{\W2;wide}normal");
}

#[test]
fn unicode_mtext_uses_lossless_fallback() {
    verify_fallback("mtext", r"A\U+00B0 B C;D");
}

#[test]
fn unicode_multileader_uses_lossless_fallback() {
    verify_fallback("mleader", r"A\U+00B0 B C;D");
}
