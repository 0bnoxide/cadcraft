use cadcraft_proposal::paths::{absolute, collisions, component, relative};

#[test]
fn portable_names_and_paths() {
    for name in ["", ".", "..", "CON", "nul.dxf", "COM¹", "LPT9.txt", "a.", "a ", "a/b", "a:b", "a\n"] {
        assert!(component(name).is_err(), "{name:?}");
    }
    assert!(component("École 01").is_ok());
    assert!(component(&"x".repeat(256)).is_err());
    assert_eq!(relative("Models\\base.dxf").ok().as_deref(), Some("Models/base.dxf"));
    for path in ["/root", "C:/root", "a/../b", "a//b"] {
        assert!(relative(path).is_err());
    }
    for path in ["/projects", "C:/projects", "//server/share/projects"] {
        assert!(absolute(path).is_ok(), "{path}");
    }
    assert!(absolute("projects").is_err());
}

#[test]
fn collisions_include_unicode_and_file_parents() {
    for paths in [
        vec![("Models".into(), false), ("models".into(), false)],
        vec![("École".into(), true), ("école".into(), true)],
        vec![("a".into(), true), ("A/b".into(), true)],
    ] {
        assert!(!collisions(&paths).is_empty());
    }
    assert!(collisions(&[("a".into(), false), ("a/b".into(), true)]).is_empty());
}

#[test]
fn extended_path_limit_counts_utf16_units() {
    let mut boundary = vec!["x".repeat(255); 128].join("/");
    assert_eq!(boundary.encode_utf16().count(), 32767);
    assert!(relative(&boundary).is_err());
    boundary.pop();
    assert!(relative(&boundary).is_ok());
    assert!(absolute(&format!("/{boundary}")).is_err());
    boundary.pop();
    assert!(absolute(&format!("/{boundary}")).is_ok());
    assert!(component(&"😀".repeat(128)).is_err());
    assert!(component(&"😀".repeat(127)).is_ok());
}
