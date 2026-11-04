//! The AutoGIS-Civil3D golden fixtures: every package gets the status and primary issue code
//! the reference .NET validator gives it.

use std::path::{Path, PathBuf};

use cadcraft_handoff::{Status, validate_file};

const CASES: &[(&str, Status, Option<&str>)] = &[
    ("valid/known-vertical-datum.zip", Status::Valid, None),
    ("valid/unknown-vertical-datum.zip", Status::ValidWithWarnings, Some("WRN001")),
    ("invalid/malformed-archive.zip", Status::Invalid, Some("ZIP001")),
    ("invalid/missing-surface.zip", Status::Invalid, Some("ZIP003")),
    ("invalid/extra-entry.zip", Status::Invalid, Some("ZIP004")),
    ("invalid/unsafe-path.zip", Status::Invalid, Some("ZIP005")),
    ("invalid/case-collision.zip", Status::Invalid, Some("ZIP006")),
    ("invalid/symlink-entry.zip", Status::Invalid, Some("ZIP007")),
    ("invalid/dos-reparse-entry.zip", Status::Invalid, Some("ZIP007")),
    ("invalid/encrypted-entry.zip", Status::Invalid, Some("ZIP008")),
    ("invalid/unsupported-compression.zip", Status::Invalid, Some("ZIP009")),
    ("invalid/manifest-too-large.zip", Status::Invalid, Some("ZIP010")),
    ("invalid/surface-too-large-declared.zip", Status::Invalid, Some("ZIP011")),
    ("invalid/compression-ratio.zip", Status::Invalid, Some("ZIP012")),
    ("invalid/manifest-invalid-json.zip", Status::Invalid, Some("MAN001")),
    ("invalid/manifest-missing-field.zip", Status::Invalid, Some("MAN002")),
    ("invalid/manifest-unknown-property.zip", Status::Invalid, Some("MAN002")),
    ("invalid/manifest-version.zip", Status::Invalid, Some("MAN002")),
    ("invalid/manifest-wrong-filename.zip", Status::Invalid, Some("MAN002")),
    ("invalid/manifest-timestamp.zip", Status::Invalid, Some("MAN003")),
    ("invalid/checksum.zip", Status::Invalid, Some("INT001")),
    ("invalid/xml-malformed.zip", Status::Invalid, Some("XML001")),
    ("invalid/xml-dtd.zip", Status::Invalid, Some("XML002")),
    ("invalid/xml-version.zip", Status::Invalid, Some("XML003")),
    ("invalid/xml-no-surface.zip", Status::Invalid, Some("XML004")),
    ("invalid/xml-multiple-surfaces.zip", Status::Invalid, Some("XML004")),
    ("invalid/xml-multiple-definitions.zip", Status::Invalid, Some("XML005")),
    ("invalid/xml-invalid-point.zip", Status::Invalid, Some("XML006")),
    ("invalid/xml-duplicate-point-id.zip", Status::Invalid, Some("XML007")),
    ("invalid/xml-nonfinite-coordinate.zip", Status::Invalid, Some("XML008")),
    ("invalid/xml-invalid-face.zip", Status::Invalid, Some("XML009")),
    ("invalid/xml-unknown-point-reference.zip", Status::Invalid, Some("XML010")),
    ("invalid/xml-repeated-face-vertex.zip", Status::Invalid, Some("XML011")),
    ("invalid/xml-degenerate-face.zip", Status::Invalid, Some("XML012")),
    ("invalid/surface-name-mismatch.zip", Status::Invalid, Some("XCK001")),
    ("invalid/point-count-mismatch.zip", Status::Invalid, Some("XCK002")),
    ("invalid/face-count-mismatch.zip", Status::Invalid, Some("XCK003")),
    ("invalid/epsg-mismatch.zip", Status::Invalid, Some("XCK004")),
    ("invalid/horizontal-unit-mismatch.zip", Status::Invalid, Some("XCK005")),
    ("invalid/vertical-unit-family-mismatch.zip", Status::Invalid, Some("XCK006")),
    ("invalid/vertical-direction-invalid.zip", Status::Invalid, Some("MAN002")),
    ("invalid/vertical-datum-invalid.zip", Status::Invalid, Some("MAN002")),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1")
}

#[test]
fn every_fixture_matches_the_reference_validator() {
    let mut failures = Vec::new();
    for (path, status, code) in CASES {
        let report = validate_file(&fixtures().join(path)).map_err(|e| format!("{path}: {e}")).unwrap();
        let primary = report.issues.first().map(|i| i.code);
        if report.status != *status || primary != *code {
            failures.push(format!("{path}: got {:?} {:?}, want {status:?} {code:?} ({:?})", report.status, primary, report.issues));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_fixture_is_covered() {
    let mut on_disk = Vec::new();
    for dir in ["valid", "invalid"] {
        for entry in std::fs::read_dir(fixtures().join(dir)).unwrap() {
            on_disk.push(format!("{dir}/{}", entry.unwrap().file_name().to_string_lossy()));
        }
    }
    on_disk.sort();
    let mut listed: Vec<String> = CASES.iter().map(|(p, _, _)| (*p).to_owned()).collect();
    listed.sort();
    assert_eq!(on_disk, listed);
}

#[test]
fn valid_package_reports_metadata_and_exit_code() {
    let report = validate_file(&fixtures().join("valid/known-vertical-datum.zip")).unwrap();
    assert_eq!(report.status.exit_code(), 0);
    let m = report.metadata.clone().unwrap();
    assert_eq!(m.package_id, "9a8ff271-b0d8-46db-809d-a6f72954af20");
    assert_eq!((m.surface_name.as_str(), m.point_count, m.face_count, m.epsg_code), ("Existing Ground", 3, 1, 26913));
    assert!(report.to_text().starts_with("Status: VALID\nIssues:\n- None\n"));
    assert_eq!(validate_file(&fixtures().join("valid/unknown-vertical-datum.zip")).unwrap().status.exit_code(), 2);
    assert_eq!(validate_file(&fixtures().join("invalid/checksum.zip")).unwrap().status.exit_code(), 1);
}

#[test]
fn read_surface_returns_the_tin() {
    let report = cadcraft_handoff::read_surface_file(&fixtures().join("valid/known-vertical-datum.zip")).unwrap();
    let tin = report.surface.unwrap();
    assert_eq!(tin.name, "Existing Ground");
    assert_eq!(tin.points.len(), 3);
    assert_eq!(tin.faces, vec![[1, 2, 3]]);
    assert_eq!((tin.points[1].northing, tin.points[1].easting, tin.points[1].elevation), (0.0, 10.0, 101.0));
}

#[test]
fn missing_file_is_operational_error() {
    assert!(validate_file(&fixtures().join("nope.zip")).is_err());
}

/// Hostile input never panics: every fixture with each byte flipped, and truncated at each length.
#[test]
fn mutated_packages_never_panic() {
    for (path, _, _) in CASES {
        let original = std::fs::read(fixtures().join(path)).unwrap();
        for at in 0..original.len() {
            let mut bytes = original.clone();
            bytes[at] ^= 0xff;
            let _ = cadcraft_handoff::validate(std::io::Cursor::new(bytes), true);
            let _ = cadcraft_handoff::validate(std::io::Cursor::new(original[..at].to_vec()), true);
        }
    }
}
