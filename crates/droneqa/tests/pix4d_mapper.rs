use cadcraft_droneqa::pix4d::parse;

#[test]
fn rounded_percentages_use_exact_display_precision() {
    for (calibrated, total, percent, valid) in [
        (1u64, 3u64, "33.33", true),
        (1, 3, "33.4", false),
        (1, 8, "12", true),
        (1, 8, "13", true),
        (1, 8, "12.49", false),
        (u64::MAX, u64::MAX, "100.00000000000000000000", true),
        (u64::MAX - 1, u64::MAX, "100.00000000000000000000", false),
    ] {
        let report = REPORT.replace("20 out of 20", &format!("{calibrated} out of {total}")).replace("(100%)", &format!("({percent}%)"));
        assert_eq!(parse(report.as_bytes(), "text/plain").metrics.calibrated_images.is_some(), valid, "{calibrated}/{total} {percent}");
    }
}
const REPORT: &str = r###"Generated with PIX4Dmapper version 4.10.0
Quality Report
Summary
Project
PRIVATE-GENERATED-NAME
Average Ground Sampling Distance (GSD)
2.50 cm / 0.98 in
Quality Check
Dataset
20 out of 20 images calibrated (100%), all images enabled
Georeferencing
yes, 2 GCPs (2 3D), mean RMS error = 0.200 US survey foot
Calibration Details
Number of Calibrated Images
20 out of 20
Geolocation Details
Ground Control Points
GCP Name
Accuracy XY/Z [US survey foot]
Error X [US survey foot]
Error Y [US survey foot]
Error Z [US survey foot]
Projection Error [pixel]
Verified/Marked
GENERATED-A (3D)
0.030/ 0.030
-0.1
0.2
-0.3
0.4
3 / 3
GENERATED-B (3D)
0.030/ 0.030
0.1
-0.2
0.3
0.4
3 / 3
Mean [US survey
foot]
-0.01
0.02
-0.03
Sigma [US survey foot]
0.11
0.12
0.13
RMS Error [US survey foot]
0.14
0.15
0.16
Localisation accuracy per GCP and mean errors in the three coordinate directions.
Absolute Geolocation Variance
Mean [US survey foot]
0.01
-0.02
-25.0
Sigma [US survey foot]
0.21
0.22
0.23
RMS Error [US survey foot]
0.24
0.25
25.1
The geolocation error is the difference between the initial and computed image positions.
Geolocation Bias
X Y Z
Translation [US survey foot]
0.01 -0.02 -24.9
Relative Geolocation Variance
Initial Processing Details
Coordinate Systems
Image Coordinate System
GENERATED-IMAGE-CRS
Ground Control Point (GCP) Coordinate System
GENERATED-CONTROL-CRS (ftUS)
Output Coordinate System
GENERATED-OUTPUT-CRS (ftUS)
Processing Options
Point Cloud Densification details
Time for Point Cloud Classification
NA
DSM, Orthomosaic and Index Details
Raster DTM
Generated: yes
DTM Resolution
5 x GSD (2.50 [cm/pixel])
Time for DTM Generation
01m:00s
"###;
#[test]
fn qualified_mapper_preserves_residual_roles() {
    let result = parse(REPORT.as_bytes(), "text/plain");
    assert_eq!(result.metrics.calibrated_images, Some(20));
    assert_eq!(result.metrics.rmse_z_feet, None);
    let evidence = result.report_evidence.expect("qualified evidence");
    assert_eq!(evidence.gsd_cm, Some(2.5));
    assert_eq!(evidence.gcp_count, Some(2));
    assert_eq!(evidence.gcp_fit.expect("gcp fit").rms_xyz, [0.14, 0.15, 0.16]);
    assert_eq!(evidence.camera_offsets.expect("camera offsets").mean_xyz, [0.01, -0.02, -25.0]);
    assert_eq!(evidence.output_crs.as_deref(), Some("GENERATED-OUTPUT-CRS (ftUS)"));
    assert_eq!(evidence.classification_time.as_deref(), Some("NA"));
}
#[test]
fn unqualified_sections_and_versions_are_rejected() {
    for report in [
        REPORT.replace("4.10.0", "4.10.1"),
        REPORT.replace("Quality Check", "UNKNOWN"),
        format!("{REPORT}Ground Control Points"),
        REPORT.replace("Absolute Geolocation Variance", "Check Points\nAbsolute Geolocation Variance"),
    ] {
        let result = parse(report.as_bytes(), "text/plain");
        assert!(result.report_evidence.is_none());
        assert!(result.diagnostics.iter().any(|d| d.code == "UNSUPPORTED_LAYOUT"));
    }
}
#[test]
fn bad_residual_and_contradictory_images_stay_unavailable() {
    for raw in ["-0.14", "nan", "inf", "1e3"] {
        let report = REPORT.replace("\n0.14\n", &format!("\n{raw}\n"));
        let result = parse(report.as_bytes(), "text/plain");
        let evidence = result.report_evidence.expect("evidence");
        assert!(evidence.gcp_fit.is_none());
        assert!(evidence.camera_offsets.is_some());
    }
    let report = REPORT.replace("Number of Calibrated Images\n20 out of 20", "Number of Calibrated Images\n19 out of 20");
    assert_eq!(parse(report.as_bytes(), "text/plain").metrics.calibrated_images, None);
}
