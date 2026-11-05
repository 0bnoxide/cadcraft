use cadcraft_surface::interpolate::{self, Method, Options};
fn wells() -> Vec<[f64; 3]> {
    vec![[0., 0., 10.], [10., 0., 20.], [10., 10., 40.], [0., 10., 30.]]
}
#[test]
fn exact_samples_symmetry_and_natural_neighbor_linear_precision() {
    let q = [[0., 0.], [5., 5.], [20., 20.]];
    for method in [Method::Idw, Method::NaturalNeighbor] {
        let values = interpolate::at_points(&wells(), &q, &Options { method, ..Options::default() }).unwrap();
        assert_eq!(values[0], Some(10.));
        assert!((values[1].unwrap() - 25.).abs() < 1e-9);
        assert_eq!(values[2], None);
    }
}
#[test]
fn gradient_downhill_azimuth_and_fit_quality() {
    let g = interpolate::fit_plane_gradient(&wells()).unwrap().unwrap();
    assert!((g.downhill[0] + 1.).abs() < 1e-10);
    assert!((g.downhill[1] + 2.).abs() < 1e-10);
    assert!((g.magnitude - 5f64.sqrt()).abs() < 1e-10);
    assert!((g.azimuth - 206.565051177).abs() < 1e-8);
    assert!((g.r_squared - 1.).abs() < 1e-12);
    assert!(interpolate::fit_plane_gradient(&[[0., 0., 0.], [1., 1., 1.], [2., 2., 2.]]).unwrap().is_none());
    assert!(interpolate::fit_plane_gradient(&[[0., 0., 1.], [1., 0., 1.], [0., 1., 1.]]).unwrap().is_none());
}
#[test]
fn duplicate_conflicts_minimum_and_radius() {
    let mut w = wells();
    w.push([0., 0., 99.]);
    assert!(interpolate::at_points(&w, &[[1., 1.]], &Options::default()).is_err());
    assert!(interpolate::at_points(&w[..2], &[[1., 1.]], &Options::default()).is_err());
    let v = interpolate::at_points(&wells(), &[[5., 5.]], &Options { radius: Some(1.), ..Options::default() }).unwrap();
    assert_eq!(v, vec![None]);
    let triangle = [[0., 0., 0.], [2., 0., 10.], [0., 2., 20.]];
    let values = interpolate::at_points(&triangle, &[[0.5, 0.5]], &Options::default()).unwrap();
    assert!((values[0].unwrap() - 30. / 7.).abs() < 1e-12);
    let nearest = interpolate::at_points(&triangle, &[[0.5, 0.5]], &Options { max_neighbors: 1, ..Options::default() }).unwrap();
    assert_eq!(nearest, vec![Some(0.)]);
}

#[test]
fn signed_zero_duplicates_are_coalesced_and_large_coordinate_plane_is_stable() {
    let wells = vec![[-0., 0., 1.], [-0., 1., 2.], [0., 0., 1.], [1., 0., 3.]];
    assert_eq!(interpolate::at_points(&wells, &[[0., 0.]], &Options::default()).unwrap(), vec![Some(1.)]);
    let shifted: Vec<_> = self::wells().into_iter().map(|[x, y, z]| [x + 500_000., y + 4_000_000., z]).collect();
    let g = interpolate::fit_plane_gradient(&shifted).unwrap().unwrap();
    assert!((g.downhill[0] + 1.).abs() < 1e-10);
    assert!((g.downhill[1] + 2.).abs() < 1e-10);
}

#[test]
fn grid_preserves_missing_nodes_and_tin_interpolation_preserves_topology() {
    let g = interpolate::grid(&wells(), [[0., 0.], [10., 10.]], 5., &Options::default()).unwrap();
    assert_eq!((g.width, g.height), (3, 3));
    let target = interpolate::grid_tin(&g).unwrap();
    assert_eq!(target.triangles.len(), 8);
    let filled = interpolate::onto_tin(&wells(), &target, &Options { method: Method::NaturalNeighbor, ..Options::default() }).unwrap();
    assert_eq!(filled.triangles, target.triangles);
    for [x, y, z] in filled.vertices {
        assert!((z - (10. + x + 2. * y)).abs() < 1e-9);
    }
    let mut missing = g.clone();
    missing.values[4] = None;
    assert!(interpolate::grid_tin(&missing).unwrap().triangles.is_empty());
    assert!(interpolate::grid(&wells(), [[0., 0.], [10., 10.]], 1e-50, &Options::default()).is_err());
}
