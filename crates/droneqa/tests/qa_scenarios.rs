//! The reference scenarios (AIGIS3D `tests/test_fixture_scenarios.py`): `pass`, `fail` and
//! `insufficient-data` give the reference outcome and findings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod qa_common;

use cadcraft_droneqa::Outcome;
use cadcraft_droneqa::qa::{analyze, summarize};
use qa_common::*;
use serde_json::json;

#[test]
fn pass_scenario_uses_qualified_radial_and_bare_ground_evidence() {
    let p = pilot();
    let analysis = analyze(&scenario_mission("pass"), &scenario_ruleset(), Some(&valid_report()), Some(&p));
    let f: std::collections::BTreeMap<_, _> = analysis.findings.iter().map(|f| (f.rule_id.as_str(), f)).collect();
    let bad: Vec<_> = analysis.findings.iter().filter(|f| !matches!(f.outcome, Outcome::Pass | Outcome::Info)).map(|f| &f.rule_id).collect();
    assert_eq!(analysis.outcome, Outcome::Pass, "not passing: {bad:?}");
    assert_eq!(observed_f64(f["ACCURACY.RMSE_HORIZONTAL"]), Some(0.25));
    assert_eq!(observed_f64(f["ACCURACY.RMSE_Z"]), Some(0.2));
    assert_eq!(f["ACCURACY.RMSE_HORIZONTAL"].requirement.as_deref(), Some("<= 0.25"));
    assert_eq!(f["ACCURACY.RMSE_Z"].requirement.as_deref(), Some("<= 0.25"));
    assert_eq!(analysis.ruleset_version, "synthetic-scenarios-1");
    assert!(summarize(&analysis).starts_with("PASS: All configured checks passed."));
}

#[test]
fn fail_scenario_has_exact_exceedances_and_actions() {
    let p = pilot();
    let analysis = analyze(&scenario_mission("fail"), &scenario_ruleset(), Some(&valid_report()), Some(&p));
    assert_eq!(analysis.outcome, Outcome::Fail);
    let f: std::collections::BTreeMap<_, _> = analysis.findings.iter().map(|f| (f.rule_id.as_str(), f)).collect();
    for (rule, expected) in [("ACCURACY.RMSE_HORIZONTAL", 0.5), ("ACCURACY.RMSE_Z", 0.4)] {
        let finding = f[rule];
        assert_eq!(finding.outcome, Outcome::Fail);
        assert_eq!(observed_f64(finding), Some(expected));
        assert_eq!(finding.requirement.as_deref(), Some("<= 0.25"));
        assert_eq!(finding.unit.as_deref(), Some("ft"));
        assert!(finding.source.as_deref().unwrap().contains("mission.rmse"));
        assert!(finding.next_action.as_deref().unwrap().contains("resolve"));
    }
    let brief = summarize(&analysis);
    assert!(brief.contains("ACCURACY.RMSE_HORIZONTAL") && brief.contains("ACCURACY.RMSE_Z"), "{brief}");
}

#[test]
fn insufficient_scenario_preserves_unknown_independence() {
    let p = pilot();
    let mission = scenario_mission("insufficient-data");
    assert_eq!((mission.rmse_x_feet, mission.rmse_y_feet, mission.rmse_z_feet), (Some(0.01), Some(0.01), Some(0.01)));
    assert_eq!((mission.checkpoint_count, mission.checkpoints_independent, mission.checkpoints_bare_ground), (None, None, None));
    let analysis = analyze(&mission, &scenario_ruleset(), Some(&valid_report()), Some(&p));
    assert_eq!(analysis.outcome, Outcome::InsufficientData);
    for rule in ["CONTROL.INDEPENDENCE", "ACCURACY.RMSE_HORIZONTAL", "ACCURACY.RMSE_Z"] {
        let f = analysis.findings.iter().find(|f| f.rule_id == rule).unwrap();
        assert_eq!(f.outcome, Outcome::InsufficientData, "{rule}");
        assert_eq!(f.observed, None, "{rule}");
        assert!(f.next_action.is_some(), "{rule}");
    }
}

#[test]
fn independent_scenario_still_requires_bare_ground_evidence() {
    for bare in [json!(null), json!(false)] {
        let mut changes = scenarios()["scenarios"]["pass"].clone();
        changes["checkpoints_bare_ground"] = bare;
        let f = by_rule(&mission_with(&changes), &scenario_ruleset(), Some(&valid_report()), Some(&pilot()));
        assert_eq!(outcome(&f, "CONTROL.INDEPENDENCE"), Outcome::Pass);
        assert_eq!(outcome(&f, "ACCURACY.RMSE_HORIZONTAL"), Outcome::Pass);
        assert_eq!(outcome(&f, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
    }
}

#[test]
fn selected_scenario_needs_each_independence_property() {
    for missing in ["checkpoint_count", "checkpoints_independent"] {
        let mut changes = scenarios()["scenarios"]["pass"].clone();
        changes[missing] = json!(null);
        let analysis = analyze(&mission_with(&changes), &scenario_ruleset(), Some(&valid_report()), Some(&pilot()));
        assert_eq!(analysis.outcome, Outcome::InsufficientData);
        let f = by_rule(&mission_with(&changes), &scenario_ruleset(), Some(&valid_report()), Some(&pilot()));
        assert_eq!(f["CONTROL.INDEPENDENCE"].observed, None);
        assert_eq!(outcome(&f, "ACCURACY.RMSE_HORIZONTAL"), Outcome::InsufficientData);
        assert_eq!(outcome(&f, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
    }
}

#[test]
fn real_parser_reads_valid_txt_as_the_scenario_fixture_assumes() {
    let bytes = std::fs::read(format!("{FIXTURES}/valid.txt")).unwrap();
    let parsed = cadcraft_droneqa::pix4d::parse(&bytes, "text/plain");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.metrics.checkpoints_independent, None);
    assert_eq!(parsed.profile, valid_report().profile);
    assert_eq!(parsed.metrics, valid_report().metrics);
    let p = pilot();
    for (name, expected) in [("pass", Outcome::Pass), ("fail", Outcome::Fail), ("insufficient-data", Outcome::InsufficientData)] {
        assert_eq!(analyze(&scenario_mission(name), &scenario_ruleset(), Some(&parsed), Some(&p)).outcome, expected, "{name}");
    }
}
