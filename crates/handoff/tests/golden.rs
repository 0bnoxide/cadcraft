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

/// A reparse point is refused whether the ZIP says it came from MS-DOS or NTFS.
#[test]
fn ntfs_reparse_entry_is_not_regular() {
    let mut bytes = std::fs::read(fixtures().join("invalid/dos-reparse-entry.zip")).unwrap();
    // Set "version made by" host (high byte) to NTFS (10) in every central-directory record.
    let mut at = 0;
    while at + 6 <= bytes.len() {
        if bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
            bytes[at + 5] = 10;
        }
        at += 1;
    }
    let report = cadcraft_handoff::validate(std::io::Cursor::new(bytes), false).unwrap();
    assert_eq!(report.issues.first().map(|i| i.code), Some("ZIP007"));
}

/// One entry for [`build_zip`]: name, compression method, and the stored data span.
struct Part {
    name: &'static str,
    method: u16,
    span: Vec<u8>,
    plain: Vec<u8>,
}

/// Writes a minimal ZIP; `central_slack` bytes are appended inside the central directory.
fn build_zip(parts: &[Part], central_slack: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for p in parts {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(&p.plain);
        let fields = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            for v in [20u16, 0, p.method, 0, 0] {
                h.extend_from_slice(&v.to_le_bytes());
            }
            for v in [crc, p.span.len() as u32, p.plain.len() as u32] {
                h.extend_from_slice(&v.to_le_bytes());
            }
            h.extend_from_slice(&(p.name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            if central {
                for v in [0u16, 0, 0] {
                    h.extend_from_slice(&v.to_le_bytes());
                }
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(p.name.as_bytes());
            h
        };
        out.extend(fields(0x0403_4b50, false));
        out.extend_from_slice(&p.span);
        central.extend(fields(0x0201_4b50, true));
    }
    central.extend(std::iter::repeat_n(0u8, central_slack));
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for v in [0u16, 0, parts.len() as u16, parts.len() as u16] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The valid fixture's entries, with the manifest deflated and `junk` appended to its span.
fn rebuilt(junk: &[u8]) -> Vec<Part> {
    use std::io::{Read, Write};
    let bytes = std::fs::read(fixtures().join("valid/known-vertical-datum.zip")).unwrap();
    let entry = |at: usize, len: usize| bytes[at..at + len].to_vec();
    let manifest = entry(30 + "handoff.json".len(), 636);
    let surface = entry(678 + 30 + "surface.landxml".len(), 558);
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&manifest).unwrap();
    let mut span = encoder.finish().unwrap();
    let mut check = Vec::new();
    flate2::read::DeflateDecoder::new(&span[..]).read_to_end(&mut check).unwrap();
    assert_eq!(check, manifest);
    span.extend_from_slice(junk);
    vec![
        Part { name: "handoff.json", method: 8, span, plain: manifest },
        Part { name: "surface.landxml", method: 0, span: surface.clone(), plain: surface },
    ]
}

fn primary(zip: Vec<u8>) -> (Status, Option<&'static str>) {
    let report = cadcraft_handoff::validate(std::io::Cursor::new(zip), false).unwrap();
    (report.status, report.issues.first().map(|i| i.code))
}

#[test]
fn rebuilt_package_is_valid() {
    assert_eq!(primary(build_zip(&rebuilt(&[]), 0)), (Status::Valid, None));
}

/// Bytes after the end of a DEFLATE stream, inside its declared span, could hide data.
#[test]
fn trailing_bytes_after_deflate_stream_are_refused() {
    assert_eq!(primary(build_zip(&rebuilt(b"PK\x03\x04hidden"), 0)), (Status::Invalid, Some("ZIP001")));
}

/// Central-directory bytes beyond the declared records could hide entries from this reader.
#[test]
fn unparsed_central_directory_bytes_are_refused() {
    assert_eq!(primary(build_zip(&rebuilt(&[]), 46)), (Status::Invalid, Some("ZIP001")));
}

/// A FIFO is refused at open without blocking.
#[cfg(unix)]
#[test]
fn fifo_path_is_refused_without_blocking() {
    let dir = std::env::temp_dir().join(format!("cadcraft-handoff-fifo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = dir.join("package.zip");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(made.success());
    let err = validate_file(&fifo).unwrap_err();
    assert!(err.to_string().contains("not a regular file"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
