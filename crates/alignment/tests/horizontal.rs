use cadcraft_alignment::{Alignment, Element, Kind, StationFormat, Turn};
use std::f64::consts::{FRAC_PI_2, PI};

fn element(start: [f64; 2], direction: f64, length: f64, kind: Kind) -> Element {
    Element { start, direction, length, kind }
}
fn route(elements: Vec<Element>) -> Alignment {
    Alignment { name: "Route".into(), start_station: -10.0, format: StationFormat::Thousands, elements }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn line_arc_line_known_answers() {
    // Tangent east 10, left radius10 quarter-circle about (10,10), tangent north10.
    // Arc delta=pi/2; L=10+5*pi+10. Arc midpoint=(10+10/sqrt2,10-10/sqrt2).
    let a = route(vec![
        element([0., 0.], 0., 10., Kind::Line),
        element([10., 0.], 0., 5. * PI, Kind::Arc { radius: 10., turn: Turn::Left }),
        element([20., 10.], FRAC_PI_2, 10., Kind::Line),
    ]);
    assert!(a.validate().unwrap().is_empty());
    close(a.length().unwrap(), 20. + 5. * PI);
    for (distance, p) in [
        (0., [0., 0.]),
        (5., [5., 0.]),
        (10. + 2.5 * PI, [10. + 10. / 2f64.sqrt(), 10. - 10. / 2f64.sqrt()]),
        (10. + 5. * PI, [20., 10.]),
        (20. + 5. * PI, [20., 20.]),
    ] {
        let pose = a.at(a.start_station + distance, 0.).unwrap();
        close(pose.point[0], p[0]);
        close(pose.point[1], p[1]);
        close(a.project(p).unwrap().station, a.start_station + distance);
    }
    assert!(a.at(-10.0001, 0.).is_err());
    let b = a.project([-2., 3.]).unwrap();
    close(b.station, -10.);
    close(b.offset, 3.);
    close(b.distance, 13f64.sqrt());
    assert_eq!(b.beyond, Some(cadcraft_alignment::Beyond::Before));
    let p = a.at(-5., 2.).unwrap().point;
    assert_eq!(p, [5., 2.]);
}

#[test]
fn clothoids_match_independent_classic_series_and_invert() {
    // Independent Fresnel series for t=L/(2R)=.1: x=L(1-t²/10+t⁴/216-t⁶/9360),
    // y=L(t/3-t³/42+t⁵/1320-t⁷/75600). Next terms <1e-11.
    let x = 60. * (1. - 0.1f64.powi(2) / 10. + 0.1f64.powi(4) / 216. - 0.1f64.powi(6) / 9360.);
    let y = 60. * (0.1 / 3. - 0.1f64.powi(3) / 42. + 0.1f64.powi(5) / 1320. - 0.1f64.powi(7) / 75600.);
    let entry = element([0., 0.], 0., 60., Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left });
    let end = entry.at(60.).unwrap();
    close(end.point[0], x);
    close(end.point[1], y);
    close(end.direction, 0.1);
    for kind in [
        entry.kind,
        Kind::Spiral { radius_start: Some(300.), radius_end: None, turn: Turn::Right },
        Kind::Spiral { radius_start: Some(300.), radius_end: Some(100.), turn: Turn::Left },
    ] {
        let a = route(vec![element([1000000., 2000000.], 0.4, 60., kind)]);
        for d in [0., 0.1, 15., 30., 59.9, 60.] {
            let p = a.at(d - 10., 0.).unwrap().point;
            close(a.project(p).unwrap().station, d - 10.);
        }
    }
}

#[test]
fn malformed_geometry_joint_reports_and_caps() {
    for length in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(route(vec![element([0., 0.], 0., length, Kind::Line)]).validate().is_err());
    }
    for radius in [0., -1., f64::NAN] {
        assert!(route(vec![element([0., 0.], 0., 1., Kind::Arc { radius, turn: Turn::Left })]).validate().is_err());
    }
    let a = route(vec![element([0., 0.], 0., 1., Kind::Line), element([2., 0.], 1., 1., Kind::Line), element([4., 0.], 2., 1., Kind::Line)]);
    assert_eq!(a.validate().unwrap().len(), 2);
    assert!(a.at(-10., 0.).is_err());
    let a = route(vec![element([0., 0.], 0., 1., Kind::Line); 1_000_000]);
    assert!(a.validate().is_err());
    let a = route(vec![element([0., 0.], 0., PI, Kind::Arc { radius: 1., turn: Turn::Left })]);
    assert!(a.polyline(0.).is_err());
    assert!(a.polyline(1e-30).is_err());
    assert_eq!(StationFormat::Thousands.display(999.9996).unwrap(), "1+000.000");
    assert_eq!(StationFormat::Hundreds.display(-125.25).unwrap(), "-1+25.25");
}

