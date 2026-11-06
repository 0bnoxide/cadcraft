use cadcraft_subsurface::{
    Boring, BoringSet, Cell,
    boring::{Lithology, Sample, WaterLevel, WellComponent},
    profile, report,
    sticklog::{self, Mark},
};

fn fixture() -> BoringSet {
    BoringSet {
        borings: vec![
            Boring { boring_id: "B-1".into(), easting: Some(50.0), northing: Some(10.0), ground_elevation: Some(100.0), ..Default::default() },
            Boring { boring_id: "EMPTY".into(), ..Default::default() },
        ],
        lithology: vec![
            Lithology { boring_id: "B-1".into(), top_depth: 5.0, bottom_depth: 12.0, uscs: "SM".into(), pid_ppm: Some(3.2), ..Default::default() },
            Lithology { boring_id: "B-1".into(), top_depth: 0.0, bottom_depth: 5.0, uscs: "CL".into(), ..Default::default() },
        ],
        samples: vec![Sample { boring_id: "B-1".into(), sample_id: "S-1".into(), top_depth: 6.0, bottom_depth: 50.0, ..Default::default() }],
        wells: vec![WellComponent { boring_id: "B-1".into(), component: "screen".into(), top_depth: 7.0, bottom_depth: 30.0, ..Default::default() }],
        water: vec![WaterLevel { boring_id: "B-1".into(), depth_to_water: Some(42.0), ..Default::default() }],
    }
}

#[test]
fn overlays_and_pid_are_drawn_at_scaled_depths() {
    let set = fixture();
    let log = sticklog::draw(&set, "B-1", [0.0, 100.0], 2.0).unwrap();
    assert_eq!(log.depth, 50.0);
    assert!(log.marks.iter().any(|m| matches!(m,Mark::Text{text,..} if text.contains("PID 3.2 ppm"))));
    assert!(log.marks.iter().any(|m| matches!(m,Mark::Line{layer:"BORING-WATER",a,..} if a[1]==16.0)));
    assert!(log.marks.iter().any(|m| matches!(m,Mark::Box{layer:"BORING-SAMPLE",at,..} if at[1]==0.0)));
    assert!(sticklog::draw(&set, "EMPTY", [0.0, 0.0], 1.0).unwrap().marks.is_empty());
    assert!(
        sticklog::draw(&set, "EMPTY", [0.0, 0.0], 1.0).unwrap().warnings.iter().any(|w| w.contains("sticklog_no_lithology") && w.contains("EMPTY"))
    );
    assert_eq!(sticklog::hatch("sp-sm"), "DOTS");
    assert_eq!(sticklog::hatch("CL"), "ANSI31");
}

#[test]
fn hostile_depth_and_coordinate_overflow_are_errors() {
    let mut set = fixture();
    set.lithology[0].top_depth = f64::NAN;
    assert!(sticklog::draw(&set, "B-1", [0.0, 0.0], 1.0).is_err());
    assert!(sticklog::draw(&fixture(), "B-1", [0.0, 0.0], f64::MAX).is_err());
    assert!(profile::project([0.0, 0.0], [1.0, 1.0], [1.0, 1.0]).is_err());
    let error = sticklog::interval("B-BAD", -1.0, 2.0).unwrap_err();
    assert!(error.contains("B-BAD") && error.contains("-1") && error.contains('2'));
}

#[test]
fn profile_tolerance_missing_elevation_and_exaggeration() {
    let mut set = fixture();
    set.borings.push(Boring { boring_id: "FAR".into(), easting: Some(2000.0), northing: Some(0.0), ..Default::default() });
    let (placements, warnings) = profile::place(&set, [0.0, 0.0], [100.0, 0.0], 50.0).unwrap();
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].station, 50.0);
    assert_eq!(placements[0].offset, 10.0);
    assert!(warnings.iter().any(|w| w.contains("FAR")));
    let log = profile::draw(&set, [0.0, 0.0], [100.0, 0.0], 50.0, [0.0, 0.0], 2.0).unwrap();
    assert!(log.marks.iter().any(|m| matches!(m,Mark::Box{at,..} if at[0]==50.0 && at[1]==190.0)));
    set.borings[0].ground_elevation = None;
    assert!(
        profile::draw(&set, [0.0, 0.0], [100.0, 0.0], 50.0, [0.0, 0.0], 1.0)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("boring_missing_ground_elevation"))
    );
}

#[test]
fn profile_endpoint_modes_are_exclusive_and_resolve_boring_coordinates() {
    let mut set = fixture();
    set.borings.push(Boring { boring_id: "B-2".into(), easting: Some(100.0), northing: Some(0.0), ..Default::default() });
    assert_eq!(profile::endpoints(&set, None, None, Some("B-1"), Some("B-2")).unwrap(), ([50.0, 10.0], [100.0, 0.0]));
    assert!(profile::endpoints(&set, Some([0.0, 0.0]), None, Some("B-1"), None).is_err());
    assert!(profile::endpoints(&set, None, None, Some("MISSING"), Some("B-2")).is_err());
    assert!(profile::endpoints(&set, None, None, None, None).is_err());
    assert!(profile::endpoints(&set, None, None, Some("EMPTY"), Some("B-2")).is_err());
}

#[test]
fn report_sorts_depth_and_uses_overlapping_pid() {
    let tables = report::build(&fixture()).unwrap().tables;
    let lith = tables.iter().find(|t| t.name == "B-1 lithology").unwrap();
    assert_eq!(lith.cell(1, 0), &Cell::Number(0.0));
    let summary = tables.last().unwrap();
    assert_eq!(summary.cell(1, 8), &Cell::Number(3.2));
    assert!(tables.iter().any(|t| t.name == "EMPTY header"));
}
