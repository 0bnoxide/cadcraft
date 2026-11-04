//! RTK control check, ported from AutoGIS `tests/test_rtk_control_check.py`.

use cadcraft_survey::Severity;
use cadcraft_survey::control::{
    ControlPoint, DEFAULT_HORIZONTAL_TOLERANCE as H, DEFAULT_VERTICAL_TOLERANCE as V, evaluate, read_control_csv, write_results_csv,
};

const HEADER: &str = "control_id,published_x,published_y,published_z,surveyed_x,surveyed_y,surveyed_z\n";

/// A control point surveyed at the given offsets from its published position.
fn cp(id: &str, dx: f64, dy: f64, dz: f64) -> ControlPoint {
    ControlPoint {
        control_id: id.into(),
        published_x: 1000.0,
        published_y: 2000.0,
        published_z: 500.0,
        surveyed_x: 1000.0 + dx,
        surveyed_y: 2000.0 + dy,
        surveyed_z: 500.0 + dz,
    }
}

fn has(f: &[cadcraft_survey::Finding], category: &str) -> bool {
    f.iter().any(|f| f.message.starts_with(&format!("{category}:")))
}

#[test]
fn perfect_control_point() {
    let (s, f) = evaluate(&[cp("CP-1", 0.0, 0.0, 0.0)], H, V);
    assert!(s.overall_pass);
    assert_eq!((s.n_pass, s.n_fail, s.rmse_horizontal), (1, 0, 0.0));
    assert!(has(&f, "control_check_complete"));
    assert_eq!(f.last().map(|f| f.severity), Some(Severity::Info));
}

#[test]
fn single_failing_point_fails_whole_network() {
    let (s, f) = evaluate(&[cp("CP-1", 0.0, 0.0, 0.0), cp("CP-2", 0.0, 0.0, 0.0), cp("CP-3", 0.5, 0.0, 0.0)], 0.05, V);
    assert_eq!((s.n_pass, s.n_fail, s.overall_pass), (2, 1, false));
    assert!(f.iter().any(|f| f.severity == Severity::Error && f.message.starts_with("control_point_fails_tolerance:")));
}

#[test]
fn vertical_tolerance_checked_independently() {
    let (s, _) = evaluate(&[cp("CP-1", 0.0, 0.0, 0.2)], 0.05, 0.10);
    assert!(!s.overall_pass);
    assert!(!s.results[0].point_pass);
    assert!((s.results[0].vertical_distance - 0.2).abs() < 1e-9);
}

#[test]
fn rmse_computation() {
    let (s, _) = evaluate(&[cp("CP-1", 3.0, 4.0, 0.0), cp("CP-2", 3.0, 4.0, 0.0)], 10.0, V);
    assert!((s.rmse_horizontal - 5.0).abs() < 1e-9);
}

#[test]
fn empty_control_points() {
    let (s, f) = evaluate(&[], H, V);
    assert!(!s.overall_pass);
    assert_eq!(s.n_points, 0);
    assert!(has(&f, "no_control_points"));
}

#[test]
fn reads_control_check_csv() {
    let text = format!("{HEADER}CP-1,1000.0,2000.0,500.0,1000.01,2000.01,500.02\nCP-2,1010.0,2010.0,502.0,1010.0,2010.0,502.0\n");
    let points = read_control_csv(text.as_bytes()).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].control_id, "CP-1");
    assert_eq!(points[1].surveyed_z, 502.0);
}

#[test]
fn columns_may_come_in_any_order() {
    let text = "surveyed_z,control_id,published_x,published_y,published_z,surveyed_x,surveyed_y\n7,CP-9,1,2,3,4,5\n";
    let p = read_control_csv(text.as_bytes()).unwrap();
    assert_eq!((p[0].control_id.as_str(), p[0].published_x, p[0].surveyed_z), ("CP-9", 1.0, 7.0));
}

#[test]
fn malformed_row_reports_line_number() {
    let text = format!("{HEADER}CP-1,1000.0,2000.0,500.0,1000.01,2000.01,500.02\nCP-2,not_a_number,2010.0,502.0,1010.0,2010.0,502.0\n");
    let e = read_control_csv(text.as_bytes()).unwrap_err().0;
    assert!(e.contains("line 3"), "{e}");
}

#[test]
fn short_row_or_missing_column_is_an_error_not_a_panic() {
    let e = read_control_csv(format!("{HEADER}CP-1,1,2\n").as_bytes()).unwrap_err().0;
    assert!(e.contains("line 2"), "{e}");
    let e = read_control_csv("control_id,published_x\nCP-1,1\n".as_bytes()).unwrap_err().0;
    assert!(e.contains("missing column"), "{e}");
    let e = read_control_csv(format!("{HEADER}CP-1,inf,2,3,4,5,6\n").as_bytes()).unwrap_err().0;
    assert!(e.contains("published_x"), "{e}");
}

#[test]
fn header_only_is_empty() {
    assert!(read_control_csv(HEADER.as_bytes()).unwrap().is_empty());
    assert!(read_control_csv(&b""[..]).unwrap().is_empty());
}

#[test]
fn duplicate_control_id_processed_independently() {
    let (s, _) = evaluate(&[cp("CP-1", 0.0, 0.0, 0.0), cp("CP-1", 0.5, 0.0, 0.0)], 0.05, V);
    assert_eq!((s.n_points, s.n_pass, s.n_fail), (2, 1, 1));
}

#[test]
fn writes_results_csv() {
    let (s, _) = evaluate(&[cp("CP-1", 0.02, 0.0, 0.03), cp("CP,2", 0.0, 0.0, 0.0)], H, V);
    let mut out = Vec::new();
    write_results_csv(&s, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("control_id,delta_x,"));
    assert!(text.contains("CP-1,"));
    assert!(text.contains("\"CP,2\",0,0,0,0,0,True"), "{text}");
}

#[test]
fn rmse_of_large_finite_residuals_stays_finite() {
    let mut p = cp("CP-1", 0.0, 0.0, 0.0);
    (p.published_x, p.surveyed_x) = (0.0, 1e200);
    let (s, _) = evaluate(&[p.clone(), p], H, V);
    assert_eq!(s.rmse_horizontal, 1e200);
}

#[test]
fn write_errors_are_reported() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("disk full"))
        }
    }
    let (s, _) = evaluate(&[cp("CP-1", 0.0, 0.0, 0.0)], H, V);
    // Buffered, so the error only appears at the final flush.
    assert!(write_results_csv(&s, std::io::BufWriter::new(Full)).is_err());
}
