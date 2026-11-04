//! Mission and ruleset boundaries, ported from AIGIS3D `tests/test_domain.py` (normalisation and
//! strict validation) and `test_accuracy_profile.py` (strict limits).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod qa_common;

use std::collections::BTreeMap;

use cadcraft_droneqa::mission::{Attachment, Mission, Ruleset, check_field_names, normalize_mission};
use qa_common::*;
use serde_json::{Map, Value, json};

const GID: &str = "11111111-1111-4111-8111-111111111111";

fn attrs(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn mapping(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn fixture_mission_reads_and_keeps_its_fields() {
    let m = mission_with(&json!({}));
    assert_eq!(m.mission_id.as_deref(), Some("SYNTHETIC-001"));
    assert_eq!(m.attachments.len(), 1);
    assert_eq!(m.attachments[0].content_type, "text/plain");
    assert_eq!(m.present_deliverables, ["orthomosaic", "QA report"]);
}

#[test]
fn mission_rejects_invalid_boundary_values() {
    for change in [
        json!({"extra": 1}),
        json!({"global_id": "invalid"}),
        json!({"gcp_count": true}),
        json!({"gcp_count": -1}),
        json!({"rmse_x_feet": -0.1}),
        json!({"front_overlap_percent": 101.0}),
        json!({"course_angle_degrees": 360.0}),
        json!({"terrain_follow": "yes"}),
        json!({"mission_date": "12/09/2026"}),
        json!({"present_deliverables": "[\"orthomosaic\"]"}),
        json!({"present_deliverables": ["   "]}),
        json!({"present_deliverables": [1]}),
        json!({"present_deliverables": ["x".repeat(2001)]}),
        json!({"present_deliverables": null}),
        json!({"pilot": "x".repeat(2001)}),
        json!({"attachments": [{"id": 1, "name": "   ", "size": 0}]}),
        json!({"attachments": [{"id": 1, "name": "a", "size": 0, "sha256": "ABC"}]}),
    ] {
        let mut m = json!({"global_id": GID});
        for (k, v) in change.as_object().unwrap() {
            m[k] = v.clone();
        }
        assert!(Mission::from_value(&m).is_err(), "accepted {change}");
    }
    assert!(Mission::from_value(&json!([])).is_err());
    assert!(Mission::from_value(&json!({})).is_err());
}

#[test]
fn canonical_text_is_trimmed_and_uuid_normalised() {
    let m = Mission::from_value(&json!({"global_id": "{11111111-1111-4111-8111-11111111111A}", "mission_id": "  synthetic  "})).unwrap();
    assert_eq!(m.mission_id.as_deref(), Some("synthetic"));
    assert_eq!(m.global_id, "11111111-1111-4111-8111-11111111111a");
}

#[test]
fn normalization_handles_epoch_dates_coded_values_and_rejects_ambiguous_input() {
    let map = mapping(&[("mission_date", "Date"), ("terrain_follow", "Terrain"), ("gcp_count", "GCP")]);
    let a = attrs(json!({"GlobalID": "{11111111-1111-4111-8111-111111111111}", "Date": 0, "Terrain": 1, "GCP": 0}));
    let domains: BTreeMap<String, Map<String, Value>> = [("terrain_follow".to_string(), attrs(json!({"1": true})))].into();
    let m = normalize_mission(&a, &map, Vec::new(), Some(&domains), "GlobalID").unwrap();
    assert_eq!(m.mission_date.as_deref(), Some("1970-01-01T00:00:00+00:00"));
    assert_eq!(m.terrain_follow, Some(true));
    assert_eq!(m.gcp_count, Some(0));
    assert!(normalize_mission(&a, &map, Vec::new(), None, "GlobalID").is_err());
    let unknown: BTreeMap<String, Map<String, Value>> = [("terrain_follow".to_string(), Map::new())].into();
    let e = normalize_mission(&a, &map, Vec::new(), Some(&unknown), "GlobalID").unwrap_err();
    assert!(e.contains("unknown configured coded-domain value"));
    // Fractional and hostile epoch values.
    let m = normalize_mission(&attrs(json!({"GlobalID": GID, "Date": 1500.5})), &mapping(&[("mission_date", "Date")]), Vec::new(), None, "GlobalID")
        .unwrap();
    assert_eq!(m.mission_date.as_deref(), Some("1970-01-01T00:00:01.500500+00:00"));
    assert!(
        normalize_mission(&attrs(json!({"GlobalID": GID, "Date": 1e300})), &mapping(&[("mission_date", "Date")]), Vec::new(), None, "GlobalID")
            .is_err()
    );
}

#[test]
fn normalization_accepts_deliverables_json_arrays_and_native_lists() {
    for field in ["present_deliverables", "intended_deliverables"] {
        let cases = [
            (json!("[\"orthomosaic\", \"point cloud\"]"), vec!["orthomosaic", "point cloud"]),
            (json!(" \n [\"  orthomosaic  \", \"caf\\u00e9\", \"quoted \\\"name\\\"\"] \t "), vec!["orthomosaic", "café", "quoted \"name\""]),
            (json!("[]"), vec![]),
            (json!(["orthomosaic", "point cloud"]), vec!["orthomosaic", "point cloud"]),
            (json!([]), vec![]),
        ];
        for (value, expected) in cases {
            let a = attrs(json!({"GlobalID": GID, "Outputs": value}));
            let m = normalize_mission(&a, &mapping(&[(field, "Outputs")]), Vec::new(), None, "GlobalID").unwrap();
            let got = if field == "present_deliverables" { m.present_deliverables } else { m.intended_deliverables };
            assert_eq!(got, expected);
        }
    }
}

#[test]
fn normalization_rejects_malformed_or_misshapen_deliverables() {
    let bad = [
        json!("[\"orthomosaic\","),
        json!("orthomosaic"),
        json!("[\"a\",]"),
        json!("["),
        json!(""),
        json!(" \t\n "),
        json!(null),
        json!("\"orthomosaic\""),
        json!("{}"),
        json!("1"),
        json!("[\"orthomosaic\", 1]"),
        json!("[null]"),
        json!("[\"   \"]"),
        json!([1]),
        json!([true]),
        json!([{}]),
        json!([[]]),
        json!(["   "]),
        json!(["x".repeat(2001)]),
    ];
    for field in ["present_deliverables", "intended_deliverables"] {
        for value in &bad {
            let a = attrs(json!({"GlobalID": GID, "Outputs": value}));
            assert!(normalize_mission(&a, &mapping(&[(field, "Outputs")]), Vec::new(), None, "GlobalID").is_err(), "{field} accepted {value}");
        }
        // Missing attribute under a mapping is null, which a list rejects; no mapping is an empty list.
        assert!(normalize_mission(&attrs(json!({"GlobalID": GID})), &mapping(&[(field, "Outputs")]), Vec::new(), None, "GlobalID").is_err());
        assert!(normalize_mission(&attrs(json!({"GlobalID": GID})), &BTreeMap::new(), Vec::new(), None, "GlobalID").is_ok());
    }
}

#[test]
fn normalization_decodes_deliverables_after_coded_domain_lookup() {
    let domains: BTreeMap<String, Map<String, Value>> = [("present_deliverables".to_string(), attrs(json!({"1": "[\"orthomosaic\"]"})))].into();
    let a = attrs(json!({"GlobalID": GID, "Outputs": 1}));
    let m = normalize_mission(&a, &mapping(&[("present_deliverables", "Outputs")]), Vec::new(), Some(&domains), "GlobalID").unwrap();
    assert_eq!(m.present_deliverables, ["orthomosaic"]);
}

#[test]
fn normalization_maps_the_complete_fixture_mission() {
    let canonical = mission_json();
    let names: Vec<String> = canonical.as_object().unwrap().keys().filter(|k| *k != "global_id" && *k != "attachments").cloned().collect();
    let map: BTreeMap<String, String> = names.iter().map(|n| (n.clone(), format!("F_{n}"))).collect();
    let mut a: Map<String, Value> = names.iter().map(|n| (format!("F_{n}"), canonical[n].clone())).collect();
    a.insert("GlobalID".into(), canonical["global_id"].clone());
    let attachments: Vec<Attachment> = serde_json::from_value(canonical["attachments"].clone()).unwrap();
    let m = normalize_mission(&a, &map, attachments, None, "GlobalID").unwrap();
    assert_eq!(m, mission_with(&json!({})));
}

#[test]
fn ruleset_reads_the_fixture_and_rejects_invalid_configuration() {
    assert_eq!(fixture_ruleset().version, "synthetic-fixture-1");
    let edits: [(&str, Value); 13] = [
        ("/version", json!("  ")),
        ("/required_properties", json!(["unknown_property"])),
        ("/accuracy/maximum_rmse_x_feet", json!(-1)),
        ("/accuracy/maximum_rmse_horizontal_feet", json!(-0.1)),
        ("/accuracy/maximum_rmse_horizontal_feet", json!(true)),
        ("/accuracy/maximum_rmse_horizontal_feet", json!("0.25")),
        ("/accuracy/extra", json!(1)),
        ("/coordinates/unknown_crs", json!("x")),
        ("/control/require_bare_ground_checkpoints", json!("yes")),
        ("/processing/required_metrics", json!(["gsd_cm"])),
        ("/processing/minimum_calibrated_percent", json!(100.5)),
        ("/required_attachment_patterns", json!(["(unclosed"])),
        ("/tolerances/maximum_image_count_difference", json!(-5)),
    ];
    for (path, value) in edits {
        let mut r = fixture_ruleset_json();
        let (parent, key) = path.rsplit_once('/').unwrap();
        let target = if parent.is_empty() { &mut r } else { r.pointer_mut(parent).unwrap() };
        target[key] = value.clone();
        assert!(Ruleset::from_value(&r).is_err(), "accepted {path} = {value}");
    }
    assert!(Ruleset::from_json("{\"version\": \"v\"}").is_err(), "required_properties is required");
    assert!(Ruleset::from_json("not json").is_err());
}

#[test]
fn unsupported_python_regex_features_are_rejected_not_reinterpreted() {
    // Rust `regex` has no lookaround or backreferences; such patterns are configuration errors.
    for p in ["quality(?=_report)", "(?<!x)report", "(a)\\1", "x".repeat(1001).as_str()] {
        let r = json!({"version": "v", "required_properties": [], "required_attachment_patterns": [p]});
        assert!(Ruleset::from_value(&r).is_err(), "accepted {p}");
    }
}

#[test]
fn sensitive_field_names_must_be_canonical() {
    assert!(check_field_names("sensitive_fields", &["pilot".into(), "rmse_x_feet".into()]).is_ok());
    assert!(check_field_names("sensitive_fields", &["Pilot".into()]).is_err());
    assert!(Mission::field_names().contains(&"attachments".to_string()));
}