#[test]
fn line_spiral_arc_spiral_line_independent_endpoints() {
    // Fresnel entry L60,R300,t=.1 gives (59.940027771368,1.998571883037).
    // Arc centre = entry endpoint +300*(-sin(.1),cos(.1)). Arc sweep .2.
    // Exit has the same local displacement rotated .1: integral exp(i(.2u-.1u²))
    // equals exp(i*.1)*conj(entry integral). End tangent is .4 radians.
    let entry = element([10., 0.], 0., 60., Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left });
    let p = entry.at(60.).unwrap();
    close(p.point[0], 69.940027771368);
    close(p.point[1], 1.998571883037);
    let center = [p.point[0] - 300. * 0.1f64.sin(), p.point[1] + 300. * 0.1f64.cos()];
    close(center[0], 39.990002777320);
    close(center[1], 300.499821466445);
    let arc = element(p.point, 0.1, 60., Kind::Arc { radius: 300., turn: Turn::Left });
    let q = arc.at(60.).unwrap();
    let exit = element(q.point, 0.3, 60., Kind::Spiral { radius_start: Some(300.), radius_end: None, turn: Turn::Left });
    let r = exit.at(60.).unwrap();
    // Exit displacement = exp(i*.4)*conj(entry displacement), added to
    // centre +300*(sin(.3),-cos(.3)); computed independently from the Fresnel series above.
    close(r.point[0], 184.63276688505437);
    close(r.point[1], 35.399814376247136);
    let a = route(vec![element([0., 0.], 0., 10., Kind::Line), entry, arc, exit, element(r.point, 0.4, 10., Kind::Line)]);
    assert!(a.validate().unwrap().is_empty());
    close(a.length().unwrap(), 200.);
    for d in [0., 5., 10., 20., 70., 95., 130., 145., 190., 200.] {
        let p = a.at(d - 10., 0.).unwrap().point;
        close(a.project(p).unwrap().station, d - 10.);
    }
}

#[test]
fn finite_large_directions_project_using_the_evaluated_tangent() {
    let a = route(vec![element([0., 0.], 1e100, 10., Kind::Line)]);
    let p = a.at(-5., 0.).unwrap();
    close(a.project(p.point).unwrap().station, -5.);
    let arc = route(vec![element([1., 0.], FRAC_PI_2, PI, Kind::Arc { radius: 1., turn: Turn::Left })]);
    close(arc.project([0., 0.]).unwrap().station, -10.);
}

#[test]
fn start_station_accepts_finite_values_beyond_coordinate_limits() {
    let mut a = route(vec![element([0., 0.], 0., 10., Kind::Line)]);
    a.start_station = -1e12;
    assert_eq!(a.at(a.start_station + 5., 0.).unwrap().point, [5., 0.]);
    assert_eq!(StationFormat::Thousands.display(1e12).unwrap(), "1000000000+000.000");
}

#[test]
fn inclusive_end_station_survives_subtraction_roundoff() {
    let mut a = route(vec![element([0., 0.], 0., 0.1, Kind::Line)]);
    for start in [1., 1000., -1000., 1e12, -1e12] {
        a.start_station = start;
        let end = start + 0.1;
        assert_eq!(a.at(start, 0.).unwrap().point, [0., 0.]);
        assert_eq!(a.at(end, 2.).unwrap().point, [0.1, 2.]);
        assert!(a.at(start.next_down(), 0.).is_err());
        assert!(a.at(end.next_up(), 0.).is_err());
    }
}

#[test]
fn projection_accepts_single_element_inclusive_endpoint() {
    let mut a = route(vec![element([0., 0.], 0., 0.1, Kind::Line)]);
    a.start_station = 1.;
    let end = a.project([0.1, 0.]).unwrap();
    assert_eq!(end.station, 1.1);
    assert_eq!(end.distance, 0.);
    assert_eq!(end.beyond, None);
    let after = a.project([1.1, 2.]).unwrap();
    assert_eq!(after.station, 1.1);
    assert_eq!(after.offset, 2.);
    assert_eq!(after.beyond, Some(cadcraft_alignment::Beyond::After));
}

