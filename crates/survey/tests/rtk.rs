//! RTK import and validation, ported from AutoGIS `tests/envmon/test_import_rtk_survey.py` and
//! `test_validate_rtk_survey.py` (the arcpy and CLI tests are left out).

use cadcraft_survey::rtk::{ColumnMap, CoordOrder, DEFAULT_HRMS, DEFAULT_VRMS, ImportOptions, MAX_ROWS, RtkPoint, parse_rtk_csv, qa_flags, validate};
use cadcraft_survey::{Finding, Severity, SurveyPoint};

const HEADERED: &str = "\
PointID,Northing,Easting,Elevation_ft,FeatureCode,Description,HRMS_ft,VRMS_ft,FixType,CollectedAt,Operator
MW-01,4527893.12,293847.55,512.34,WELL,Monitoring Well,0.01,0.02,RTK_FIXED,2026-06-15,Alice
MW-02,4527750.00,293900.00,509.12,WELL,Monitoring Well,0.05,0.08,RTK_FLOAT,2026-06-15,Alice
INV001,4527800.00,293870.00,510.00,INV,Invalid high precision,0.50,0.80,AUTONOMOUS,2026-06-15,Alice
";
const HL5_CONFIDENT: &str = "1,558281.5482,2206038.3110,3224.0823,PT-1\n2,558300.1200,2206050.4400,3225.1000,PT-2\n";
const HL5_AMBIGUOUS: &str = "1,555000.12,545000.34,510.50,PT-1\n2,556000.00,546000.00,511.00,PT-2\n";
const EXTENDED_11COL: &str = "1,558281.5482,2206038.3110,3224.0823,MW-5R,0.011,0.019,1.2,22,FIXED,14:23:10\n";

fn parse(text: &str) -> Vec<RtkPoint> {
    parse_rtk_csv(text.as_bytes(), &ImportOptions::default()).unwrap().points
}

fn with(text: &str, opts: ImportOptions) -> cadcraft_survey::rtk::Import {
    parse_rtk_csv(text.as_bytes(), &opts).unwrap()
}

fn fail(text: &str, opts: ImportOptions) -> String {
    parse_rtk_csv(text.as_bytes(), &opts).unwrap_err().0
}

fn extra(names: &[&str]) -> ImportOptions {
    ImportOptions { extra_columns: Some(names.iter().map(|s| s.to_string()).collect()), ..Default::default() }
}

fn order(order: CoordOrder) -> ImportOptions {
    ImportOptions { order, ..Default::default() }
}

fn has(findings: &[Finding], category: &str) -> bool {
    findings.iter().any(|f| f.message.starts_with(&format!("{category}:")))
}

fn by_id<'a>(points: &'a [RtkPoint], id: &str) -> &'a RtkPoint {
    points.iter().find(|p| p.point.id == id).unwrap()
}

#[test]
fn headered_count_northing_and_id() {
    let points = parse(HEADERED);
    assert_eq!(points.len(), 3);
    assert!((points[0].point.northing - 4527893.12).abs() < 0.001);
    assert_eq!(points[0].point.id, "MW-01");
    assert_eq!(points[0].point.feature_code, "WELL");
    assert_eq!(points[0].operator, "Alice");
}

#[test]
fn qa_flags_fixed_poor_hrms_and_autonomous() {
    let points = parse(HEADERED);
    assert!(qa_flags(by_id(&points, "MW-01"), 0.03, 0.05).is_empty());
    assert!(qa_flags(by_id(&points, "MW-02"), 0.03, 0.05).contains(&"hrms_exceeds_threshold"));
    assert!(qa_flags(by_id(&points, "INV001"), 0.10, 0.10).contains(&"fix_type_not_rtk"));
}

