mod common;
use cadcraft_proposal::manifest::Manifest;
use common::fixture;

#[test]
fn version_two_settings_and_source_roles_are_closed_and_version_one_still_works() {
    let mut m = fixture();
    assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_ok());
    m["version"] = 2.into();
    m["source_roles"] = serde_json::json!(["survey", "record_drawing", "gis_export", "imagery", "other"]);
    m["drawing_settings"] = serde_json::json!({"headers":{"INSUNITS":6,"LUPREC":3},"layers":[{"name":"Survey","color":3}],"text_styles":[{"name":"Notes","height":2.5}],"dimension_styles":[{"name":"Dimensions","decimals":3}],"annotation_scales":[{"name":"1:50","paper_units":1,"drawing_units":50}]});
    let valid = serde_json::to_vec(&m).unwrap();
    assert!(Manifest::parse(&valid).is_ok(), "v2 must accept its new fields");
    for field in ["coordinate_system", "invented"] {
        let mut bad = m.clone();
        bad["drawing_settings"][field] = "no".into();
        assert!(Manifest::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for change in ["role", "location", "units", "style", "scale"] {
        let mut bad = m.clone();
        match change {
            "role" => bad["source_roles"][0] = "unknown".into(),
            "location" => bad["source_location"] = "../escape".into(),
            "units" => bad["drawing_settings"]["headers"]["INSUNITS"] = 25.into(),
            "style" => bad["drawing_settings"]["text_styles"][0]["invented"] = true.into(),
            "scale" => bad["drawing_settings"]["annotation_scales"][0]["paper_units"] = 0.into(),
            _ => unreachable!(),
        }
        assert!(Manifest::parse(&serde_json::to_vec(&bad).unwrap()).is_err(), "{change}");
    }
}

#[test]
fn malformed_and_ambiguous_manifests_fail_closed() {
    for bytes in [b"{".as_slice(), b"[]", b"{\"version\":1,\"version\":2}", b"{\"version\":999}", b"\xff"] {
        assert!(Manifest::parse(bytes).is_err());
    }
}

#[test]
fn complete_manifest_and_mutations() {
    let manifest = fixture();
    assert!(Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).is_ok());
    for change in ["unknown", "missing", "role", "path", "geometry", "xref", "mapping", "duplicate"] {
        let mut m = manifest.clone();
        match change {
            "unknown" => m["invented"] = true.into(),
            "missing" => {
                m.as_object_mut().unwrap().remove("model_template");
            }
            "role" => m["models"][1]["role"] = "BaseModel".into(),
            "path" => m["models"][0]["path"] = "../escaped.dxf".into(),
            "geometry" => m["profiles"][0]["printable_area"]["width"] = (-1).into(),
            "xref" => m["xrefs"][0]["mode"] = "Attach".into(),
            "mapping" => m["property_mappings"][0]["input"] = "unknown".into(),
            "duplicate" => m["models"][1]["path"] = "models/base.DXF".into(),
            _ => {}
        }
        assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_err(), "{change}");
    }
    let json = serde_json::to_string(&manifest).unwrap().replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
    assert!(Manifest::parse(json.as_bytes()).is_err());
}

#[test]
fn template_paths_and_reference_validation_matrix() {
    let original = fixture();
    for name in ["model.dxf", std::env::temp_dir().join("model.dwg").to_str().unwrap()] {
        let mut m = original.clone();
        m["model_template"] = name.into();
        assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_ok());
    }
    for change in [
        "year",
        "profile",
        "number",
        "support",
        "unknown_mapping",
        "duplicate_mapping",
        "attribute",
        "placeholder",
        "rectangle",
        "cycle",
        "missing_edge",
        "duplicate_edge",
        "zero_scale",
        "folder",
        "file_parent",
    ] {
        let mut m = original.clone();
        match change {
            "year" => {
                let v = m["base_roots"][0].clone();
                m["base_roots"].as_array_mut().unwrap().push(v);
            }
            "profile" => {
                let v = m["profiles"][0].clone();
                m["profiles"].as_array_mut().unwrap().push(v);
            }
            "number" => m["sheets"][1]["number"] = m["sheets"][0]["number"].clone(),
            "support" => {
                m["support_records"].as_array_mut().unwrap().pop();
            }
            "unknown_mapping" => m["property_mappings"][0]["input"] = "Unknown".into(),
            "duplicate_mapping" => {
                let v = m["property_mappings"][0].clone();
                m["property_mappings"].as_array_mut().unwrap().push(v);
            }
            "attribute" => m["property_mappings"][0]["title_block_attribute"] = " ".into(),
            "placeholder" => {
                m["profiles"][0]["placeholders"].as_array_mut().unwrap().pop();
            }
            "rectangle" => m["profiles"][0]["placeholders"][0]["rectangle"]["width"] = 999.into(),
            "cycle" => m["xrefs"][0]["host_role"] = "BaseModel".into(),
            "missing_edge" => {
                m["xrefs"].as_array_mut().unwrap().pop();
            }
            "duplicate_edge" => m["xrefs"][1] = m["xrefs"][0].clone(),
            "zero_scale" => m["xrefs"][0]["scale"] = 0.into(),
            "folder" => m["folders"].as_array_mut().unwrap().push("models".into()),
            "file_parent" => m["folders"].as_array_mut().unwrap().push("Models/base.dxf/child".into()),
            _ => {}
        }
        assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_err(), "{change}");
    }
}
