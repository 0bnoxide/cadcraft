use cadcraft_surface::audit::*;
use cadcraft_surface::{Region, Tin};

fn pt(number: &str, x: f64, y: f64, z: f64, description: &str) -> SurveyPoint {
    SurveyPoint { number: number.into(), xyz: [x, y, z], description: description.into() }
}

fn codes(a: &Audit) -> Vec<&'static str> {
    a.issues.iter().map(|i| i.code).collect()
}

fn square(size: f64) -> Region {
    Region { outer: vec![[0., 0.], [size, 0.], [size, size], [0., size]], holes: vec![] }
}

/// A 5 x 5 grid of points on the plane z = 100 + x / 10.
fn grid() -> Vec<SurveyPoint> {
    (0..25).map(|i| pt(&(i + 1).to_string(), f64::from(i % 5) * 10., f64::from(i / 5) * 10., 100. + f64::from(i % 5), "EG")).collect()
}

#[test]
fn clean_points_pass_every_check() {
    let opts = PreflightOptions {
        keys: Some(vec!["eg".into(), "TOP*".into()]),
        elevation_band: Some((90., 110.)),
        neighbor_tolerance: Some(2.),
        extent: Some(square(40.)),
        points_crs: Some("EPSG:26913".into()),
        expected_crs: Some("epsg:26913 ".into()),
    };
    let a = preflight(&grid(), &opts);
    assert!(a.issues.is_empty(), "{:?}", a.issues);
    assert_eq!(a.receipt.check, "preflight");
    assert_eq!(a.receipt.version, VERSION);
    assert_eq!(a.receipt.counts.get("points"), Some(&25));
    assert_eq!(a.receipt.counts.get("issues"), Some(&0));
    assert_eq!(a.receipt.thresholds.get("neighbor_tolerance"), Some(&2.));
}

#[test]
fn each_preflight_check_reports_its_code() {
    let mut pts = grid();
    pts.push(pt("7", 5., 5., 101., "TOPBANK")); // duplicate number, keyed by wildcard
    pts.push(pt("30", 15., 15., 500., "XX")); // unkeyed, out of band, spike above neighbours
    pts.push(pt("31", 99., 99., 100., "")); // no code, outside extent
    pts.push(pt("32", f64::NAN, 0., 100., "EG"));
    let opts = PreflightOptions {
        keys: Some(vec!["EG".into(), "TOP*".into()]),
        elevation_band: Some((90., 110.)),
        neighbor_tolerance: Some(5.),
        extent: Some(square(40.)),
        points_crs: Some("EPSG:2232".into()),
        expected_crs: Some("EPSG:26913".into()),
    };
    let a = preflight(&pts, &opts);
    let c = codes(&a);
    for code in [PF_CRS_MISMATCH, PF_DUP_NUMBER, PF_BAD_COORD, PF_ELEV_BAND, PF_OUTSIDE_EXTENT, PF_ELEV_NEIGHBOR] {
        assert_eq!(c.iter().filter(|&&x| x == code).count(), 1, "{code}: {c:?}");
    }
    assert_eq!(c.iter().filter(|&&x| x == PF_CODE_UNKEYED).count(), 2, "{c:?}");
    let spike = a.issues.iter().find(|i| i.code == PF_ELEV_NEIGHBOR).map(|i| i.refs.clone());
    assert_eq!(spike, Some(vec!["30".to_owned()]));
    assert_eq!(a.receipt.counts.get(PF_CODE_UNKEYED), Some(&2));
    assert_eq!(a.receipt.counts.get("errors"), Some(&4));
    assert_eq!(a.receipt.counts.get("warnings"), Some(&4));
    // Same inputs, same result.
    assert_eq!(preflight(&pts, &opts), a);
}

#[test]
fn unset_checks_do_not_run_and_empty_input_is_an_issue() {
    let a = preflight(&[pt("1", 0., 0., 1e9, "??")], &PreflightOptions::default());
    assert!(a.issues.is_empty());
    assert_eq!(a.receipt.inputs.get("keys").map(String::as_str), Some("none"));
    assert_eq!(codes(&preflight(&[], &PreflightOptions::default())), vec![PF_EMPTY]);
}

#[test]
fn hostile_points_do_not_panic() {
    let pts = vec![
        pt("1", f64::MAX, f64::MAX, 0., ""),
        pt("1", -f64::MAX, 0., f64::INFINITY, ""),
        pt("", 0., 0., 0., ""),
        pt("2", 0., 0., 5., ""),
        pt("3", 1e300, -1e300, 0., ""),
    ];
    let opts = PreflightOptions { neighbor_tolerance: Some(0.), keys: Some(vec![]), elevation_band: Some((1., 0.)), ..Default::default() };
    let a = preflight(&pts, &opts);
    assert!(codes(&a).contains(&PF_DUP_NUMBER));
}

