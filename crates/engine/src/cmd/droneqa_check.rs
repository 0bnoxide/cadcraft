//! DRONEQA: run the QA rules on a mission and a parsed report. Owner: Claude (see
//! `docs/agents/slice-3-droneqa.md`). Parser API (`pix4d::parse`) and the redaction boundary were
//! agreed with Codex by Paseo message (see `docs/agents/slice-3-notes-claude.md`).

use cadcraft_droneqa::mission::{Mission, Ruleset, check_field_names, check_shape};
use cadcraft_droneqa::qa::{analyze, summarize, validate_report};
use cadcraft_droneqa::{ParseResult, pix4d};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad};
use crate::{Result, Session};

const ID: &str = "droneqa";
/// Default private mission fields (the reference configuration's default).
const DEFAULT_SENSITIVE: &[&str] = &["pilot"];

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(ID, "Drone Survey QA", run).enabled(always).noundo().params(
        "{mission: canonical mission object, ruleset: ruleset object, report?: ParseResult (from pix4dimport) | \
                 report_text?: report text + content_type?: (default text/plain), sensitive_fields?: [mission field] (default [\"pilot\"])} \
                 → {analysis: {findings, outcome, risk_level, ruleset_version}, summary}; findings from a sensitive field carry [REDACTED]",
    )]
}

fn run(_: &mut Session, p: &Value) -> Result<Value> {
    let mission = Mission::from_value(p.get("mission").ok_or_else(|| bad(ID, "`mission` (object) is required"))?).map_err(|e| bad(ID, e))?;
    let ruleset = Ruleset::from_value(p.get("ruleset").ok_or_else(|| bad(ID, "`ruleset` (object) is required"))?).map_err(|e| bad(ID, e))?;
    let sensitive: Vec<String> = match p.get("sensitive_fields") {
        None | Some(Value::Null) => DEFAULT_SENSITIVE.iter().map(|s| s.to_string()).collect(),
        Some(v) => v
            .as_array()
            .filter(|a| a.len() <= 1000)
            .and_then(|a| a.iter().map(|x| x.as_str().map(str::to_string)).collect::<Option<Vec<_>>>())
            .ok_or_else(|| bad(ID, "`sensitive_fields` must be an array of mission field names"))?,
    };
    check_field_names("sensitive_fields", &sensitive).map_err(|e| bad(ID, e))?;
    let text = opt_str(p, "report_text")?;
    let content_type = opt_str(p, "content_type")?.unwrap_or("text/plain");
    let report: Option<ParseResult> = match (p.get("report").filter(|v| !v.is_null()), text) {
        (Some(_), Some(_)) => return Err(bad(ID, "give `report` or `report_text`, not both")),
        (Some(r), None) => {
            check_shape("report", r).map_err(|e| bad(ID, e))?;
            // The serde error would quote the rejected input, so it is not passed on.
            Some(serde_json::from_value(r.clone()).map_err(|_| bad(ID, "`report` is not a parse result"))?)
        }
        (None, Some(text)) => Some(pix4d::parse(text.as_bytes(), content_type)),
        (None, None) => None,
    };
    if let Some(r) = &report {
        validate_report(r).map_err(|e| bad(ID, e))?;
    }
    let analysis = analyze(&mission, &ruleset, report.as_ref(), Some(&sensitive));
    let summary = summarize(&analysis);
    Ok(json!({ "analysis": analysis, "summary": summary }))
}

