use cadcraft_subsurface::{Boring, BoringSet, profile};

#[test]
fn signed_stations_do_not_collapse_borings_near_the_line_ends() {
    let a = [0.0, 0.0];
    let b = [100.0, 0.0];
    assert_eq!(profile::project([30.0, 0.0], a, b).unwrap(), (30.0, 0.0));
    assert_eq!(profile::project([-2.0, 0.0], a, b).unwrap(), (-2.0, 2.0));
    assert_eq!(profile::project([103.0, 0.0], a, b).unwrap(), (103.0, 3.0));
    let set = BoringSet {
        borings: [("BEFORE", -2.0), ("AFTER", 103.0), ("FAR", 2000.0)]
            .into_iter()
            .map(|(id, x)| Boring { boring_id: id.into(), easting: Some(x), northing: Some(0.0), ..Default::default() })
            .collect(),
        ..Default::default()
    };
    let (placements, warnings) = profile::place(&set, a, b, 5.0).unwrap();
    assert_eq!(placements.iter().map(|p| p.station).collect::<Vec<_>>(), vec![-2.0, 103.0]);
    assert!(warnings.iter().any(|w| w.contains("boring_outside_tolerance") && w.contains("FAR")));
}

#[test]
fn missing_coordinates_are_reported_and_not_placed() {
    let set = BoringSet { borings: vec![Boring { boring_id: "MISSING".into(), ..Default::default() }], ..Default::default() };
    let (placements, warnings) = profile::place(&set, [0.0, 0.0], [10.0, 0.0], 5.0).unwrap();
    assert!(placements.is_empty());
    assert!(warnings.iter().any(|w| w.contains("missing_coordinates") && w.contains("MISSING")));
}
