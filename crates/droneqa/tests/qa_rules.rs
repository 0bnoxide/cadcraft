//! QA rule behaviour, ported from AIGIS3D `tests/test_findings.py`, `test_accuracy_profile.py` and
//! the `analyze` cases in `test_domain.py`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod qa_common;

use cadcraft_droneqa::mission::{Mission, Ruleset};
use cadcraft_droneqa::qa::{analyze, summarize};
use cadcraft_droneqa::{Analysis, Finding, Outcome, ParseResult, Severity};
use qa_common::*;
use serde_json::{Value, json};

const GID: &str = "11111111-1111-4111-8111-111111111111";
const NONE: &[String] = &[];

/// `test_findings.mission`: independent, bare-ground checkpoints with a radial failure.
fn mission(changes: Value) -> Mission {
    let mut m = json!({
        "global_id": GID, "checkpoint_count": 3, "checkpoints_independent": true, "checkpoints_bare_ground": true,
        "rmse_x_feet": 0.2, "rmse_y_feet": 0.2, "rmse_z_feet": 0.1
    });
    for (k, v) in changes.as_object().unwrap() {
        m[k] = v.clone();
    }
    Mission::from_value(&m).unwrap()
}

/// `test_findings.rules`, with keys replaced by `changes`.
fn rules(changes: Value) -> Ruleset {
    let mut r = json!({
        "version": "synthetic-actionable-v1", "required_properties": ["global_id"],
        "accuracy": {"maximum_rmse_horizontal_feet": 0.25, "maximum_rmse_z_feet": 0.25}
    });
    for (k, v) in changes.as_object().unwrap() {
        r[k] = v.clone();
    }
    Ruleset::from_value(&r).unwrap()
}

fn report(metrics: Value) -> ParseResult {
    ParseResult { profile: "synthetic".into(), metrics: serde_json::from_value(metrics).unwrap(), ..ParseResult::default() }
}

#[test]
fn radial_failure_shows_measurement_limit_units_and_qualified_source() {
    let f = &by_rule(&mission(json!({})), &rules(json!({})), None, Some(NONE))["ACCURACY.RMSE_HORIZONTAL"];
    assert_eq!(f.outcome, Outcome::Fail);
    assert!((observed_f64(f).unwrap() - 0.282842712474619).abs() < 1e-12);
    assert_eq!(f.requirement.as_deref(), Some("<= 0.25"));
    assert_eq!(f.unit.as_deref(), Some("ft"));
    let source = f.source.as_deref().unwrap();
    assert!(source.contains("mission.rmse_x_feet") && source.contains("mission.rmse_y_feet"));
    assert!(f.next_action.as_deref().unwrap().to_lowercase().contains("checkpoint"));
}