#[test]
fn projection_uses_one_station_origin_across_multiple_elements() {
    let mut a = route(vec![element([0., 0.], 0., 0.1, Kind::Line), element([0.1, 0.], 0., 0.1, Kind::Line)]);
    a.start_station = 1.;
    let end = a.project([0.2, 0.]).unwrap();
    assert_eq!(end.station, 1.2);
    assert_eq!(end.distance, 0.);
    assert_eq!(end.element, 1);
    assert_eq!(end.beyond, None);
    let after = a.project([1.2, 2.]).unwrap();
    assert_eq!(after.station, 1.2);
    assert_eq!(after.offset, 2.);
    assert_eq!(after.beyond, Some(cadcraft_alignment::Beyond::After));
}

#[test]
fn shallow_arc_projection_preserves_small_sweep_precision() {
    for (turn, y) in [(Turn::Left, 5e-13), (Turn::Right, -5e-13)] {
        let mut a = route(vec![element([0., 0.], 0., 10., Kind::Arc { radius: 1e12, turn })]);
        a.start_station = 0.;
        // sin(1e-12)*1e12 = 1 within 1e-24; sagitta = 5e-13 within 1e-37.
        let projected = a.project([1., y]).unwrap();
        assert!((projected.station - 1.).abs() < 1e-6, "{projected:?}");
        assert!(projected.offset.abs() < 1e-6, "{projected:?}");
        assert!(projected.distance < 1e-6, "{projected:?}");
    }
}

#[test]
fn inclusive_endpoint_uses_final_element_after_long_route() {
    let mut elements = Vec::new();
    let mut point = [0., 0.];
    let mut direction = 0.;
    // Keep coordinates bounded while the cumulative length grows. Subtracting
    // those large lengths back out of the total loses part of the final 0.1.
    for i in 0..4094 {
        let e = element(point, direction, 1e7, Kind::Arc { radius: 2e6 + 100. * f64::from(i), turn: Turn::Left });
        let end = e.at(e.length).unwrap();
        point = end.point;
        direction = end.direction;
        elements.push(e);
    }
    let last = element(point, direction, 0.1, Kind::Line);
    let endpoint = last.at(0.1).unwrap();
    elements.push(last);
    let mut a = route(elements);
    a.start_station = 0.;
    assert_eq!(a.at(a.length().unwrap(), 0.).unwrap(), endpoint);
    let projected = a.project(endpoint.point).unwrap();
    assert_eq!(projected.distance, 0.);
    assert_eq!(projected.offset, 0.);
    assert_eq!(projected.beyond, None);
}

#[test]
fn off_curve_spiral_projection_refines_the_stationary_point() {
    for kind in [
        Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left },
        Kind::Spiral { radius_start: Some(300.), radius_end: None, turn: Turn::Left },
        Kind::Spiral { radius_start: Some(300.), radius_end: Some(100.), turn: Turn::Left },
    ] {
        let a = route(vec![element([0., 0.], 0., 60., kind)]);
        for station in [10., 30.] {
            for offset in [1., 50., -1000.] {
                // A normal offset on these convex spirals has its unique minimum at station.
                let point = a.at(station, offset).unwrap().point;
                let projected = a.project(point).unwrap();
                assert!((projected.station - station).abs() <= 1e-7, "{kind:?}, {station}, {offset}: {projected:?}");
                close(projected.offset, offset);
            }
        }
    }
}

#[test]
fn carrier_preserves_major_arcs_and_chords_only_the_spiral() {
    for turn in [Turn::Left, Turn::Right] {
        let a = route(vec![element([0., 0.], 0., 15. * PI, Kind::Arc { radius: 10., turn })]);
        let vertices = a.validated().unwrap().carrier_vertices(0.001).unwrap();
        assert_eq!(vertices.len(), 2);
        let sign = if turn == Turn::Left { 1. } else { -1. };
        // A 270-degree arc has bulge tan(270/4 degrees) = 1+sqrt(2).
        assert!((vertices[0].bulge - sign * (1. + 2f64.sqrt())).abs() < 1e-12);
        assert_eq!(vertices[1].bulge, 0.);
        assert!(a.polyline(0.001).unwrap().len() > 2);
    }
    let a = route(vec![element([0., 0.], 0., 60., Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left })]);
    let vertices = a.validated().unwrap().carrier_vertices(0.001).unwrap();
    assert!(vertices.len() > 2);
    assert!(vertices.iter().all(|v| v.bulge == 0.));
}
