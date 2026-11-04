use cadcraft_surface::{Tin, contour::contours};

fn slope() -> Tin {
    Tin { vertices: vec![[0., 0., 0.], [2., 0., 2.], [2., 2., 2.], [0., 2., 0.]], triangles: vec![[0, 1, 2], [0, 2, 3]] }
}

#[test]
fn chains_adjacent_triangles_and_marks_zero_based_index_levels() {
    let paths = contours(&slope(), 0.5, 2).unwrap();
    let middle: Vec<_> = paths.iter().filter(|p| p.elevation == 1.).collect();
    assert_eq!(middle.len(), 1);
    let path = middle[0];
    assert!(!path.closed);
    assert!(path.index);
    assert_eq!(path.points.len(), 3);
    assert!(path.points.iter().all(|p| p[0] == 1. && p[2] == 1.));
    assert!(path.points.contains(&[1., 0., 1.]));
    assert!(path.points.contains(&[1., 2., 1.]));
    assert!(!paths.iter().find(|p| p.elevation == 0.5).unwrap().index);
}

#[test]
fn hill_is_closed_without_repeating_first_vertex() {
    let tin = Tin {
        vertices: vec![[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [0., 2., 0.], [1., 1., 2.]],
        triangles: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
    };
    let paths = contours(&tin, 1., 5).unwrap();
    let ring = paths.iter().find(|p| p.elevation == 1.).unwrap();
    assert!(ring.closed);
    assert_eq!(ring.points.len(), 4);
    assert_ne!(ring.points.first(), ring.points.last());
    assert!(!paths.iter().any(|p| p.elevation == 2.)); // isolated peak is not a line
}

#[test]
fn flat_faces_are_skipped_and_shared_level_edges_are_not_duplicated() {
    let flat = Tin { vertices: vec![[0., 0., 1.], [1., 0., 1.], [0., 1., 1.]], triangles: vec![[0, 1, 2]] };
    assert!(contours(&flat, 1., 5).unwrap().is_empty());
    let ridge = Tin { vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 1.], [0., -1., 1.]], triangles: vec![[0, 1, 2], [1, 0, 3]] };
    let paths = contours(&ridge, 1., 5).unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].points.len(), 2);
}

#[test]
fn vertex_on_level_connects_and_negative_levels_keep_index_cadence() {
    let tin = Tin { vertices: vec![[0., 0., -2.], [2., 0., 0.], [2., 2., 0.], [0., 2., -2.]], triangles: vec![[0, 1, 2], [0, 2, 3]] };
    let paths = contours(&tin, 0.5, 2).unwrap();
    assert!(paths.iter().find(|p| p.elevation == -1.).unwrap().index);
    let mut tin = slope();
    tin.vertices.push([1., 1., 1.]);
    tin.triangles = vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
    let paths = contours(&tin, 1., 5).unwrap();
    let middle: Vec<_> = paths.iter().filter(|p| p.elevation == 1.).collect();
    assert_eq!(middle.len(), 1);
    assert_eq!(middle[0].points.len(), 3);
}

#[test]
fn hostile_parameters_and_invalid_tins_fail_without_panicking() {
    for interval in [0., -1., f64::NAN, f64::INFINITY, 1e-300] {
        assert!(contours(&slope(), interval, 5).is_err());
    }
    assert!(contours(&slope(), 1., 0).is_err());
    let mut tin = slope();
    tin.triangles.push([0, 1, u32::MAX]);
    assert!(contours(&tin, 1., 5).is_err());
    tin.triangles.pop();
    tin.vertices[0][0] = f64::NAN;
    assert!(contours(&tin, 1., 5).is_err());
    assert!(contours(&Tin::default(), 1., 5).unwrap().is_empty());
}

