//! Ported from AutoGIS `tests/envmon/test_import_boring_logs.py`, plus the slice 6 additions
//! (workbook packages, wells, water levels, header aliases, inverted intervals).

use cadcraft_subsurface::import::{self, from_tables, validate};
use cadcraft_subsurface::{Cell, Finding, Severity, Table, sheet};
use std::path::PathBuf;

const LOC: &str =
    "BoringID,SiteID,LocationType,Northing,Easting,GroundElevation_ft,Status\nB-01,H281,Monitoring Well,4527893.12,293847.55,512.34,Complete\n";
const LITH: &str = "BoringID,TopDepth_ft,BottomDepth_ft,USCS,PrimaryMaterial,Color,Moisture,Description\nB-01,0.0,5.0,SM,Sandy loam,Brown,Moist,Fill\nB-01,5.0,12.0,CL,Clay,Gray,Wet,Native\n";
const SAMP: &str = "SampleID,BoringID,SampleType,TopDepth_ft,BottomDepth_ft,Matrix\nB-01-S1,B-01,Soil,3.0,4.5,SOIL\n";

fn csv(name: &str, text: &str) -> Table {
    sheet::read(text.as_bytes(), name).unwrap().remove(0)
}

fn codes(f: &[Finding]) -> Vec<&str> {
    f.iter().map(|f| f.code.as_str()).collect()
}

fn errors(f: &[Finding]) -> usize {
    f.iter().filter(|f| f.severity == Severity::Error).count()
}

#[test]
fn parse_boring_locations() {
    let (set, _) = from_tables(&[csv("boring_locations.csv", LOC)]);
    assert_eq!(set.borings.len(), 1);
    assert_eq!(set.borings[0].boring_id, "B-01");
    assert_eq!(set.borings[0].ground_elevation, Some(512.34));
    assert_eq!(set.borings[0].easting, Some(293847.55));
}

#[test]
fn parse_lithology_and_samples() {
    let (set, _) = from_tables(&[csv("lithology.csv", LITH), csv("samples.csv", SAMP)]);
    assert_eq!(set.lithology.len(), 2);
    assert_eq!(set.lithology[0].top_depth, 0.0);
    assert_eq!(set.lithology[1].bottom_depth, 12.0);
    assert_eq!(set.lithology[1].uscs, "CL");
    assert_eq!(set.samples.len(), 1);
    assert_eq!(set.samples[0].sample_id, "B-01-S1");
    assert_eq!(set.samples[0].top_depth, 3.0);
}

#[test]
fn rows_missing_depth_are_dropped_with_a_warning() {
    let lith = format!("{LITH}B-01,,,,,,,no depths\n");
    let (set, f) = from_tables(&[csv("boring_locations", LOC), csv("lithology", &lith)]);
    assert_eq!(set.lithology.len(), 2);
    let w = f.iter().find(|f| f.code == "lithology_rows_dropped_missing_depth").unwrap();
    assert_eq!(w.severity, Severity::Warning);
    assert_eq!(w.source, Some(("lithology".into(), 4)));
}

#[test]
fn lithology_boring_not_in_locations() {
    let (_, f) = from_tables(&[csv("boring_locations", LOC), csv("lithology", &LITH.replace("B-01,0.0", "B-99,0.0"))]);
    assert!(codes(&f).contains(&"lithology_boring_not_in_locations"));
}

#[test]
fn overlapping_intervals() {
    let lith = LITH.replace("B-01,0.0,5.0", "B-01,0.0,6.0");
    let (_, f) = from_tables(&[csv("boring_locations", LOC), csv("lithology", &lith)]);
    assert!(codes(&f).contains(&"overlapping_intervals"));
    assert_eq!(errors(&f), 1);
}

#[test]
fn clean_package_has_no_errors() {
    let (set, f) = from_tables(&[csv("boring_locations", LOC), csv("lithology", LITH), csv("samples", SAMP)]);
    assert_eq!(errors(&f), 0, "{f:?}");
    assert_eq!(f.last().map(|f| f.code.as_str()), Some("validation_complete"));
    assert_eq!(validate(&set).len(), 1);
}