#[test]
fn custom_column_map() {
    let cm = ColumnMap {
        point_id: "ID".into(),
        northing: "Y".into(),
        easting: "X".into(),
        elevation: "Z".into(),
        feature_code: "Code".into(),
        ..Default::default()
    };
    let i = with("ID,Y,X,Z,Code\nMW-01,4527893.12,293847.55,512.34,WELL\n", ImportOptions { columns: cm, ..Default::default() });
    assert_eq!(i.points.len(), 1);
    assert_eq!(i.points[0].point.northing, 4527893.12);
    assert_eq!(i.points[0].point.feature_code, "WELL");
}

#[test]
fn headered_rows_missing_coordinates_are_skipped_with_a_warning() {
    let text = "PointID,Northing,Easting,Elevation_ft\nA,1,2,3\nB,,2,3\nC,1,2\n";
    let i = with(text, ImportOptions::default());
    assert_eq!(i.points.len(), 1);
    assert!(has(&i.findings, "skipped_rows"));
}

#[test]
fn headerless_5col_is_not_treated_as_header() {
    assert_eq!(parse(HL5_CONFIDENT).len(), 2);
}

#[test]
fn headerless_5col_confident_order_and_warning() {
    let i = with(HL5_CONFIDENT, ImportOptions::default());
    let p = &i.points[0];
    assert_eq!(p.point.id, "1");
    assert!((p.point.easting - 558281.5482).abs() < 1e-9);
    assert!((p.point.northing - 2206038.3110).abs() < 1e-9);
    assert_eq!(p.point.description, "PT-1");
    assert!(has(&i.findings, "guessed_coord_order"));
}

#[test]
fn headerless_5col_ambiguous_magnitude_is_refused() {
    assert!(fail(HL5_AMBIGUOUS, ImportOptions::default()).contains("`format`"));
}

#[test]
fn headerless_explicit_order_bypasses_guess() {
    let i = with(HL5_AMBIGUOUS, order(CoordOrder::Pnezd));
    assert_eq!((i.points[0].point.northing, i.points[0].point.easting), (555000.12, 545000.34));
    assert!(!has(&i.findings, "guessed_coord_order"));
    let i = with(HL5_AMBIGUOUS, order(CoordOrder::Penzd));
    assert_eq!((i.points[0].point.easting, i.points[0].point.northing), (555000.12, 545000.34));
}

#[test]
fn headerless_11col_extended_layout() {
    let points = parse(EXTENDED_11COL);
    assert_eq!(points.len(), 1);
    let p = &points[0];
    assert_eq!(p.point.id, "1");
    assert_eq!((p.point.easting, p.point.northing, p.point.elevation), (558281.5482, 2206038.3110, 3224.0823));
    assert_eq!(p.point.description, "MW-5R");
    assert_eq!((p.hrms, p.vrms, p.pdop, p.satellites), (Some(0.011), Some(0.019), Some(1.2), Some(22)));
    assert_eq!(p.fix_type, "RTK_FIXED"); // normalised from "FIXED"
    assert_eq!(p.collected_at, "14:23:10");
}

#[test]
fn fix_type_normalization_variants() {
    let text = "1,558281.55,2206038.31,3224.08,A,,,,,FIXED,\n\
                2,558290.55,2206040.31,3225.08,B,,,,,float,\n\
                3,558295.55,2206045.31,3226.08,C,,,,,nrtk,\n\
                4,558298.55,2206048.31,3227.08,D,,,,,DGPS,\n";
    let points = parse(text);
    let fix: Vec<&str> = points.iter().map(|p| p.fix_type.as_str()).collect();
    assert_eq!(fix, ["RTK_FIXED", "RTK_FLOAT", "NETWORK_RTK", "DGPS"]);
    assert!(qa_flags(&points[3], DEFAULT_HRMS, DEFAULT_VRMS).contains(&"fix_type_not_rtk"));
    assert!(qa_flags(&points[0], DEFAULT_HRMS, DEFAULT_VRMS).is_empty());
}

#[test]
fn extra_columns_override_non_11col() {
    let i = with("1,558281.55,2206038.31,3224.08,PT-1,0.02,0.03\n", extra(&["hrms", "vrms"]));
    assert_eq!((i.points[0].hrms, i.points[0].vrms), (Some(0.02), Some(0.03)));
}

