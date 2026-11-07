use cadcraft_engine::Session;
use serde_json::json;

#[test]
fn converts_only_codes_and_refuses_existing_output() {
    let root = std::env::temp_dir().join(format!("pointconvert-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("points.csv");
    let map = root.join("map.csv");
    let output = root.join("points_converted.csv");
    let _ = std::fs::remove_file(&output);
    std::fs::write(&map, "from,to\nA,Tree\n").unwrap();
    std::fs::write(&input, "# keep this\r\n1, 02.00,A\r\n2,3,Z\nshort\n").unwrap();
    let p = json!({"inputs":[input],"map_path":map,"code_column":3});
    let mut s = Session::empty();
    let result = s.execute("pointconvert", &p).unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "# keep this\r\n1, 02.00,Tree\r\n2,3,Z\nshort\n");
    assert!(result.to_string().contains('Z'));
    assert!(s.execute("pointconvert", &p).is_err());
    std::fs::write(&input, "1,\"other,field\",A;Z\r\n").unwrap();
    let mut quoted = p.clone();
    quoted["suffix"] = json!("_quoted");
    quoted["code_separator"] = json!(";");
    s.execute("pointconvert", &quoted).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("points_quoted.csv")).unwrap(), "1,\"other,field\",Tree;Z\r\n");
    quoted["suffix"] = json!("_tabs");
    quoted["tab_output"] = json!(true);
    s.execute("pointconvert", &quoted).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("points_tabs.csv")).unwrap(), "1\tother,field\tTree;Z\r\n");
    std::fs::write(&input, "1,\"embedded\ttab\",A\n").unwrap();
    quoted["suffix"] = json!("_refused");
    assert!(s.execute("pointconvert", &quoted).is_err());
    assert!(!root.join("points_refused.csv").exists());
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    let relative = s.execute("pointconvert", &json!({"inputs":["points.csv"],"map_path":map,"code_column":3,"suffix":"_relative"}));
    std::env::set_current_dir(previous).unwrap();
    relative.unwrap();
    assert!(root.join("points_relative.csv").exists());
    std::fs::remove_dir_all(root).unwrap();
}
