//! TIN building: duplicates, breaklines, maximum edge length, boundary clipping, hostile input.

use cadcraft_surface::triangulate::{BuildOptions, build, signed_area2};

fn grid(n: usize) -> Vec<[f64; 3]> {
    (0..n * n).map(|i| [(i % n) as f64, (i / n) as f64, (i % n + i / n) as f64]).collect()
}

fn all_ccw(t: &cadcraft_surface::Tin) -> bool {
    t.triangles.iter().all(|tri| {
        let p = tri.map(|k| t.vertices[k as usize]);
        let (a, degenerate) = signed_area2(p[0], p[1], p[2]);
        a > 0.0 && !degenerate
    })
}

#[test]
fn square_of_points_makes_two_ccw_triangles() {
    let b = build(&[[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [10.0, 10.0, 3.0], [0.0, 10.0, 4.0]], &[], &BuildOptions::default()).unwrap();
    assert_eq!((b.tin.vertices.len(), b.tin.triangles.len()), (4, 2));
    assert!(all_ccw(&b.tin));
}

#[test]
fn duplicate_xy_keeps_the_first_elevation_and_breaklines_win() {
    let pts = [[0.0, 0.0, 1.0], [10.0, 0.0, 1.0], [0.0, 10.0, 1.0], [0.0, 0.0, 99.0]];
    let b = build(&pts, &[], &BuildOptions::default()).unwrap();
    assert_eq!(b.duplicates, 1);
    assert!(b.tin.vertices.contains(&[0.0, 0.0, 1.0]) && !b.tin.vertices.iter().any(|v| v[2] == 99.0));
    let b = build(&pts, &[vec![[0.0, 0.0, 5.0], [10.0, 0.0, 5.0]]], &BuildOptions::default()).unwrap();
    assert!(b.tin.vertices.contains(&[0.0, 0.0, 5.0]) && b.tin.vertices.contains(&[10.0, 0.0, 5.0]));
}

#[test]
fn breakline_becomes_a_triangle_edge() {
    // Plain Delaunay of this square would connect (0,5)-(10,5)'s neighbours the other way.
    let pts = [[5.0, 0.0, 0.0], [5.0, 10.0, 0.0], [0.0, 4.0, 0.0], [10.0, 6.0, 0.0]];
    let b = build(&pts, &[vec![[0.0, 4.0, 3.0], [10.0, 6.0, 3.0]]], &BuildOptions::default()).unwrap();
    let ix = |p: [f64; 2]| b.tin.vertices.iter().position(|v| v[0] == p[0] && v[1] == p[1]).unwrap() as u32;
    let (u, v) = (ix([0.0, 4.0]), ix([10.0, 6.0]));
    assert!(b.tin.triangles.iter().any(|t| t.contains(&u) && t.contains(&v)));
    // A breakline crossing the first one is skipped, not forced.
    let b = build(&pts, &[vec![[0.0, 4.0, 3.0], [10.0, 6.0, 3.0]], vec![[5.0, 0.0, 0.0], [5.0, 10.0, 0.0]]], &BuildOptions::default()).unwrap();
    assert_eq!(b.skipped_segments, 1);
}

#[test]
fn max_edge_and_boundary_clip_triangles() {
    let mut pts = grid(5);
    pts.push([100.0, 100.0, 0.0]);
    let all = build(&pts, &[], &BuildOptions::default()).unwrap();
    let short = build(&pts, &[], &BuildOptions { max_edge: Some(2.0), ..Default::default() }).unwrap();
    assert_eq!(short.tin.triangles.len(), 32);
    assert_eq!(short.tin.vertices.len(), 25, "the far point is no longer used");
    assert!(short.dropped_triangles > 0 && all.tin.triangles.len() > 32);
    let half = build(&grid(5), &[], &BuildOptions { boundary: Some(vec![[-1.0, -1.0], [2.0, -1.0], [2.0, 5.0], [-1.0, 5.0]]), ..Default::default() })
        .unwrap();
    assert_eq!(half.tin.triangles.len(), 16);
    assert!(all_ccw(&half.tin));
}

#[test]
fn hostile_input_is_an_error_not_a_panic() {
    let o = BuildOptions::default();
    assert!(build(&[], &[], &o).is_err());
    assert!(build(&[[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 0.0]], &[], &o).is_err(), "collinear");
    assert!(build(&[[0.0, 0.0, f64::NAN], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[], &o).is_err());
    assert!(build(&[[f64::INFINITY, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[], &o).is_err());
    assert!(build(&grid(3), &[], &BuildOptions { max_edge: Some(-1.0), ..Default::default() }).is_err());
    assert!(build(&grid(3), &[], &BuildOptions { max_edge: Some(f64::NAN), ..Default::default() }).is_err());
    assert!(build(&grid(3), &[], &BuildOptions { boundary: Some(vec![[0.0, 0.0], [1.0, f64::NAN], [0.0, 1.0]]), ..Default::default() }).is_err());
    assert!(
        build(&grid(3), &[], &BuildOptions { boundary: Some(vec![[50.0, 50.0], [51.0, 50.0], [50.0, 51.0]]), ..Default::default() }).is_err(),
        "nothing inside"
    );
    // Huge but finite coordinates, overlapping breaklines, single-vertex and zero-length breaklines.
    let big = [[1e300, 0.0, 0.0], [-1e300, 0.0, 0.0], [0.0, 1e300, 0.0], [0.0, 0.0, 0.0]];
    let _ = build(
        &big,
        &[
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[3.0, 3.0, 0.0]],
            vec![[4.0, 4.0, 0.0], [4.0, 4.0, 1.0]],
        ],
        &o,
    );
}