fn tin(vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Tin {
    Tin { vertices, triangles }
}

fn flat() -> Tin {
    tin(vec![[0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.]], vec![[0, 1, 2], [0, 2, 3]])
}

#[test]
fn clean_tin_with_boundary_passes() {
    let a = audit_tin(&flat(), Some(&square(10.)), &[vec![[0., 0., 0.], [10., 10., 0.]]], &TinAuditOptions::default());
    assert!(a.issues.is_empty(), "{:?}", a.issues);
    assert_eq!(a.receipt.check, "tin");
    assert_eq!(a.receipt.counts.get("triangles"), Some(&2));
    assert_eq!(a.receipt.thresholds.get("max_slope_deg"), Some(&60.));
}

#[test]
fn missing_or_invalid_boundary_and_empty_tin() {
    let opts = TinAuditOptions::default();
    assert_eq!(codes(&audit_tin(&flat(), None, &[], &opts)), vec![TIN_NO_BOUNDARY]);
    let bad = Region { outer: vec![[0., 0.], [1., 1.]], holes: vec![] };
    assert_eq!(codes(&audit_tin(&flat(), Some(&bad), &[], &opts)), vec![TIN_NO_BOUNDARY]);
    assert_eq!(codes(&audit_tin(&Tin::default(), Some(&square(1.)), &[], &opts)), vec![TIN_EMPTY]);
}

#[test]
fn spikes_need_both_steep_and_short() {
    // A 0.5-wide sliver rising 10: steep (~88°) with a short edge.
    let spike = tin(vec![[0., 0., 0.], [0.5, 0., 10.], [0., 5., 0.]], vec![[0, 1, 2]]);
    let opts = TinAuditOptions::default();
    let a = audit_tin(&spike, Some(&square(10.)), &[], &opts);
    assert_eq!(codes(&a), vec![TIN_SPIKE]);
    assert_eq!(a.issues.first().map(|i| i.refs.clone()), Some(vec!["0".to_owned()]));
    // Same slope but long edges: a cliff, not a spike.
    let cliff = tin(vec![[0., 0., 0.], [5., 0., 100.], [0., 50., 0.]], vec![[0, 1, 2]]);
    assert!(audit_tin(&cliff, Some(&square(10.)), &[], &opts).issues.is_empty());
    // Short edge but gentle: not a spike.
    let gentle = tin(vec![[0., 0., 0.], [0.5, 0., 0.1], [0., 5., 0.]], vec![[0, 1, 2]]);
    assert!(audit_tin(&gentle, Some(&square(10.)), &[], &opts).issues.is_empty());
}

#[test]
fn bad_triangles_are_reported_not_panicked() {
    let t = tin(vec![[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [0., 1., f64::NAN]], vec![[0, 1, 9], [0, 0, 1], [0, 1, 2], [0, 1, 3]]);
    let a = audit_tin(&t, Some(&square(1.)), &[], &TinAuditOptions::default());
    assert_eq!(codes(&a), vec![TIN_BAD_TRIANGLE; 4]);
}

#[test]
fn crossing_breaklines_are_found_and_shared_vertices_are_not() {
    let opts = TinAuditOptions::default();
    let lines = vec![
        vec![[0., 0., 1.], [10., 10., 1.]],
        vec![[0., 10., 2.], [10., 0., 2.]],   // crosses line 0 at (5, 5)
        vec![[10., 10., 1.], [20., 10., 1.]], // continues line 0 from its end: fine
    ];
    let a = audit_tin(&flat(), Some(&square(10.)), &lines, &opts);
    assert_eq!(codes(&a), vec![TIN_BREAKLINE_CROSS]);
    let i = a.issues.first().cloned();
    assert_eq!(i.as_ref().map(|i| i.refs.clone()), Some(vec!["0".to_owned(), "1".to_owned()]));
    assert_eq!(i.and_then(|i| i.at), Some([5., 5., 1.]));
    // A line that loops back across itself.
    let bow = vec![vec![[0., 0., 0.], [10., 10., 0.], [10., 0., 0.], [0., 10., 0.]]];
    assert_eq!(codes(&audit_tin(&flat(), Some(&square(10.)), &bow, &opts)), vec![TIN_BREAKLINE_CROSS]);
    // A T junction (an end on another line's interior) counts.
    let tee = vec![vec![[0., 5., 0.], [10., 5., 0.]], vec![[5., 5., 0.], [5., 10., 0.]]];
    assert_eq!(codes(&audit_tin(&flat(), Some(&square(10.)), &tee, &opts)), vec![TIN_BREAKLINE_CROSS]);
}