#[test]
fn extra_columns_count_mismatch_is_refused_not_silently_dropped() {
    let e = fail("1,558281.55,2206038.31,3224.08,PT-1,0.02,0.03\n", extra(&["hrms"]));
    assert!(e.contains("count must match"), "{e}");
}

#[test]
fn non_numeric_coordinate_names_the_row() {
    let text = "1,558281.5482,2206038.3110,3224.0823,PT-1\n2,N/A,2206050.4400,3225.1000,PT-2\n";
    let e = fail(text, ImportOptions::default());
    assert!(e.contains("row 2"), "{e}");
}

#[test]
fn ragged_row_is_a_clear_error() {
    let text = "1,558281.5482,2206038.3110,3224.0823,PT-1\n2,558300.1200,2206050.4400\n";
    assert!(fail(text, ImportOptions::default()).contains("ragged"));
}

#[test]
fn extra_columns_unrecognized_field_is_refused() {
    let e = fail("1,558281.55,2206038.31,3224.08,PT-1,0.02\n", extra(&["bogus_field"]));
    assert!(e.contains("unrecognized"), "{e}");
}

#[test]
fn extra_columns_noop_on_5col_warns() {
    let i = with(HL5_CONFIDENT, extra(&["hrms"]));
    assert_eq!(i.points.len(), 2);
    assert!(i.findings.iter().any(|f| f.message.contains("no extra columns")));
}

#[test]
fn unsupported_width_without_extra_columns_is_refused() {
    let e = fail("1,558281.5482,2206038.3110,3224.0823,PT-1,extra1,extra2\n", ImportOptions::default());
    assert!(e.contains("`extra_columns`"), "{e}");
}

#[test]
fn headered_ignores_format_and_extra_columns_with_warning() {
    let opts = ImportOptions { order: CoordOrder::Pnezd, ..extra(&["hrms"]) };
    let i = with(HEADERED, opts);
    assert_eq!(i.points.len(), 3);
    assert!(has(&i.findings, "format_options_ignored"));
}

#[test]
fn hostile_input_does_not_panic() {
    // Infinite and NaN coordinates are not numbers; a quoted comma stays in its field.
    // An infinite coordinate makes row 1 a header, so nothing is imported.
    assert!(parse("1,inf,2206038.31,3224.08,PT-1\n").is_empty());
    assert_eq!(parse("PointID,Northing,Easting,Elevation_ft\nA,NaN,1,2\n").len(), 0);
    let p = parse("PointID,Northing,Easting,Elevation_ft,Description\nA,1,2,3,\"well, north\"\n");
    assert_eq!(p[0].point.description, "well, north");
    // Huge coordinates whose mean overflows are refused, not averaged to infinity.
    let big = "1,1e308,2,3,A\n2,1e308,2,3,B\n";
    assert!(fail(big, ImportOptions::default()).contains("too large"));
    // Satellite counts out of range are dropped.
    let p = parse("1,558281.55,2206038.31,3224.08,A,,,,-4,,\n2,558281.55,2206038.31,3224.08,A,,,,1e20,,\n");
    assert_eq!((p[0].satellites, p[1].satellites), (None, None));
    assert!(parse("").is_empty());
    assert!(parse("\u{feff}PointID,Northing,Easting,Elevation_ft\nA,1,2,3\n").len() == 1);
    assert!(parse_rtk_csv(&[0xff, 0xfe, 0x00][..], &ImportOptions::default()).is_err());
}

#[test]
fn row_count_is_capped() {
    let text = "1,2,3,4,5\n".repeat(MAX_ROWS + 1);
    assert!(fail(&text, order(CoordOrder::Pnezd)).contains("more than"));
}

