use cadcraft_surface::{
    Tin,
    compare::{compare_points, compare_vertices, elevations},
};

fn plane() -> Tin {
    Tin { vertices: vec![[0., 0., 0.], [10., 0., 10.], [0., 10., 20.]], triangles: vec![[0, 1, 2]] }
}

#[test]
fn interpolates_a_plane_and_counts_edges_with_xy_distance_tolerance() {
    let samples = elevations(&plane(), &[[2., 3.], [5., 5.], [0., 0.], [-5e-10, 0.], [-2e-9, 0.], [10., 10.]]).unwrap();
    assert_eq!(samples[0], Some(8.));
    assert_eq!(samples[1], Some(15.));
    assert_eq!(samples[2], Some(0.));
    assert_eq!(samples[3], Some(0.)); // near-edge point is clamped to the surface
    assert_eq!(samples[4], None);
    assert_eq!(samples[5], None);
    // Tolerance must not scale with triangle size, nor extend a corner by sqrt(2).
    assert!(elevations(&plane(), &[[-8e-10, -8e-10]]).unwrap()[0].is_none());
    let large = Tin { vertices: vec![[0., 0., 0.], [1e6, 0., 0.], [0., 1e6, 0.]], triangles: vec![[0, 1, 2]] };
    assert_eq!(elevations(&large, &[[-1e-6, 1.]]).unwrap(), vec![None]);
}

#[test]
fn difference_is_target_minus_reference_and_outside_is_not_in_statistics() {
    let report = compare_points(&plane(), &[[0., 0., 2.], [2., 3., 4.], [20., 20., 1000.]]).unwrap();
    assert_eq!(report.differences, vec![Some(2.), Some(-4.), None]);
    assert_eq!(report.statistics.count, 2);
    assert_eq!(report.statistics.outside, 1);
    assert_eq!(report.statistics.min, Some(-4.));
    assert_eq!(report.statistics.max, Some(2.));
    assert_eq!(report.statistics.mean, Some(-1.));
    assert!((report.statistics.rms.unwrap() - 10f64.sqrt()).abs() < 1e-12);
}

#[test]
fn comparison_at_another_tins_vertices_includes_unused_vertices() {
    let other = Tin { vertices: vec![[0., 0., 3.], [10., 0., 13.], [0., 10., 23.], [20., 20., 7.]], triangles: vec![[0, 1, 2]] };
    let report = compare_vertices(&plane(), &other).unwrap();
    assert_eq!(report.differences, vec![Some(3.), Some(3.), Some(3.), None]);
    assert_eq!(report.statistics.mean, Some(3.));
    assert!((report.statistics.rms.unwrap() - 3.).abs() < 1e-12);
    assert_eq!(report.statistics.outside, 1);
}

#[test]
fn empty_overlap_has_absent_statistics_not_nan_or_zero() {
    let report = compare_points(&plane(), &[[20., 20., 1.]]).unwrap();
    assert_eq!(report.statistics.count, 0);
    assert_eq!(report.statistics.outside, 1);
    assert_eq!(report.statistics.min, None);
    assert_eq!(report.statistics.max, None);
    assert_eq!(report.statistics.mean, None);
    assert_eq!(report.statistics.rms, None);
}

#[test]
fn rejects_hostile_geometry_and_queries_without_panicking() {
    assert!(elevations(&plane(), &[[f64::NAN, 0.]]).is_err());
    assert!(compare_points(&plane(), &[[0., 0., f64::INFINITY]]).is_err());
    let mut tin = plane();
    tin.triangles[0][0] = u32::MAX;
    assert!(elevations(&tin, &[[0., 0.]]).is_err());
    tin.triangles = vec![[0, 0, 1]];
    assert!(elevations(&tin, &[[0., 0.]]).is_err());
    assert_eq!(elevations(&Tin::default(), &[[0., 0.]]).unwrap(), vec![None]);
}

#[test]
fn large_comparison_returns_a_work_limit_error_before_sampling() {
    let mut tin = plane();
    tin.triangles = vec![[0, 1, 2]; 4000];
    let points = vec![[0., 0.]; 4000];
    assert!(elevations(&tin, &points).unwrap_err().to_string().contains("limit"));
}