#[test]
fn checkpoint_independence_observation_distinguishes_unknown_and_negative() {
    let n = Value::Null;
    let cases = [
        (n.clone(), n.clone(), None, Outcome::InsufficientData),
        (n.clone(), json!(false), None, Outcome::InsufficientData),
        (n.clone(), json!(true), None, Outcome::InsufficientData),
        (json!(0), n.clone(), None, Outcome::InsufficientData),
        (json!(0), json!(false), Some(false), Outcome::InsufficientData),
        (json!(0), json!(true), Some(false), Outcome::InsufficientData),
        (json!(3), n.clone(), None, Outcome::InsufficientData),
        (json!(3), json!(false), Some(false), Outcome::InsufficientData),
        (json!(3), json!(true), Some(true), Outcome::Pass),
    ];
    for (count, independence, observed, expected) in cases {
        let f = by_rule(&mission(json!({"checkpoint_count": count, "checkpoints_independent": independence})), &rules(json!({})), None, Some(NONE));
        let ind = &f["CONTROL.INDEPENDENCE"];
        assert_eq!(ind.observed, observed.map(Value::Bool), "{count} {independence}");
        assert_eq!(ind.outcome, expected);
        if expected != Outcome::Pass {
            assert_eq!(outcome(&f, "ACCURACY.RMSE_HORIZONTAL"), Outcome::InsufficientData);
            assert_eq!(outcome(&f, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
        }
    }
}

#[test]
fn private_checkpoint_observation_keeps_unknown_distinct_from_redacted() {
    let privates: [Option<Vec<String>>; 3] = [None, Some(vec!["checkpoint_count".into()]), Some(vec!["checkpoints_independent".into()])];
    for private in &privates {
        for (independence, observed) in [(Value::Null, None), (json!(false), Some(json!("[REDACTED]"))), (json!(true), Some(json!("[REDACTED]")))] {
            let f = by_rule(&mission(json!({"checkpoints_independent": independence})), &rules(json!({})), None, private.as_deref());
            let ind = &f["CONTROL.INDEPENDENCE"];
            assert_eq!(ind.observed, observed);
            assert_eq!(ind.requirement.as_deref(), Some("[REDACTED]"));
        }
    }
}

#[test]
fn missing_criterion_and_missing_measurement_have_different_actions() {
    let unconfigured = by_rule(&mission(json!({"gcp_count": 3})), &rules(json!({})), None, Some(NONE)).remove("CONTROL.GCP_COUNT").unwrap();
    let missing = by_rule(&mission(json!({"gcp_count": null})), &rules(json!({"control": {"minimum_gcp_count": 3}})), None, Some(NONE))
        .remove("CONTROL.GCP_COUNT")
        .unwrap();
    assert_eq!(unconfigured.outcome, Outcome::InsufficientData);
    assert_eq!(missing.outcome, Outcome::InsufficientData);
    assert_eq!(unconfigured.observed, Some(json!(3)));
    assert!(unconfigured.next_action.as_deref().unwrap().starts_with("Configure"));
    assert_eq!(missing.observed, None);
    assert_eq!(missing.requirement.as_deref(), Some(">= 3"));
    assert_ne!(missing.next_action, unconfigured.next_action);
    assert!(missing.next_action.as_deref().unwrap().contains("gcp_count"));
}

#[test]
fn report_accuracy_details_require_qualified_same_source_components() {
    let metrics =
        json!({"checkpoints_independent": true, "checkpoints_bare_ground": true, "rmse_x_feet": 0.15, "rmse_y_feet": 0.2, "rmse_z_feet": 0.1});
    let full = report(metrics.clone());
    let f =
        &by_rule(&mission(json!({"rmse_x_feet": null, "rmse_y_feet": null})), &rules(json!({})), Some(&full), Some(NONE))["ACCURACY.RMSE_HORIZONTAL"];
    assert_eq!(observed_f64(f), Some(0.25));
    assert_eq!(f.outcome, Outcome::Pass);
    let source = f.source.as_deref().unwrap();
    assert!(source.contains("parser.metrics.rmse_x_feet") && source.contains("parser.metrics.rmse_y_feet"));

    let mut partial = metrics.clone();
    partial["rmse_y_feet"] = Value::Null;
    let f = &by_rule(&mission(json!({"rmse_x_feet": null})), &rules(json!({})), Some(&report(partial)), Some(NONE))["ACCURACY.RMSE_HORIZONTAL"];
    assert_eq!(f.observed, None);
    assert_eq!(f.outcome, Outcome::InsufficientData);

    let mut unqualified = metrics;
    unqualified["checkpoints_independent"] = Value::Null;
    let f = &by_rule(&mission(json!({"rmse_x_feet": null, "rmse_y_feet": null})), &rules(json!({})), Some(&report(unqualified)), Some(NONE))["ACCURACY.RMSE_HORIZONTAL"];
    assert_eq!(f.observed, None);
    assert!(f.next_action.as_deref().unwrap().to_lowercase().contains("independent"));
}

#[test]
fn flight_and_processing_details_describe_actual_calculations() {
    let f = by_rule(
        &mission(json!({"planned_altitude_agl_feet": 100.0, "actual_altitude_agl_feet": 125.0})),
        &rules(json!({"tolerances": {"maximum_altitude_difference_feet": 10.0}, "processing": {"minimum_calibrated_percent": 95.0}})),
        Some(&report(json!({"calibrated_images": 90, "total_images": 100}))),
        Some(NONE),
    );
    let flight = &f["FLIGHT.ALTITUDE"];
    assert_eq!(observed_f64(flight), Some(25.0));
    assert_eq!(flight.requirement.as_deref(), Some("<= 10.0"));
    assert_eq!(flight.unit.as_deref(), Some("ft"));
    let source = flight.source.as_deref().unwrap();
    assert!(source.contains("planned_altitude_agl_feet") && source.contains("actual_altitude_agl_feet"));
    let cal = &f["PROCESSING.CALIBRATED_PERCENT"];
    assert_eq!(observed_f64(cal), Some(90.0));
    assert_eq!(cal.requirement.as_deref(), Some(">= 95.0"));
    assert_eq!(cal.unit.as_deref(), Some("%"));
    let source = cal.source.as_deref().unwrap();
    assert!(source.contains("calibrated_images") && source.contains("total_images"));
}

#[test]
fn sensitive_raw_derived_and_report_details_are_suppressed() {
    let listed: Vec<String> = ["rmse_x_feet", "actual_altitude_agl_feet", "horizontal_crs", "attachments"].map(String::from).to_vec();
    for sensitive in [None, Some(listed.as_slice())] {
        let f = by_rule(
            &mission(
                json!({"horizontal_crs": "PRIVATE_CRS", "rmse_x_feet": 3.0, "rmse_y_feet": 4.0, "planned_altitude_agl_feet": 100.0, "actual_altitude_agl_feet": 137.0}),
            ),
            &rules(json!({
                "coordinates": {"horizontal_crs": "PRIVATE_CRS"},
                "tolerances": {"maximum_altitude_difference_feet": 10.0},
                "processing": {"required_metrics": ["processing_software"], "minimum_calibrated_percent": 95.0}
            })),
            Some(&report(json!({"calibrated_images": 71, "total_images": 100, "processing_software": "PRIVATE_SOFTWARE"}))),
            sensitive,
        );
        for rule in [
            "ACCURACY.RMSE_HORIZONTAL",
            "FLIGHT.ALTITUDE",
            "COORDINATES.HORIZONTAL_CRS",
            "PROCESSING.PROCESSING_SOFTWARE",
            "PROCESSING.CALIBRATED_PERCENT",
        ] {
            let x = &f[rule];
            assert!(x.observed.as_ref().is_none_or(|o| o == "[REDACTED]"), "{rule}: {:?}", x.observed);
            assert!(x.requirement.as_deref().is_none_or(|r| r == "[REDACTED]"), "{rule}: {:?}", x.requirement);
        }
        let all = serde_json::to_string(&f).unwrap();
        assert!(!all.contains("PRIVATE_CRS") && !all.contains("PRIVATE_SOFTWARE"));
    }
}

#[test]
fn missing_deliverables_are_actionable_and_privacy_aware() {
    let req = rules(json!({"required_deliverables": ["orthomosaic", "point cloud"]}));
    let source = mission(json!({"present_deliverables": ["orthomosaic"]}));
    let f = &by_rule(&source, &req, None, Some(NONE))["DELIVERABLES.PRESENCE"];
    assert_eq!(f.observed, Some(json!(["orthomosaic"])));
    assert!(f.requirement.as_deref().unwrap().contains("point cloud"));
    assert!(f.next_action.is_some());
    let private = vec!["present_deliverables".to_string()];
    let p = serde_json::to_string(&by_rule(&source, &req, None, Some(&private))["DELIVERABLES.PRESENCE"]).unwrap();
    assert!(!p.contains("orthomosaic") && !p.contains("point cloud"));
}

#[test]
fn credential_like_configured_requirement_is_redacted() {
    let marker = "SYNTHETIC_CONFIG_VALUE_72913";
    let f = &by_rule(
        &mission(json!({"horizontal_crs": "Documented CRS"})),
        &rules(json!({"coordinates": {"horizontal_crs": format!("Bearer {marker}")}})),
        None,
        Some(NONE),
    )["COORDINATES.HORIZONTAL_CRS"];
    assert_eq!(f.outcome, Outcome::Fail);
    assert!(!serde_json::to_string(f).unwrap().contains(marker));
    assert_eq!(f.observed, Some(json!("Documented CRS")));
}

#[test]
fn display_preserves_the_exact_acceptance_boundary() {
    let req = rules(json!({
        "control": {"require_bare_ground_checkpoints": true},
        "accuracy": {"maximum_rmse_horizontal_feet": 0.2499999, "maximum_rmse_z_feet": 0.2499999}
    }));
    let f = by_rule(&mission(json!({"rmse_x_feet": 0.15, "rmse_y_feet": 0.2, "checkpoints_bare_ground": null})), &req, None, Some(NONE));
    let h = &f["ACCURACY.RMSE_HORIZONTAL"];
    assert_eq!(observed_f64(h), Some(0.25));
    assert_eq!(h.outcome, Outcome::Fail);
    assert_eq!(h.requirement.as_deref(), Some("<= 0.2499999"));
    assert_eq!(f["ACCURACY.RMSE_Z"].requirement.as_deref(), Some("<= 0.2499999"));
}

#[test]
fn source_conflict_details_preserve_distinct_measurements() {
    let rep = report(json!({"checkpoints_independent": true, "rmse_x_feet": 0.123456781}));
    let f = &by_rule(&mission(json!({"rmse_x_feet": 0.123456789})), &rules(json!({})), Some(&rep), Some(NONE))["ACCURACY.SOURCE_CONFLICT_X"];
    assert_eq!(f.outcome, Outcome::Fail);
    assert_eq!(f.observed, Some(json!(["mission: 0.123456789", "report: 0.123456781"])));
}

#[test]
fn findings_and_summary_prioritize_problems_and_bound_the_brief() {
    let analysis = analyze(&mission(json!({})), &rules(json!({})), None, Some(NONE));
    assert_eq!(analysis.findings[0].rule_id, "ACCURACY.RMSE_HORIZONTAL");
    let rank = |o: Outcome| match o {
        Outcome::Fail => 0,
        Outcome::InsufficientData => 1,
        Outcome::Conditional => 2,
        Outcome::Pass => 3,
        Outcome::Info => 4,
    };
    let ranks: Vec<_> = analysis.findings.iter().map(|f| rank(f.outcome)).collect();
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]));
    let brief = summarize(&analysis);
    assert!(brief.chars().count() <= 500);
    assert!(brief.contains("ACCURACY.RMSE_HORIZONTAL") && brief.contains("FAIL") && brief.contains("INSUFFICIENT_DATA"));
    assert!(brief.contains(analysis.findings[0].next_action.as_deref().unwrap()));
    assert!(!brief.contains("All configured checks passed"));
}