fn pt(id: &str, hrms: f64, vrms: f64, fix: &str) -> RtkPoint {
    RtkPoint {
        point: SurveyPoint { id: id.into(), northing: 4527893.12, easting: 293847.55, elevation: 512.34, ..Default::default() },
        hrms: Some(hrms),
        vrms: Some(vrms),
        fix_type: fix.into(),
        ..Default::default()
    }
}

fn summary(f: &[Finding]) -> &str {
    &f.iter().find(|f| f.message.starts_with("validation_complete:")).unwrap().message
}

#[test]
fn valid_points_have_no_errors_and_a_summary() {
    let f = validate(&[pt("MW-01", 0.01, 0.02, "RTK_FIXED"), pt("MW-02", 0.01, 0.02, "RTK_FIXED")], DEFAULT_HRMS, DEFAULT_VRMS);
    assert!(f.iter().all(|f| f.severity != Severity::Error));
    assert_eq!(summary(&f), "validation_complete: RTK validation: 2/2 points QA pass.");
    assert_eq!(f.last().map(|f| f.severity), Some(Severity::Info));
}

#[test]
fn poor_precision_and_autonomous_fix_warn() {
    let f = validate(&[pt("MW-03", 0.15, 0.20, "AUTONOMOUS")], DEFAULT_HRMS, DEFAULT_VRMS);
    assert!(has(&f, "hrms_exceeds_threshold"));
    assert!(has(&f, "fix_type_not_rtk"));
    assert!(f.iter().filter(|f| f.severity == Severity::Warning).count() == 3);
}

#[test]
fn duplicate_point_id_warns_and_counts_once() {
    let f = validate(&[pt("MW-01", 0.01, 0.02, "RTK_FIXED"), pt("MW-01", 0.01, 0.02, "RTK_FIXED")], DEFAULT_HRMS, DEFAULT_VRMS);
    assert!(has(&f, "duplicate_point_id"));
    assert_eq!(summary(&f), "validation_complete: RTK validation: 1/2 points QA pass.");
}

#[test]
fn duplicate_point_id_with_a_flag_does_not_pass() {
    let f = validate(&[pt("MW-01", 0.15, 0.02, "AUTONOMOUS"), pt("MW-01", 0.01, 0.02, "RTK_FIXED")], DEFAULT_HRMS, DEFAULT_VRMS);
    assert_eq!(summary(&f), "validation_complete: RTK validation: 0/2 points QA pass.");
    assert!(has(&f, "fix_type_not_rtk"));
}

#[test]
fn duplicates_are_reported_in_first_seen_order() {
    let pts = [pt("B", 0.0, 0.0, ""), pt("A", 0.0, 0.0, ""), pt("B", 0.0, 0.0, ""), pt("A", 0.0, 0.0, "")];
    let dups: Vec<&str> = validate(&pts, DEFAULT_HRMS, DEFAULT_VRMS)
        .iter()
        .filter(|f| f.message.starts_with("duplicate_point_id:"))
        .map(|f| if f.message.contains("'B'") { "B" } else { "A" })
        .collect();
    assert_eq!(dups, ["B", "A"]);
}

#[test]
fn no_points_no_error() {
    let f = validate(&[], DEFAULT_HRMS, DEFAULT_VRMS);
    assert!(f.iter().all(|f| f.severity != Severity::Error));
    assert!(has(&f, "validation_complete"));
}

#[test]
fn custom_thresholds() {
    let f = validate(&[pt("MW-01", 0.05, 0.04, "RTK_FIXED")], 0.02, 0.02);
    assert!(has(&f, "hrms_exceeds_threshold"));
}

#[test]
fn long_lines_and_wide_rows_are_refused_before_allocating() {
    let long = format!("1,2,3,4,{}\n", "x".repeat(cadcraft_survey::rtk::MAX_LINE_BYTES));
    assert!(fail(&long, ImportOptions::default()).contains("longer than"));
    let wide = ",".repeat(cadcraft_survey::rtk::MAX_FIELDS);
    assert!(fail(&wide, ImportOptions::default()).contains("fields"));
}
