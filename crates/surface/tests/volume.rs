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

#[test]
fn tiny_projected_boundary_and_hole_keep_their_area() {
    let (x, y, size) = (500_000., 4_000_000., 0.01);
    let ring = vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]];
    let base = Tin { vertices: ring.iter().map(|&[x, y]| [x, y, 0.]).collect(), triangles: vec![[0, 1, 2], [0, 2, 3]] };
    let mut other = base.clone();
    for p in &mut other.vertices {
        p[2] = 2.;
    }
    let region = Region { outer: ring, holes: vec![] };
    let full = volume::calculate(&base, &other, std::slice::from_ref(&region), 0.001).unwrap();
    assert!((full.total.covered_area - size * size).abs() < 1e-10);
    let mut hole = region;
    hole.holes.push(vec![[x + 0.004, y + 0.004], [x + 0.006, y + 0.004], [x + 0.006, y + 0.006], [x + 0.004, y + 0.006]]);
    let report = volume::calculate(&base, &other, &[hole], 0.001).unwrap();
    assert!((report.total.covered_area - 0.000096).abs() < 1e-10);
    assert!((report.total.fill - 0.000192).abs() < 1e-10);
}

#[test]
fn ordinary_site_grid_does_not_require_scanning_every_face_for_every_cell() {
    let mut base = Tin::default();
    for y in 0..26 {
        for x in 0..26 {
            base.vertices.push([f64::from(x) * 4., f64::from(y) * 4., 0.]);
        }
    }
    for y in 0..25u32 {
        for x in 0..25u32 {
            let i = y * 26 + x;
            base.triangles.extend([[i, i + 1, i + 27], [i, i + 27, i + 26]]);
        }
    }
    let mut other = base.clone();
    for p in &mut other.vertices {
        p[2] = 5.;
    }
    let r = Region { outer: vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]], holes: vec![] };
    let report = volume::calculate(&base, &other, &[r], 1.).unwrap();
    assert_eq!(report.total.covered_area, 10000.);
    assert!((report.total.fill - 50000.).abs() < 1e-8);
}