#[test]
fn summary_only_declares_all_clear_for_pass_and_stays_bounded() {
    for o in [Outcome::Pass, Outcome::Info, Outcome::Conditional] {
        let finding = Finding {
            rule_id: "SYNTHETIC.CHECK".into(),
            outcome: o,
            severity: Severity::Info,
            message: "Synthetic".into(),
            observed: None,
            requirement: None,
            unit: None,
            source: None,
            next_action: None,
        };
        let a = Analysis { findings: vec![finding], outcome: o, risk_level: Severity::Info, ruleset_version: "synthetic".into() };
        assert_eq!(summarize(&a).contains("All configured checks passed"), o == Outcome::Pass);
    }
    let long = Finding {
        rule_id: "R".repeat(300),
        outcome: Outcome::Fail,
        severity: Severity::High,
        message: String::new(),
        observed: None,
        requirement: None,
        unit: None,
        source: None,
        next_action: Some("é".repeat(300)),
    };
    let a = Analysis {
        findings: vec![long.clone(), long.clone(), long],
        outcome: Outcome::Fail,
        risk_level: Severity::High,
        ruleset_version: "s".into(),
    };
    let brief = summarize(&a);
    assert_eq!(brief.chars().count(), 500);
    assert!(brief.ends_with("..."));
}

