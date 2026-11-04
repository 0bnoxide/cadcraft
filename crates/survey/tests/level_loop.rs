//! Behaviour fixtures ported from AutoGIS tests/test_level_loop.py (no geodatabase writes).
use cadcraft_survey::{Severity, level_loop::*};
use std::collections::BTreeSet;

fn obs(point: &str, bs: Option<f64>, fs: Option<f64>, side: Option<f64>) -> Observation {
    Observation {
        run_id: "L1".into(),
        setup_id: "1".into(),
        point_id: point.into(),
        backsight: bs,
        foresight: fs,
        intermediate_sight: side,
        ..Default::default()
    }
}
fn options(tolerance: Option<f64>) -> LoopOptions {
    LoopOptions {
        run_id: "L1".into(),
        site_id: "S".into(),
        survey_date: "2026-04-01".into(),
        benchmark_id: "BM".into(),
        known_elevation: 100.0,
        tolerance,
    }
}
fn loop_rows(closing: f64) -> Vec<Observation> {
    vec![obs("BM", Some(2.0), None, None), obs("TP1", None, Some(3.0), None), obs("TP1", Some(4.0), None, None), obs("BM", None, Some(closing), None)]
}
fn close(actual: Option<f64>, expected: f64) {
    assert!((actual.unwrap() - expected).abs() < 1e-9);
}

#[test]
fn perfect_loop_preserves_raw_elevations_and_blocks_promotion() {
    let result = process_level_loop(&loop_rows(3.0), &options(Some(0.05))).unwrap();
    close(result.run.misclosure, 0.0);
    close(result.observations[1].elevation, 99.0);
    assert!(!result.findings.iter().any(|f| f.severity == Severity::Error));
    let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::new()).unwrap();
    assert_eq!(plan.block_reason.as_deref(), Some("not_adjusted"));
    assert!(plan.updates.is_empty());
}

#[test]
fn correction_is_equal_per_setup_even_when_outside_tolerance() {
    let result = process_level_loop(&loop_rows(2.9), &options(Some(0.05))).unwrap();
    close(result.run.misclosure, 0.1);
    close(result.observations[1].elevation, 98.95);
    close(result.observations[3].elevation, 100.1); // Reference keeps the closing shot raw.
    assert!(result.run.adjusted);
    assert!(result.findings.iter().any(|f| f.severity == Severity::Error && f.message.starts_with("misclosure_exceeds_tolerance:")));
    let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::new()).unwrap();
    assert_eq!(plan.block_reason.as_deref(), Some("misclosure_exceeds_tolerance"));
}

#[test]
fn default_tolerance_and_intermediate_sights() {
    let mut rows = loop_rows(3.0);
    rows.insert(1, obs("IS1", None, None, Some(1.5)));
    let result = process_level_loop(&rows, &options(None)).unwrap();
    assert!((result.run.closure_tolerance - 0.07071067811865477).abs() < 1e-12);
    close(result.observations[1].elevation, 100.5);
    close(result.observations[2].elevation, 99.0);
    assert!(result.findings.iter().any(|f| f.message.starts_with("closure_tolerance_default:")));
}

#[test]
fn combined_turning_point_processes_foresight_before_backsight() {
    let rows = vec![obs("BM", Some(2.0), None, None), obs("TP1", Some(4.0), Some(3.0), None), obs("BM", None, Some(2.9), None)];
    let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
    close(result.observations[1].hi, 103.0);
    close(result.observations[1].elevation, 98.95);
}

#[test]
fn invalid_readings_missing_backsight_and_unclosed_loop_are_ordered_findings() {
    let rows = vec![obs("BM", Some(-2.0), None, None), obs("TP", None, Some(2.0), None)];
    let result = process_level_loop(&rows, &options(Some(0.05))).unwrap();
    let categories: Vec<_> = result.findings.iter().map(|f| f.message.split(':').next().unwrap()).collect();
    assert_eq!(categories, ["negative_reading", "missing_backsight", "unclosed_loop", "benchmark_mismatch", "level_loop_complete"]);
    assert_eq!(result.run.misclosure, None);
}

#[test]
fn repeated_turning_point_warns() {
    let mut rows = loop_rows(3.0);
    rows.insert(3, obs("TP1", None, Some(2.5), None));
    let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
    assert!(result.findings.iter().any(|f| f.message.starts_with("duplicate_turning_point:")));
}

#[test]
fn revisited_turning_points_keep_each_occurrence_correction_when_promoting_side_shots() {
    for combined in [true, false] {
        let book = [
            obs("BM", Some(2.0), None, None),
            obs("A", Some(2.0), Some(1.0), None),
            obs("W1", None, None, Some(1.0)),
            obs("B", Some(2.0), Some(1.0), None),
            obs("A", Some(2.0), Some(1.0), None),
            obs("W2", None, None, Some(1.0)),
            obs("BM", None, Some(4.9), None),
        ];
        let mut rows = Vec::new();
        for row in book {
            if !combined && row.backsight.is_some() && row.foresight.is_some() {
                rows.push(Observation { backsight: None, ..row.clone() });
                rows.push(Observation { foresight: None, ..row });
            } else {
                rows.push(row);
            }
        }
        let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
        assert!(!result.run.has_errors);
        close(result.run.misclosure, 0.1);
        let a_shots: Vec<_> = result.observations.iter().filter(|r| r.point_id == "A" && r.foresight.is_some()).collect();
        close(a_shots[0].elevation, 100.975);
        close(a_shots[1].elevation, 102.925);
        let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::from(["W1".into(), "W2".into()])).unwrap();
        assert_eq!(plan.block_reason, None);
        close(plan.updates.get("W1").copied(), 101.975);
        close(plan.updates.get("W2").copied(), 103.925);
    }
}

