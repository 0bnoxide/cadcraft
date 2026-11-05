use cadcraft_droneqa::pix4d::parse;

#[test]
fn shared_valid_fixture_has_no_diagnostics() {
    let parsed = parse(include_bytes!("fixtures/valid.txt"), "text/plain");
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.profile, "synthetic-pix4d-v1");
    assert_eq!(parsed.metrics.rmse_x_feet, Some(0.1));
    assert_eq!(parsed.metrics.rmse_y_feet, Some(0.2));
    assert_eq!(parsed.metrics.rmse_z_feet, Some(0.3));
    assert_eq!(parsed.metrics.calibrated_images, Some(90));
    assert_eq!(parsed.metrics.total_images, Some(100));
    assert_eq!(parsed.metrics.checkpoints_independent, None);
    assert_eq!(parsed.metrics.processing_software.as_deref(), Some("Pix4D synthetic"));
    assert_eq!(parsed.metrics.processing_version.as_deref(), Some("fixture-1"));
}

#[test]
fn synthetic_metrics_and_duplicate_invalidation() {
    let report = "AIGIS3D SYNTHETIC PIX4D QA v1\nRMSE X: 0.1 ft\nRMSE Y: 0.2 ft\nRMSE Z: 0.3 ft\nImages calibrated: 90\nImages total: 100\nSoftware: synthetic\nVersion: fixture-1\n";
    let parsed = parse(report.as_bytes(), "text/plain");
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.metrics.rmse_x_feet, Some(0.1));
    let duplicate = parse(format!("{report}RMSE X: 0.2 ft\nRMSE X: 0.3 ft").as_bytes(), "text/plain");
    assert_eq!(duplicate.metrics.rmse_x_feet, None);
    assert!(duplicate.diagnostics.iter().any(|item| item.code == "DUPLICATE_METRIC"));
}

#[test]
fn hostile_inputs_do_not_echo_or_supply_metrics() {
    for (bytes, mime) in
        [(b"private report".as_slice(), "application/pdf"), (b"\xff".as_slice(), "text/plain"), (b"private report".as_slice(), "text/plain")]
    {
        let result = parse(bytes, mime);
        assert!(!result.diagnostics.is_empty());
        assert!(!format!("{result:?}").contains("private report"));
    }
    for raw in ["-1 ft", "inf ft", "1 m", "1e9 ft"] {
        let report = format!("AIGIS3D SYNTHETIC PIX4D QA v1\nRMSE X: {raw}");
        assert_eq!(parse(report.as_bytes(), "text/plain").metrics.rmse_x_feet, None);
    }
}