// --- test_accuracy_profile.py ---

/// The fixture ruleset with the radial and bare-ground accuracy profile.
fn profile(changes: impl FnOnce(&mut Value)) -> Ruleset {
    let mut r = fixture_ruleset_json();
    r["accuracy"] = json!({"maximum_rmse_horizontal_feet": 0.25, "maximum_rmse_z_feet": 0.25});
    r["control"]["require_bare_ground_checkpoints"] = json!(true);
    changes(&mut r);
    Ruleset::from_value(&r).unwrap()
}

fn profile_mission(changes: Value) -> Mission {
    let mut c = json!({"rmse_x_feet": 0.15, "rmse_y_feet": 0.2, "rmse_z_feet": 0.25, "checkpoints_bare_ground": true});
    for (k, v) in changes.as_object().unwrap() {
        c[k] = v.clone();
    }
    mission_with(&c)
}

#[test]
fn radial_horizontal_and_bare_ground_vertical_boundaries() {
    let up = f64::from_bits(0.25f64.to_bits() + 1);
    let (p, f, i) = (Outcome::Pass, Outcome::Fail, Outcome::InsufficientData);
    let n = Value::Null;
    let cases = [
        (json!(0.15), json!(0.2), json!(0.25), p, p),
        (json!(0.25), json!(0.0), json!(0.25), p, p),
        (json!(0.0), json!(0.25), json!(0.0), p, p),
        (json!(0.2), json!(0.2), json!(0.25), f, p),
        (json!(up), json!(0.0), json!(0.25), f, p),
        (json!(0.15), json!(0.2), json!(up), p, f),
        (n.clone(), json!(0.2), json!(0.25), i, p),
        (json!(0.15), n.clone(), json!(0.25), i, p),
        (json!(0.15), json!(0.2), n, p, i),
    ];
    for (x, y, z, horizontal, vertical) in cases {
        let r = by_rule(&profile_mission(json!({"rmse_x_feet": x, "rmse_y_feet": y, "rmse_z_feet": z})), &profile(|_| {}), None, None);
        assert_eq!(outcome(&r, "ACCURACY.RMSE_HORIZONTAL"), horizontal, "{x} {y} {z}");
        assert_eq!(outcome(&r, "ACCURACY.RMSE_Z"), vertical, "{x} {y} {z}");
        assert_eq!(outcome(&r, "ACCURACY.RMSE_X"), Outcome::Info);
        assert_eq!(outcome(&r, "ACCURACY.RMSE_Y"), Outcome::Info);
    }
}