#[test]
fn short_location_row_does_not_crash() {
    let (set, _) = from_tables(&[csv("boring_locations", "BoringID,SiteID,LocationType,Northing,Easting,GroundElevation_ft,Status\nB-01,H281\n")]);
    assert_eq!(set.borings.len(), 1);
    assert_eq!(set.borings[0].status, "");
    assert_eq!(set.borings[0].ground_elevation, None);
}

#[test]
fn missing_ids_duplicates_inverted_orphans_and_coordinates() {
    let loc = "Boring ID,Northing,Easting\nB-1,10,20\nB-1,11,21\nB-2,,\n,1,1\n";
    let lith = "boring id,Top Depth,Bottom Depth\nB-1,5,2\n";
    let wells = "BoringID,Component_Type,Top,Bottom\nB-3,Screen,10,20\n";
    let water = "BoringID,Observation_DateTime,Observation_Type,DTW\nB-4,2026-01-02,static,7.5\n";
    let (set, f) = from_tables(&[csv("Borings", loc), csv("Lithology", lith), csv("well_construction", wells), csv("water_levels", water)]);
    let c = codes(&f);
    for want in [
        "missing_table",
        "missing_boring_id",
        "duplicate_boring_id",
        "missing_coordinates",
        "inverted_interval",
        "well_boring_not_in_locations",
        "water_boring_not_in_locations",
    ] {
        assert!(c.contains(&want), "{want} not in {c:?}");
    }
    let missing = f.iter().find(|f| f.code == "missing_boring_id").unwrap();
    assert_eq!(missing.source, Some(("Borings".into(), 5)));
    assert_eq!(set.water[0].depth_to_water, Some(7.5));
    assert_eq!((set.water[0].observed.as_str(), set.water[0].kind.as_str()), ("2026-01-02", "static"));
    assert_eq!((set.water[0].observed.as_str(), set.water[0].kind.as_str()), ("2026-01-02", "static"));
    assert_eq!(set.wells[0].component, "Screen");
}

#[test]
fn workbook_package_matches_csv_package() {
    let tables = [csv("boring_locations", LOC), csv("lithology", LITH), csv("samples", SAMP)];
    let dir = tmp("workbook");
    let wb = dir.join("package.xlsx");
    std::fs::write(&wb, sheet::write_xlsx(&tables).unwrap()).unwrap();
    let (from_book, f1) = import::load(&wb).unwrap();

    let folder = dir.join("csv");
    std::fs::create_dir_all(&folder).unwrap();
    for (name, text) in [("boring_locations.csv", LOC), ("lithology.csv", LITH), ("samples.csv", SAMP), ("notes.txt", "ignored")] {
        std::fs::write(folder.join(name), text).unwrap();
    }
    let (from_dir, f2) = import::load(&folder).unwrap();
    assert_eq!(from_book, from_dir);
    assert_eq!(f1, f2);
    assert_eq!(from_dir.lithology.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_is_an_io_error() {
    assert!(matches!(import::load(&tmp("none").join("missing.xlsx")), Err(import::ImportError::Io(_))));
}

#[test]
fn numbers_and_text_cells_both_parse() {
    let t = Table {
        name: "lithology".into(),
        rows: vec![
            vec![Cell::Text("BoringID".into()), Cell::Text("TopDepth_ft".into()), Cell::Text("BottomDepth_ft".into())],
            vec![Cell::Text("B-1".into()), Cell::Number(0.0), Cell::Text(" 2.5 ".into())],
        ],
    };
    let (set, _) = from_tables(&[t]);
    assert_eq!(set.lithology[0].bottom_depth, 2.5);
}

#[test]
fn package_folder_shares_one_byte_budget() {
    let dir = tmp("budget");
    // One cell per file, so only the shared byte budget can trip.
    let half = "a".repeat(sheet::MAX_BYTES / 2 + 1);
    for n in ["a.csv", "b.csv"] {
        std::fs::write(dir.join(n), &half).unwrap();
    }
    assert!(matches!(import::load(&dir), Err(import::ImportError::Sheet(sheet::SheetError::TooLarge(m))) if m.contains("package limit")));
    let _ = std::fs::remove_dir_all(&dir);
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cadcraft-subsurface-import-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