#[test]
fn closed_setups_require_a_new_backsight_before_further_sights() {
    for rows in [
        vec![
            obs("BM", Some(2.0), None, None),
            obs("A", None, Some(3.0), None),
            obs("B", Some(4.0), Some(2.0), None),
            obs("W", None, None, Some(1.0)),
            obs("BM", None, Some(3.9), None),
        ],
        vec![
            obs("BM", Some(2.0), None, None),
            obs("A", None, Some(3.0), None),
            obs("W", None, None, Some(1.0)),
            obs("A", Some(4.0), None, None),
            obs("BM", None, Some(2.9), None),
        ],
    ] {
        let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
        assert!(result.run.has_errors);
        assert_eq!(result.observations[2].elevation, None);
        assert!(result.findings.iter().any(|f| f.severity == Severity::Error && f.message.starts_with("missing_backsight:")));
        let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::from(["W".into()])).unwrap();
        assert!(plan.block_reason.is_some());
        assert!(plan.updates.is_empty());
    }
}

#[test]
fn an_earlier_benchmark_closure_cannot_authorize_a_later_unclosed_setup() {
    let rows =
        [obs("BM", Some(2.0), None, None), obs("BM", None, Some(1.9), None), obs("BM", Some(2.0), None, None), obs("W", None, Some(1.0), None)];
    let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
    assert_eq!(result.run.misclosure, None);
    assert!(result.run.has_errors);
    assert!(result.findings.iter().any(|f| f.message.starts_with("unclosed_loop:")));
    let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::from(["W".into()])).unwrap();
    assert!(plan.block_reason.is_some());
    assert!(plan.updates.is_empty());
}

#[test]
fn promotion_excludes_benchmark_uses_last_elevation_and_deduplicates_skips() {
    let mut result = process_level_loop(&loop_rows(2.9), &options(Some(0.5))).unwrap();
    result.observations.extend([
        Observation { point_id: "TP1".into(), elevation: Some(51.0), ..Default::default() },
        Observation { point_id: "unknown".into(), elevation: Some(9.0), ..Default::default() },
        Observation { point_id: "unknown".into(), elevation: Some(10.0), ..Default::default() },
    ]);
    let wells = BTreeSet::from(["BM".into(), "TP1".into()]);
    let plan = select_elevations_for_update(&result.run, &result.observations, &wells).unwrap();
    assert_eq!(plan.block_reason, None);
    assert_eq!(plan.updates, [("TP1".into(), 51.0)].into());
    assert_eq!(plan.skipped, ["unknown"]);
    assert_eq!(plan.survey_date, "2026-04-01");
    assert!(select_elevations_for_update(&result.run, &[], &wells).unwrap().updates.is_empty());
}

#[test]
fn hostile_numbers_and_overflow_return_errors() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut opts = options(Some(0.5));
        opts.known_elevation = value;
        assert!(process_level_loop(&[], &opts).is_err());
        let rows = vec![obs("BM", Some(value), None, None)];
        assert!(process_level_loop(&rows, &options(Some(0.5))).is_err());
    }
    let mut opts = options(Some(-1.0));
    assert!(process_level_loop(&[], &opts).is_err());
    opts.tolerance = Some(0.5);
    opts.known_elevation = f64::MAX;
    assert!(process_level_loop(&[obs("BM", Some(f64::MAX), None, None)], &opts).is_err());
}

#[test]
fn observation_csv_accepts_bom_reordered_columns_and_quoted_ids() {
    let rows = read_observations("\u{feff}point_id,setup_id,run_id,foresight,backsight,intermediate_sight\r\n\"BM, north\",1,L1,,2,\r\n".as_bytes())
        .unwrap();
    assert_eq!(rows[0].point_id, "BM, north");
    assert_eq!(rows[0].backsight, Some(2.0));
    for csv in ["point_id\nBM\n", "run_id,setup_id,point_id,backsight,foresight,intermediate_sight\nL,1,BM,nope,,\n"] {
        assert!(read_observations(csv.as_bytes()).is_err());
    }
}

#[test]
fn missing_foresight_and_wrong_start_block_elevation_promotion() {
    let mut rows = loop_rows(2.9);
    rows.insert(1, obs("BM", Some(4.0), None, None));
    let result = process_level_loop(&rows, &options(Some(5.0))).unwrap();
    assert!(result.findings.iter().any(|f| f.severity == Severity::Error && f.message.starts_with("missing_foresight:")));
    let plan = select_elevations_for_update(&result.run, &result.observations, &BTreeSet::from(["TP1".into()])).unwrap();
    assert_eq!(plan.block_reason.as_deref(), Some("invalid_observations"));
    assert!(plan.updates.is_empty());
    let rows = vec![obs("other", None, None, Some(1.0)), obs("BM", Some(2.0), None, None), obs("BM", None, Some(1.9), None)];
    let result = process_level_loop(&rows, &options(Some(0.5))).unwrap();
    assert!(result.findings.iter().any(|f| f.message.starts_with("benchmark_mismatch:")));
}

#[test]
fn final_setup_without_foresight_is_reported() {
    let result = process_level_loop(&[obs("BM", Some(2.0), None, None)], &options(Some(0.5))).unwrap();
    assert!(result.findings.iter().any(|f| f.message.starts_with("missing_foresight:")));
}
