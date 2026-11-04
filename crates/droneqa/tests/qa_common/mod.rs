//! Shared fixtures for the QA tests: the AIGIS3D `config/fixture.yaml` ruleset as JSON, the
//! fixture mission, the scenario overlay from `scenarios.json`, and what the synthetic v1 parser
//! reads from `valid.txt`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use cadcraft_droneqa::mission::{Mission, Ruleset};
use cadcraft_droneqa::qa::analyze;
use cadcraft_droneqa::{Finding, Outcome, ParseResult, QAMetrics};
use serde_json::{Value, json};

pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// The reference `config/fixture.yaml` ruleset.
pub fn fixture_ruleset_json() -> Value {
    json!({
        "version": "synthetic-fixture-1",
        "required_properties": ["mission_id", "mission_date", "aircraft", "sensor", "horizontal_crs", "horizontal_units", "vertical_datum", "vertical_units"],
        "accuracy": {"maximum_rmse_x_feet": 0.2, "maximum_rmse_y_feet": 0.3, "maximum_rmse_z_feet": 0.4},
        "tolerances": {
            "maximum_altitude_difference_feet": 10.0,
            "maximum_speed_difference_mph": 1.0,
            "maximum_overlap_difference_percent": 5.0,
            "maximum_image_count_difference": 5,
            "require_matching_flight_pattern": true
        },
        "required_deliverables": ["orthomosaic", "QA report"],
        "coordinates": {
            "horizontal_crs": "EPSG:26913", "horizontal_datum": "NAD83", "horizontal_units": "metre",
            "vertical_crs": "NAVD88 height", "vertical_datum": "NAVD88", "vertical_units": "foot", "geoid_model": "synthetic-geoid"
        },
        "control": {
            "minimum_gcp_count": 3, "minimum_checkpoint_count": 2, "require_independent_checkpoints": true,
            "maximum_control_accuracy_feet": 0.05, "maximum_checkpoint_accuracy_feet": 0.05
        },
        "allowed_correction_statuses": ["RTK_FIXED"],
        "require_correction_source": true,
        "required_attachment_patterns": ["quality_report\\.(txt|pdf)"],
        "processing": {
            "required_metrics": ["rmse_x_feet", "rmse_y_feet", "rmse_z_feet", "calibrated_images", "total_images", "processing_software", "processing_version"],
            "minimum_calibrated_percent": 85.0
        }
    })
}

pub fn fixture_ruleset() -> Ruleset {
    Ruleset::from_value(&fixture_ruleset_json()).unwrap()
}

pub fn mission_json() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/mission.json")).unwrap()).unwrap()
}

pub fn scenarios() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/scenarios.json")).unwrap()).unwrap()
}

/// `mission | changes`, as Python's dict union.
pub fn mission_with(changes: &Value) -> Mission {
    let mut m = mission_json();
    for (k, v) in changes.as_object().unwrap() {
        m[k] = v.clone();
    }
    Mission::from_value(&m).unwrap()
}

/// The ruleset a scenario runs under (reference `fixture_client.load_fixture`): the fixture
/// ruleset with the scenario's keys replaced and its `control` merged.
pub fn scenario_ruleset() -> Ruleset {
    let overlay = scenarios()["ruleset"].clone();
    let mut rules = fixture_ruleset_json();
    for (k, v) in overlay.as_object().unwrap() {
        if k == "control" {
            for (ck, cv) in v.as_object().unwrap() {
                rules["control"][ck] = cv.clone();
            }
        } else {
            rules[k] = v.clone();
        }
    }
    Ruleset::from_value(&rules).unwrap()
}

pub fn scenario_mission(name: &str) -> Mission {
    mission_with(&scenarios()["scenarios"][name])
}

/// What the synthetic v1 parser reads from `valid.txt` (independence is not declared there).
pub fn valid_report() -> ParseResult {
    ParseResult {
        profile: "synthetic-pix4d-v1".into(),
        metrics: QAMetrics {
            rmse_x_feet: Some(0.1),
            rmse_y_feet: Some(0.2),
            rmse_z_feet: Some(0.3),
            calibrated_images: Some(90),
            total_images: Some(100),
            processing_software: Some("Pix4D synthetic".into()),
            processing_version: Some("fixture-1".into()),
            ..QAMetrics::default()
        },
        report_evidence: None,
        diagnostics: Vec::new(),
    }
}

pub fn pilot() -> Vec<String> {
    vec!["pilot".into()]
}

pub fn by_rule(mission: &Mission, ruleset: &Ruleset, report: Option<&ParseResult>, sensitive: Option<&[String]>) -> BTreeMap<String, Finding> {
    analyze(mission, ruleset, report, sensitive).findings.into_iter().map(|f| (f.rule_id.clone(), f)).collect()
}

pub fn outcome(findings: &BTreeMap<String, Finding>, rule: &str) -> Outcome {
    findings.get(rule).unwrap_or_else(|| panic!("no finding {rule}")).outcome
}

pub fn observed_f64(f: &Finding) -> Option<f64> {
    f.observed.as_ref().and_then(Value::as_f64)
}
