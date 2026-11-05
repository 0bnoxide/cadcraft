use cadcraft_surface::{Region, Tin, volume};

fn plane(offset: f64) -> Tin {
    Tin {
        vertices: vec![[0., 0., offset], [10., 0., 10. + offset], [10., 10., 30. + offset], [0., 10., 20. + offset]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    }
}
fn region() -> Region {
    Region { outer: vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]], holes: vec![] }
}
#[test]
fn sloped_offset_planes_give_analytic_cut_fill_and_regions() {
    let r = region();
    let report = volume::calculate(&plane(0.), &plane(3.), &[r.clone(), r], 1.).unwrap();
    assert_eq!(report.regions.len(), 2);
    assert!((report.total.fill - 600.).abs() < 1e-9);
    assert_eq!(report.total.cut, 0.);
    let cut = volume::calculate(&plane(3.), &plane(0.), &[region()], 1.).unwrap();
    assert!((cut.total.cut - 300.).abs() < 1e-9);
    assert!((cut.total.net + 300.).abs() < 1e-9);
}
#[test]
fn holes_missing_coverage_and_hostile_spacing() {
    let mut r = region();
    r.holes.push(vec![[4., 4.], [6., 4.], [6., 6.], [4., 6.]]);
    let report = volume::calculate(&plane(0.), &plane(2.), &[r], 1.).unwrap();
    assert_eq!(report.total.covered_area, 96.);
    assert!((report.total.fill - 192.).abs() < 1e-9);
    let empty = volume::calculate(&plane(0.), &Tin::default(), &[region()], 1.).unwrap();
    assert_eq!(empty.total.skipped_area, 100.);
    assert_eq!(empty.total.fill, 0.);
    for spacing in [0., -1., f64::NAN, 1e-100] {
        assert!(volume::calculate(&plane(0.), &plane(1.), &[region()], spacing).is_err());
    }
}

#[test]
fn rejects_crossed_boundaries() {
    let crossed = Region { outer: vec![[0., 0.], [10., 10.], [0., 10.], [10., 0.]], holes: vec![] };
    assert!(volume::calculate(&plane(0.), &plane(1.), &[crossed], 1.).is_err());
}

#[test]
fn crossing_planes_split_cut_and_fill_and_partial_coverage_is_skipped() {
    let mut base = plane(0.);
    for p in &mut base.vertices {
        p[2] = 0.;
    }
    let mut other = base.clone();
    for p in &mut other.vertices {
        p[2] = p[0] - 5.;
    }
    let report = volume::calculate(&base, &other, &[region()], 1.).unwrap();
    assert!((report.total.cut - 125.).abs() < 1e-9);
    assert!((report.total.fill - 125.).abs() < 1e-9);
    assert!(report.total.net.abs() < 1e-9);
    for p in &mut other.vertices {
        p[0] *= 0.5;
        p[2] = 2.;
    }
    let partial = volume::calculate(&base, &other, &[region()], 1.).unwrap();
    assert_eq!(partial.total.covered_area, 50.);
    assert_eq!(partial.total.skipped_area, 50.);
    assert!((partial.total.fill - 100.).abs() < 1e-9);
}