#[test]
fn decimal_interval_keeps_level_edges_at_the_minimum_and_maximum() {
    let tin = Tin { vertices: vec![[0., 0., 0.3], [2., 0., 0.6], [2., 2., 0.6], [0., 2., 0.3]], triangles: vec![[0, 1, 2], [0, 2, 3]] };
    let paths = contours(&tin, 0.1, 3).unwrap();
    for (x, z) in [(0., 0.3), (2., 0.6)] {
        let path = paths.iter().find(|p| (p.elevation - z).abs() < 1e-14).unwrap();
        assert!(path.points.iter().all(|p| p[0] == x));
        assert_eq!(path.points.len(), 2);
        assert!(path.index);
    }
}

#[test]
fn saddle_vertex_ends_branches_without_arbitrarily_joining_them() {
    let tin = Tin {
        vertices: vec![[0., 0., 0.], [-1., -1., 1.], [1., -1., -1.], [1., 1., 1.], [-1., 1., -1.]],
        triangles: vec![[0, 1, 2], [0, 2, 3], [0, 3, 4], [0, 4, 1]],
    };
    let paths = contours(&tin, 1., 5).unwrap();
    assert_eq!(paths.len(), 4);
    for path in paths {
        assert_eq!(path.elevation, 0.);
        assert_eq!(path.points.len(), 2);
        assert!(path.points.contains(&[0., 0., 0.]));
        assert!(!path.closed);
    }
}

#[test]
fn disconnected_surfaces_do_not_bridge_even_at_the_same_elevation() {
    let mut tin = slope();
    tin.vertices.extend(slope().vertices.iter().map(|&[x, y, z]| [x + 10., y, z]));
    tin.triangles.extend([[4, 5, 6], [4, 6, 7]]);
    let paths = contours(&tin, 1., 5).unwrap();
    let middle: Vec<_> = paths.iter().filter(|p| p.elevation == 1.).collect();
    assert_eq!(middle.len(), 2);
    for path in middle {
        assert!(path.points.iter().all(|p| p[0] == path.points[0][0]));
    }
}

#[test]
fn high_elevations_do_not_snap_unrelated_low_vertices_to_zero() {
    let tin = Tin {
        vertices: vec![[0., 0., 1e-7], [1., 0., 1e-7], [0., 1., 1.], [10., 0., 0.], [11., 0., 1e10], [10., 1., 1e10]],
        triangles: vec![[0, 1, 2], [3, 4, 5]],
    };
    let paths = contours(&tin, 1e9, 5).unwrap();
    assert!(paths.iter().flat_map(|p| &p.points).all(|p| p[0] >= 10.));
}

#[test]
fn boundary_clipped_tin_never_draws_contours_outside_the_boundary() {
    use cadcraft_surface::triangulate::{BuildOptions, build};
    let options = BuildOptions { boundary: Some(vec![[0., 0.], [4., 0.], [4., 4.], [0., 4.]]), ..BuildOptions::default() };
    // A conservative clip may reject the entire triangle; a precise clip may split it.
    if let Ok(built) = build(&[[0., 0., 0.], [10., 0., 10.], [0., 10., 0.]], &[], &options) {
        for path in contours(&built.tin, 1., 5).unwrap() {
            assert!(path.points.iter().all(|p| p[0] >= 0. && p[0] <= 4. && p[1] >= 0. && p[1] <= 4.), "contour leaves boundary: {:?}", path.points);
        }
    }
}

#[test]
fn concave_boundary_vertices_on_a_triangle_edge_do_not_hide_a_crossing() {
    use cadcraft_surface::triangulate::{BuildOptions, build};
    // The notch meets the diagonal at explicit boundary vertices (2,8) and (1,9).
    let options = BuildOptions {
        boundary: Some(vec![[0., 0.], [10., 0.], [10., 10.], [2., 10.], [2., 8.], [2., 1.], [1., 1.], [1., 9.], [1., 10.], [0., 10.]]),
        ..BuildOptions::default()
    };
    if let Ok(built) = build(&[[0., 0., 0.], [10., 0., 10.], [0., 10., 0.]], &[], &options) {
        for path in contours(&built.tin, 0.5, 5).unwrap() {
            assert!(path.points.iter().all(|p| !(p[0] > 1. && p[0] < 2. && p[1] > 1.)), "contour crosses notch: {:?}", path.points);
        }
    }
}
