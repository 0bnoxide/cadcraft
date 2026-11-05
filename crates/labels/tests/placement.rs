use cadcraft_labels::{
    Pt, Rect,
    placement::{Request, place},
};

#[test]
fn exact_boundary_width_does_not_panic_on_rounding() {
    let b = Rect { min_x: -2778.850520327856, max_x: 4580.849370357377, min_y: 0.0, max_y: 2.0 };
    let r = Request { anchor: Pt::default(), width: b.width(), height: 2.0, origin: None, locked: false };
    let p = place(&[r], &[], Some(b), 1.0, 0.0).unwrap();
    assert_eq!(p[0].rect, b);
}

#[test]
fn reports_point_buffer_and_obstacle_collisions() {
    let r = Request { anchor: Pt::default(), width: 2.0, height: 2.0, origin: Some(Pt::default()), locked: true };
    let obstacle = Rect::from_corner(Pt::default(), 1.0, 1.0).unwrap();
    let p = place(&[r], &[obstacle], None, 0.0, 1.0).unwrap();
    assert_eq!(p[0].buffer_hits, vec![0]);
    assert_eq!(p[0].obstacle_hits, vec![0]);
}

#[test]
fn same_point_labels_separate_and_crowded_labels_are_all_reported() {
    let r = Request { anchor: Pt::default(), width: 2.0, height: 1.0, origin: None, locked: false };
    let separate = place(&[r.clone(), r.clone()], &[], None, 1.0, 0.0).unwrap();
    assert!(separate.iter().all(|p| p.overlap == 0.0));
    let crowded = place(&vec![r; 10], &[], Some(Rect { min_x: -2.0, min_y: -1.0, max_x: 2.0, max_y: 1.0 }), 0.0, 0.0).unwrap();
    assert_eq!(crowded.len(), 10);
    assert!(crowded.iter().any(|p| p.overlap > 0.0));
}

#[test]
fn crowded_labels_report_overlap_and_stay_inside() {
    let bounds = Rect::from_corner(Pt::default(), 10.0, 10.0).unwrap();
    let req = Request { anchor: Pt { x: 5.0, y: 5.0 }, width: 8.0, height: 8.0, origin: None, locked: false };
    let placed = place(&[req.clone(), req], &[], Some(bounds), 1.0, 0.0).unwrap();
    assert_eq!(placed.len(), 2);
    assert!(placed.iter().all(|p| p.rect.min_x >= 0.0 && p.rect.max_x <= 10.0));
    assert!(placed.last().unwrap().overlap > 0.0);
}

#[test]
fn rejects_hostile_dimensions_and_impossible_boundary() {
    let mut req = Request { anchor: Pt::default(), width: f64::NAN, height: 1.0, origin: None, locked: false };
    assert!(place(&[req.clone()], &[], None, 1.0, 0.0).is_err());
    req.width = 20.0;
    assert!(place(&[req], &[], Rect::from_corner(Pt::default(), 10.0, 10.0), 1.0, 0.0).is_err());
}

#[test]
fn far_labels_do_not_collide_and_locked_origin_is_preserved() {
    let a = Request { anchor: Pt { x: 0.0, y: 0.0 }, width: 2.0, height: 1.0, origin: Some(Pt { x: 1.0, y: 1.0 }), locked: true };
    let b = Request { anchor: Pt { x: 100.0, y: 100.0 }, origin: None, locked: false, ..a.clone() };
    let p = place(&[a, b], &[], None, 1.0, 0.1).unwrap();
    assert_eq!(p[0].rect.min_x, 1.0);
    assert!(p.iter().all(|p| p.overlap == 0.0));
}

#[test]
fn touching_rectangles_have_no_overlap_and_prohibited_area_is_avoided() {
    use cadcraft_labels::placement::intersection;
    let r = Rect::from_corner(Pt::default(), 2.0, 2.0).unwrap();
    let other = Rect::from_corner(Pt { x: 2.0, y: 0.0 }, 2.0, 2.0).unwrap();
    assert_eq!(intersection(r, other), 0.0);
    let request = Request { anchor: Pt::default(), width: 1.0, height: 1.0, origin: None, locked: false };
    let p = place(&[request], &[r], None, 0.5, 0.0).unwrap();
    assert_eq!(p[0].overlap, 0.0);
}