/// An optional string parameter; present but not a string is an error, never ignored.
fn opt_str<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(bad(ID, format!("`{key}` must be a string"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../droneqa/tests/fixtures");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
    }

    /// The reference scenario inputs: mission.json with the scenario's overrides, and the fixture
    /// ruleset with the scenario ruleset overlaid (control merged).
    fn scenario(name: &str) -> (Value, Value) {
        let scenarios: Value = serde_json::from_str(&fixture("scenarios.json")).unwrap();
        let mut mission: Value = serde_json::from_str(&fixture("mission.json")).unwrap();
        for (k, v) in scenarios["scenarios"][name].as_object().unwrap() {
            mission[k] = v.clone();
        }
        let ruleset = json!({
            "version": scenarios["ruleset"]["version"],
            "required_properties": ["mission_id", "mission_date", "aircraft", "sensor", "horizontal_crs", "horizontal_units", "vertical_datum", "vertical_units"],
            "accuracy": scenarios["ruleset"]["accuracy"],
            "tolerances": {"maximum_altitude_difference_feet": 10.0, "maximum_speed_difference_mph": 1.0, "maximum_overlap_difference_percent": 5.0,
                           "maximum_image_count_difference": 5, "require_matching_flight_pattern": true},
            "required_deliverables": ["orthomosaic", "QA report"],
            "coordinates": {"horizontal_crs": "EPSG:26913", "horizontal_datum": "NAD83", "horizontal_units": "metre", "vertical_crs": "NAVD88 height",
                            "vertical_datum": "NAVD88", "vertical_units": "foot", "geoid_model": "synthetic-geoid"},
            "control": {"minimum_gcp_count": 3, "minimum_checkpoint_count": 2, "require_independent_checkpoints": true,
                        "maximum_control_accuracy_feet": 0.05, "maximum_checkpoint_accuracy_feet": 0.05, "require_bare_ground_checkpoints": true},
            "allowed_correction_statuses": ["RTK_FIXED"],
            "require_correction_source": true,
            "required_attachment_patterns": ["quality_report\\.(txt|pdf)"],
            "processing": {"required_metrics": ["rmse_x_feet", "rmse_y_feet", "rmse_z_feet", "calibrated_images", "total_images", "processing_software", "processing_version"],
                           "minimum_calibrated_percent": 85.0}
        });
        (mission, ruleset)
    }

    #[test]
    fn reference_scenarios_give_the_reference_outcome_through_the_command() {
        let mut s = Session::new();
        for (name, expected) in [("pass", "PASS"), ("fail", "FAIL"), ("insufficient-data", "INSUFFICIENT_DATA")] {
            let (mission, ruleset) = scenario(name);
            let v = s.execute(ID, &json!({"mission": mission, "ruleset": ruleset, "report_text": fixture("valid.txt")})).unwrap();
            assert_eq!(v["analysis"]["outcome"], expected, "{name}: {v}");
            assert_eq!(v["analysis"]["ruleset_version"], "synthetic-scenarios-1");
            assert!(v["summary"].as_str().unwrap().starts_with(expected), "{name}");
        }
    }

    #[test]
    fn parse_result_from_pix4dimport_is_accepted_and_pdf_is_not_supported_yet() {
        let mut s = Session::new();
        let (mission, ruleset) = scenario("pass");
        let report = serde_json::to_value(pix4d::parse(fixture("valid.txt").as_bytes(), "text/plain")).unwrap();
        let v = s.execute(ID, &json!({"mission": mission, "ruleset": ruleset, "report": report})).unwrap();
        assert_eq!(v["analysis"]["outcome"], "PASS");
        let v =
            s.execute(ID, &json!({"mission": mission, "ruleset": ruleset, "report_text": "%PDF-1.7", "content_type": "application/pdf"})).unwrap();
        let rule = v["analysis"]["findings"].as_array().unwrap().iter().find(|f| f["rule_id"] == "PROCESSING.REPORT").unwrap().clone();
        assert_eq!(rule["outcome"], "INSUFFICIENT_DATA");
    }

    #[test]
    fn sensitive_fields_default_to_pilot_and_redact_findings() {
        let mut s = Session::new();
        let (mut mission, ruleset) = scenario("fail");
        mission["pilot"] = json!("Synthetic Operator Name");
        let v = s.execute(ID, &json!({"mission": mission, "ruleset": ruleset})).unwrap();
        assert!(!v.to_string().contains("Synthetic Operator Name"));
        let v = s.execute(ID, &json!({"mission": mission, "ruleset": ruleset, "sensitive_fields": ["rmse_z_feet"]})).unwrap();
        let z = v["analysis"]["findings"].as_array().unwrap().iter().find(|f| f["rule_id"] == "ACCURACY.RMSE_Z").unwrap().clone();
        assert_eq!(z["observed"], "[REDACTED]");
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let mut s = Session::new();
        let (mission, ruleset) = scenario("pass");
        for p in [
            json!({}),
            json!({"mission": mission}),
            json!({"mission": 1, "ruleset": ruleset}),
            json!({"mission": mission, "ruleset": {"version": "v"}}),
            json!({"mission": mission, "ruleset": ruleset, "sensitive_fields": ["not_a_field"]}),
            json!({"mission": mission, "ruleset": ruleset, "sensitive_fields": "pilot"}),
            json!({"mission": mission, "ruleset": ruleset, "report": {"profile": 1}}),
            json!({"mission": mission, "ruleset": ruleset, "report": {}, "report_text": "x"}),
            json!({"mission": mission, "ruleset": ruleset, "report_text": 7}),
            json!({"mission": mission, "ruleset": ruleset, "report_text": "x", "content_type": ["text/plain"]}),
        ] {
            assert!(s.execute(ID, &p).is_err(), "accepted {p}");
        }
    }

    #[test]
    fn errors_never_echo_rejected_values() {
        let mut s = Session::new();
        let (mission, ruleset) = scenario("pass");
        let secret = "token=SYNTHETIC_PRIVATE_31337";
        let mut bad_mission = mission.clone();
        bad_mission["rmse_x_feet"] = json!(secret);
        for p in [
            json!({"mission": bad_mission, "ruleset": ruleset}),
            json!({"mission": mission, "ruleset": ruleset, "sensitive_fields": [secret]}),
            json!({"mission": mission, "ruleset": ruleset, "report": {"profile": secret, "metrics": secret}}),
            json!({"mission": mission, "ruleset": ruleset, "report": {"profile": "p", "metrics": {"rmse_z_feet": -1.0}, "report_evidence": null, "diagnostics": []}}),
        ] {
            let e = s.execute(ID, &p).unwrap_err().to_string();
            assert!(!e.contains("SYNTHETIC_PRIVATE"), "{e}");
        }
    }
}