#[test]
fn missing_independent_results_are_not_accuracy_pass_or_measured_failure() {
    let rules = profile(|r| r["control"]["minimum_checkpoint_count"] = Value::Null);
    for changes in [
        json!({"checkpoint_count": null}),
        json!({"checkpoint_count": 0}),
        json!({"checkpoints_independent": null}),
        json!({"checkpoints_independent": false}),
    ] {
        for rmse in [0.01, 10.0] {
            let mut c = changes.clone();
            for k in ["rmse_x_feet", "rmse_y_feet", "rmse_z_feet"] {
                c[k] = json!(rmse);
            }
            let a = analyze(&profile_mission(c), &rules, None, None);
            let r: std::collections::BTreeMap<_, _> = a.findings.iter().map(|f| (f.rule_id.clone(), f.clone())).collect();
            assert_eq!(outcome(&r, "CONTROL.INDEPENDENCE"), Outcome::InsufficientData);
            assert_eq!(outcome(&r, "ACCURACY.RMSE_HORIZONTAL"), Outcome::InsufficientData);
            assert_eq!(outcome(&r, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
            assert_eq!(a.outcome, Outcome::InsufficientData);
        }
    }
}

#[test]
fn unqualified_report_residuals_cannot_replace_independent_checkpoint_rmse() {
    for independent in [Value::Null, json!(false)] {
        let rep = report(
            json!({"rmse_x_feet": 0.01, "rmse_y_feet": 0.01, "rmse_z_feet": 0.01, "checkpoints_independent": independent, "checkpoints_bare_ground": true}),
        );
        let r = by_rule(&profile_mission(json!({"rmse_x_feet": null, "rmse_y_feet": null, "rmse_z_feet": null})), &profile(|_| {}), Some(&rep), None);
        assert_eq!(outcome(&r, "ACCURACY.RMSE_HORIZONTAL"), Outcome::InsufficientData);
        assert_eq!(outcome(&r, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
    }
}

#[test]
fn qualified_report_results_and_conflicts_require_explicit_provenance() {
    let metrics =
        json!({"rmse_x_feet": 0.15, "rmse_y_feet": 0.2, "rmse_z_feet": 0.25, "checkpoints_independent": true, "checkpoints_bare_ground": true});
    let rep = report(metrics.clone());
    let r = by_rule(&profile_mission(json!({"rmse_x_feet": null, "rmse_y_feet": null, "rmse_z_feet": null})), &profile(|_| {}), Some(&rep), None);
    assert_eq!(outcome(&r, "ACCURACY.RMSE_HORIZONTAL"), Outcome::Pass);
    assert_eq!(outcome(&r, "ACCURACY.RMSE_Z"), Outcome::Pass);
    let conflict = by_rule(&profile_mission(json!({"rmse_x_feet": 0.1})), &profile(|_| {}), Some(&rep), None);
    assert_eq!(outcome(&conflict, "ACCURACY.SOURCE_CONFLICT_X"), Outcome::Fail);
    let mut unq = metrics;
    unq["checkpoints_independent"] = json!(false);
    let r = by_rule(&profile_mission(json!({})), &profile(|_| {}), Some(&report(unq)), None);
    assert!(!r.keys().any(|k| k.contains("SOURCE_CONFLICT")));
}

#[test]
fn radial_rmse_never_combines_complementary_sources() {
    for (report_axis, mission_axis) in [("x", "y"), ("y", "x")] {
        for component in [0.15, 0.2] {
            let rep = report(json!({format!("rmse_{report_axis}_feet"): component, "checkpoints_independent": true}));
            let m = profile_mission(json!({format!("rmse_{report_axis}_feet"): null, format!("rmse_{mission_axis}_feet"): component}));
            let f = &by_rule(&m, &profile(|_| {}), Some(&rep), None)["ACCURACY.RMSE_HORIZONTAL"];
            assert_eq!(f.outcome, Outcome::InsufficientData);
        }
    }
}

#[test]
fn zero_checkpoint_exception_is_conditional_and_manufactures_no_accuracy() {
    let rules = profile(|r| {
        r["control"]["permit_zero_checkpoints"] = json!(true);
        r["control"]["minimum_checkpoint_count"] = Value::Null;
    });
    let r = by_rule(&profile_mission(json!({"checkpoint_count": 0})), &rules, None, None);
    assert_eq!(outcome(&r, "CONTROL.INDEPENDENCE"), Outcome::Conditional);
    assert_eq!(outcome(&r, "ACCURACY.RMSE_HORIZONTAL"), Outcome::InsufficientData);
    assert_eq!(outcome(&r, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
}

// --- test_domain.py (analyze cases) ---

#[test]
fn no_threshold_is_not_pass_and_configured_accuracy_is_deterministic() {
    let m = Mission::from_value(&json!({"global_id": GID, "rmse_x_feet": 0.2, "checkpoint_count": 1, "checkpoints_independent": true})).unwrap();
    let unset = Ruleset::from_value(&json!({"version": "test", "required_properties": []})).unwrap();
    assert_eq!(analyze(&m, &unset, None, None).outcome, Outcome::InsufficientData);
    let configured = Ruleset::from_value(&json!({"version": "test", "required_properties": [], "accuracy": {"maximum_rmse_x_feet": 0.1}})).unwrap();
    assert_eq!(outcome(&by_rule(&m, &configured, None, None), "ACCURACY.RMSE_X"), Outcome::Fail);
}

#[test]
fn fully_configured_fixture_passes_and_each_category_detects_missing_or_conflicting_data() {
    let rules = fixture_ruleset();
    let mut rep = valid_report();
    // Declare provenance only for this fabricated source-conflict check.
    rep.metrics.checkpoints_independent = Some(true);
    let base = mission_with(&json!({}));
    let a = analyze(&base, &rules, Some(&rep), Some(&pilot()));
    let bad: Vec<_> = a.findings.iter().filter(|f| !matches!(f.outcome, Outcome::Pass | Outcome::Info)).map(|f| &f.rule_id).collect();
    assert_eq!(a.outcome, Outcome::Pass, "{bad:?}");
    let (i, f) = (Outcome::InsufficientData, Outcome::Fail);
    let cases = [
        (json!({"mission_id": null}), "COMPLETENESS.PROPERTIES", i),
        (json!({"attachments": []}), "COMPLETENESS.ATTACHMENT_1", i),
        (json!({"horizontal_units": "foot"}), "COORDINATES.HORIZONTAL_UNITS", f),
        (json!({"checkpoint_count": 0}), "CONTROL.INDEPENDENCE", i),
        (json!({"checkpoints_independent": false}), "CONTROL.INDEPENDENCE", i),
        (json!({"rmse_x_feet": 0.8}), "ACCURACY.SOURCE_CONFLICT_X", f),
        (json!({"actual_altitude_agl_feet": 140.0}), "FLIGHT.ALTITUDE", f),
        (json!({"front_overlap_percent": null}), "FLIGHT.FRONT_OVERLAP", i),
        (json!({"rtk_ppk_status": null}), "CORRECTIONS.STATUS", i),
        (json!({"correction_source": null}), "CORRECTIONS.SOURCE", i),
        (json!({"present_deliverables": []}), "DELIVERABLES.PRESENCE", i),
    ];
    for (changes, rule, expected) in cases {
        assert_eq!(outcome(&by_rule(&mission_with(&changes), &rules, Some(&rep), Some(&pilot())), rule), expected, "{changes} {rule}");
    }
    assert_eq!(outcome(&by_rule(&base, &rules, None, Some(&pilot())), "PROCESSING.REPORT"), Outcome::InsufficientData);
}

#[test]
fn parser_diagnostics_are_insufficient_and_list_codes_only() {
    let mut rep = valid_report();
    rep.diagnostics
        .push(cadcraft_droneqa::ParserDiagnostic { code: "UNSUPPORTED_CONTENT_TYPE".into(), message: "PDF reports are not supported yet.".into() });
    let f = &by_rule(&mission_with(&json!({})), &fixture_ruleset(), Some(&rep), Some(&pilot()))["PROCESSING.REPORT"];
    assert_eq!(f.outcome, Outcome::InsufficientData);
    assert_eq!(f.observed, Some(json!(["UNSUPPORTED_CONTENT_TYPE"])));
}

#[test]
fn coordinate_documentation_is_strict() {
    let m = Mission::from_value(&json!({"global_id": GID, "horizontal_crs": "EPSG:26913"})).unwrap();
    let r = Ruleset::from_value(
        &json!({"version": "fixture", "required_properties": ["horizontal_crs"], "coordinates": {"horizontal_crs": "EPSG:26913"}}),
    )
    .unwrap();
    assert_eq!(outcome(&by_rule(&m, &r, None, None), "COORDINATES.DOCUMENTATION"), Outcome::InsufficientData);
}

#[test]
fn blank_canonical_text_is_missing_not_complete() {
    for field in [
        "mission_id",
        "horizontal_crs",
        "horizontal_datum",
        "vertical_units",
        "correction_source",
        "rtk_ppk_status",
        "processing_software",
        "mission_date",
    ] {
        let m = Mission::from_value(&json!({"global_id": GID, field: " \t\n "})).unwrap();
        assert_eq!(m.to_map()[field], Value::Null, "{field}");
        let r = Ruleset::from_value(&json!({"version": "fixture", "required_properties": [field]})).unwrap();
        assert_eq!(outcome(&by_rule(&m, &r, None, None), "COMPLETENESS.PROPERTIES"), Outcome::InsufficientData);
    }
}

#[test]
fn qa_findings_never_embed_private_source_values() {
    let private_crs = "SYNTHETIC_PRIVATE_CRS_12345";
    for matches in [false, true] {
        let expected = if matches { private_crs } else { "SYNTHETIC_EXPECTED_CRS" };
        let m = Mission::from_value(&json!({
            "global_id": GID, "horizontal_crs": private_crs, "rmse_x_feet": 9876.543, "checkpoint_count": 1,
            "checkpoints_independent": true, "planned_altitude_agl_feet": 0.0, "actual_altitude_agl_feet": 9876.543
        }))
        .unwrap();
        let r = Ruleset::from_value(&json!({
            "version": "privacy-fixture", "required_properties": ["horizontal_crs"], "coordinates": {"horizontal_crs": expected},
            "accuracy": {"maximum_rmse_x_feet": 1.0}, "tolerances": {"maximum_altitude_difference_feet": 1.0}
        }))
        .unwrap();
        // Default (None): every mission field is private.
        let a = analyze(&m, &r, None, None);
        let all = serde_json::to_string(&a).unwrap();
        assert!(!all.contains(private_crs) && !all.contains("9876.543"));
        let f = by_rule(&m, &r, None, None);
        assert_eq!(outcome(&f, "COORDINATES.HORIZONTAL_CRS"), if matches { Outcome::Pass } else { Outcome::Fail });
        assert_eq!(outcome(&f, "ACCURACY.RMSE_X"), Outcome::Fail);
        assert_eq!(outcome(&f, "FLIGHT.ALTITUDE"), Outcome::Fail);
    }
}

#[test]
fn attachment_patterns_match_whole_names_including_alternation() {
    let r = |p: &str| Ruleset::from_value(&json!({"version": "v", "required_properties": [], "required_attachment_patterns": [p]})).unwrap();
    let m = mission_with(&json!({}));
    assert_eq!(outcome(&by_rule(&m, &r("quality_report\\.(txt|pdf)"), None, None), "COMPLETENESS.ATTACHMENT_1"), Outcome::Pass);
    assert_eq!(outcome(&by_rule(&m, &r("txt|quality_report\\.txt"), None, None), "COMPLETENESS.ATTACHMENT_1"), Outcome::Pass);
    // Whole-name match: a prefix or suffix alone does not count.
    assert_eq!(outcome(&by_rule(&m, &r("quality"), None, None), "COMPLETENESS.ATTACHMENT_1"), Outcome::InsufficientData);
    assert_eq!(outcome(&by_rule(&m, &r("txt|pdf"), None, None), "COMPLETENESS.ATTACHMENT_1"), Outcome::InsufficientData);
}

#[test]
fn analysis_round_trips_through_json() {
    let a = analyze(&scenario_mission("fail"), &scenario_ruleset(), Some(&valid_report()), Some(&pilot()));
    let v = serde_json::to_value(&a).unwrap();
    assert_eq!(v["outcome"], "FAIL");
    assert_eq!(v["risk_level"], "HIGH");
    assert_eq!(serde_json::from_value::<Analysis>(v).unwrap(), a);
}

#[test]
fn caller_supplied_reports_are_validated_like_the_reference_models() {
    use cadcraft_droneqa::qa::validate_report;
    use cadcraft_droneqa::{ParserDiagnostic, ReportEvidence, ReportResiduals};
    assert!(validate_report(&valid_report()).is_ok());
    let residuals = ReportResiduals {
        provenance: "fitted_gcps".into(),
        unit: "US survey foot".into(),
        mean_xyz: [0.0, -0.1, 0.1],
        sigma_xyz: [0.1; 3],
        rms_xyz: [0.1; 3],
    };
    let mut with_evidence = valid_report();
    with_evidence.report_evidence = Some(ReportEvidence { gcp_fit: Some(residuals.clone()), ..ReportEvidence::default() });
    assert!(validate_report(&with_evidence).is_ok());

    let mut bad: Vec<ParseResult> = Vec::new();
    for v in [-1.0, f64::NAN, f64::INFINITY] {
        let mut r = valid_report();
        r.metrics.rmse_z_feet = Some(v);
        bad.push(r);
    }
    let mut r = valid_report();
    r.metrics.calibrated_images = Some(101);
    bad.push(r);
    let mut r = valid_report();
    r.metrics.processing_software = Some("x".repeat(2001));
    bad.push(r);
    let mut r = valid_report();
    r.profile = " ".into();
    bad.push(r);
    let mut r = valid_report();
    r.diagnostics = vec![ParserDiagnostic { code: "X".into(), message: "m".into() }; 257];
    bad.push(r);
    for (provenance, unit, sigma) in [("checkpoints", "US survey foot", 0.1), ("fitted_gcps", "metre", 0.1), ("fitted_gcps", "US survey foot", -0.1)]
    {
        let mut r = valid_report();
        r.report_evidence = Some(ReportEvidence {
            camera_offsets: Some(ReportResiduals { provenance: provenance.into(), unit: unit.into(), sigma_xyz: [sigma; 3], ..residuals.clone() }),
            ..ReportEvidence::default()
        });
        bad.push(r);
    }
    for r in &bad {
        assert!(validate_report(r).is_err(), "{r:?}");
    }
    let mut r = valid_report();
    r.metrics.processing_software = Some(format!("token=SYNTHETIC_PRIVATE {}", "x".repeat(2001)));
    assert!(!validate_report(&r).unwrap_err().contains("SYNTHETIC_PRIVATE"));
}

#[test]
fn analyze_ignores_an_invalid_report_instead_of_passing_it() {
    // Codex's reproduction (#10): a negative report RMSE must not pass the Z criterion.
    let rep = report(json!({"checkpoints_independent": true, "checkpoints_bare_ground": true, "rmse_z_feet": -1.0}));
    let f = by_rule(&mission(json!({"rmse_z_feet": null})), &rules(json!({})), Some(&rep), Some(NONE));
    assert_eq!(outcome(&f, "ACCURACY.RMSE_Z"), Outcome::InsufficientData);
    assert_eq!(outcome(&f, "PROCESSING.REPORT"), Outcome::InsufficientData);
}
