use cadcraft_alignment::{
    Alignment, Element, Kind, StationFormat, Turn,
    landxml::{self, Unit},
};
fn route() -> Alignment {
    let first = Element {
        start: [12., 34.],
        direction: 0.,
        length: 60.,
        kind: Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left },
    };
    let p = first.at(60.).unwrap();
    Alignment {
        name: "Route & <路线>".into(),
        start_station: -123.5,
        format: StationFormat::Thousands,
        elements: vec![first, Element { start: p.point, direction: p.direction, length: 100., kind: Kind::Arc { radius: 300., turn: Turn::Left } }],
    }
}
#[test]
fn exact_spiral_arc_export_reimports_and_units_scale() {
    let a = route();
    let xml = landxml::export(std::slice::from_ref(&a), Unit::Foot).unwrap();
    assert!(xml.contains("spiType=\"clothoid\""));
    assert!(xml.contains("radiusStart=\"INF\""));
    let imported = landxml::import(xml.as_bytes(), Unit::Foot).unwrap();
    assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
    let b = &imported.alignments[0].definition;
    assert_eq!(b.name, a.name);
    assert_eq!(b.start_station, a.start_station);
    for s in [-123.5, -100., -63.5, 0., 36.5] {
        let p = a.at(s, 0.).unwrap();
        let q = b.at(s, 0.).unwrap();
        assert!((p.point[0] - q.point[0]).hypot(p.point[1] - q.point[1]) < 1e-6);
    }
    let metric = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!((metric.alignments[0].definition.length().unwrap() - a.length().unwrap() * 0.3048).abs() < 1e-6);
}
fn file(body: &str) -> String {
    format!(
        "<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" version=\"1.2\"><Units><Metric linearUnit=\"meter\" directionUnit=\"decimal degrees\"/></Units><Alignments>{body}</Alignments></LandXML>"
    )
}
#[test]
fn unsupported_alignment_is_refused_without_losing_other_alignment() {
    let body = "<Alignment name=\"bad\" staStart=\"0\" length=\"10\"><CoordGeom><Spiral spiType=\"cubic\"/></CoordGeom></Alignment><Alignment name=\"good\" staStart=\"0\" length=\"10\"><CoordGeom><Line><Start>0 0</Start><End>0 10</End></Line></CoordGeom><StaEquation staBack=\"5\" staAhead=\"10\"/><Profile/></Alignment>";
    let r = landxml::import(file(body).as_bytes(), Unit::Meter).unwrap();
    assert_eq!(r.alignments.len(), 1);
    assert_eq!(r.refusals[0].code, "UNSUPPORTED_SPIRAL");
    assert!(r.warnings.iter().any(|w| w.code == "STATION_EQUATIONS_IGNORED"));
    assert!(!r.warnings.iter().any(|w| w.code == "PROFILES_NOT_SUPPORTED"));
}
#[test]
fn hostile_xml_limits_and_nonfinite_values_are_refused() {
    for xml in
        ["<LandXML>", "<!DOCTYPE foo [<!ENTITY x 'hi'>]><LandXML/>", "<LandXML xmlns='http://www.landxml.org/schema/LandXML-1.2' version='1.2'/>"]
    {
        assert!(landxml::import(xml.as_bytes(), Unit::Meter).is_err());
    }
    let deep = format!("{}{}", "<x>".repeat(33), "</x>".repeat(33));
    assert!(landxml::import(deep.as_bytes(), Unit::Meter).is_err());
    for number in ["NaN", "INF", "1e400"] {
        let xml = file(&format!(
            "<Alignment name='bad' length='1' staStart='0'><CoordGeom><Line><Start>0 {number}</Start><End>0 1</End></Line></CoordGeom></Alignment>"
        ));
        let r = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        assert_eq!(r.refusals.len(), 1);
    }
    let million = file(&format!("<Alignment name='huge' length='1' staStart='0'><CoordGeom>{}</CoordGeom></Alignment>", "<Line/>".repeat(1_000_000)));
    assert!(landxml::import(million.as_bytes(), Unit::Meter).is_err());
}

#[test]
fn malformed_declarations_and_attributes_are_errors() {
    let valid =
        file("<Alignment name='good' length='1' staStart='0'><CoordGeom><Line><Start>0 0</Start><End>0 1</End></Line></CoordGeom></Alignment>");
    for body in [
        valid.replace("<Units>", "<?xml version='1.0'?><Units>"),
        format!("<?xml version='1.0' encoding='UTF-16'?>{valid}"),
        valid.replace("version=\"1.2\"", "version=\"1.2\" bad='<x'"),
    ] {
        assert!(landxml::import(body.as_bytes(), Unit::Meter).is_err());
    }
}

#[test]
fn export_uses_the_same_normalized_direction_as_evaluation() {
    let a = Alignment {
        name: "direction".into(),
        start_station: 0.,
        format: StationFormat::Thousands,
        elements: vec![Element { start: [0., 0.], direction: 1e100, length: 10., kind: Kind::Line }],
    };
    let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
    let b = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!(b.refusals.is_empty(), "{:?}", b.refusals);
}

#[test]
fn shallow_arc_roundtrip_preserves_small_sweep_precision() {
    for (turn, direction, length) in [Turn::Left, Turn::Right]
        .into_iter()
        .flat_map(|turn| [0., 0.1, 0.4, std::f64::consts::FRAC_PI_2, 5.9].into_iter().map(move |direction| (turn, direction)))
        .flat_map(|(turn, direction)| [1., 10_000.].into_iter().map(move |length| (turn, direction, length)))
    {
        let a = Alignment {
            name: "shallow".into(),
            start_station: 0.,
            format: StationFormat::Thousands,
            elements: vec![Element { start: [0., 0.], direction, length, kind: Kind::Arc { radius: 1e12, turn } }],
        };
        let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
        let imported = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
        assert_eq!(imported.alignments.len(), 1);
        let b = &imported.alignments[0].definition;
        assert!((b.length().unwrap() - length).abs() < 1e-6);
        assert_eq!(b.elements[0].kind, Kind::Arc { radius: 1e12, turn });
        let end = b.elements[0].at(b.elements[0].length).unwrap();
        // For sweep <= 1e-8, the omitted Taylor terms contribute < 2e-13
        // drawing units: local x=L, local y=+/-L^2/(2R), then rotate.
        let left = if turn == Turn::Left { 1. } else { -1. } * length * length / 2e12;
        let expected = [length * direction.cos() - left * direction.sin(), length * direction.sin() + left * direction.cos()];
        assert!((end.point[0] - expected[0]).abs() < 1e-6);
        assert!((end.point[1] - expected[1]).abs() < 1e-6);
    }
}

#[test]
fn arc_exchange_and_projection_keep_major_sweeps_and_rotation() {
    for (turn, side) in [(Turn::Left, -1.), (Turn::Right, 1.)] {
        // Radius 2, north-facing start, 270-degree turn: end=(+/-2,-2).
        // At half the arc, 135 degrees from the start radius, the point is
        // (+/-(2+sqrt(2)),sqrt(2)) and station is 3*pi/2.
        let a = Alignment {
            name: "major".into(),
            start_station: 0.,
            format: StationFormat::Thousands,
            elements: vec![Element {
                start: [0., 0.],
                direction: std::f64::consts::FRAC_PI_2,
                length: 3. * std::f64::consts::PI,
                kind: Kind::Arc { radius: 2., turn },
            }],
        };
        let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
        let imported = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
        let b = &imported.alignments[0].definition;
        assert!((b.length().unwrap() - 3. * std::f64::consts::PI).abs() < 1e-6);
        let endpoint = b.at(b.length().unwrap(), 0.).unwrap().point;
        assert!((endpoint[0] - side * 2.).abs() < 1e-6);
        assert!((endpoint[1] + 2.).abs() < 1e-6);
        let mid = [side * (2. + 2f64.sqrt()), 2f64.sqrt()];
        let projected = b.project(mid).unwrap();
        assert!((projected.station - 1.5 * std::f64::consts::PI).abs() < 1e-6);
        assert!(projected.distance < 1e-6);
    }
}

#[test]
fn arc_radius_roundoff_keeps_geometry_and_radius_checks_strict() {
    // A one-unit arc at radius 1e12 has endpoint (1, 5e-13) to much
    // better than 1e-6. Radial calculations may round at that radius,
    // but a 1e-4 endpoint displacement is still a geometry mismatch.
    for (radius, end, reason) in [
        ("1000000000000", "0.0001000000005 1", "End gap"),
        ("999999999999", "0.0000000000005 1", "radius mismatch"),
        ("999999999999.9999", "0.0000000000005 1", ""),
    ] {
        let xml = file(&format!(
            "<Alignment name='radius' staStart='0' length='1'><CoordGeom><Curve radius='{radius}' length='1' rot='ccw'><Start>0 0</Start><Center>1000000000000 0</Center><End>{end}</End></Curve></CoordGeom></Alignment>"
        ));
        let imported = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        if reason.is_empty() {
            assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
        } else {
            assert!(imported.alignments.is_empty());
            assert_eq!(imported.refusals[0].code, "GEOMETRY_MISMATCH");
            assert!(imported.refusals[0].reason.contains(reason), "{:?}", imported.refusals);
        }
    }
}

#[test]
fn full_turn_bounded_clothoid_exports_as_exact_forward_pi_spirals() {
    let length = 4. * std::f64::consts::PI / (1. / 100. + 1. / 200.);
    let original = Alignment {
        name: "full turn".into(),
        start_station: 17.,
        format: StationFormat::Thousands,
        elements: vec![Element {
            start: [1000., -2000.],
            direction: 0.3,
            length,
            kind: Kind::Spiral { radius_start: Some(100.), radius_end: Some(200.), turn: Turn::Left },
        }],
    };
    let xml = landxml::export(std::slice::from_ref(&original), Unit::Meter).unwrap();
    let imported = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
    let restored = &imported.alignments[0].definition;
    assert!(restored.elements.len() > 1);
    assert!(restored.elements.iter().all(|e| matches!(e.kind, Kind::Spiral { .. })));
    for fraction in [0., 0.01, 0.1, 0.25, 0.5, 0.8, 0.99, 1.] {
        let station = 17. + fraction * length;
        let expected = original.at(station, 0.).unwrap();
        let actual = restored.at(station, 0.).unwrap();
        assert!((expected.point[0] - actual.point[0]).hypot(expected.point[1] - actual.point[1]) < 1e-6, "{station}");
        assert!((expected.direction - actual.direction).sin().abs() < 1e-8, "{station}");
    }
    // Independent high-precision quadrature of cos/sin(0.3 + k0*s +
    // (k1-k0)*s²/(2*length)), not the implementation's series integrator.
    for (fraction, north, east) in [
        (0.25, -1828.8508430972907, 1046.5732763594276),
        (0.5, -1828.0548440677378, 861.514779309197),
        (0.8, -2048.2956827670844, 891.2199714791577),
        (1., -2071.91392660059, 1051.1680521684968),
    ] {
        let actual = restored.at(17. + fraction * length, 0.).unwrap();
        assert!((actual.point[0] - east).hypot(actual.point[1] - north) < 1e-6, "{fraction}");
    }
}

#[test]
fn divided_spirals_preserve_entrance_exit_turn_and_endpoint_curvature() {
    for (radius_start, radius_end, turn) in [(None, Some(100.), Turn::Left), (Some(100.), None, Turn::Right), (Some(100.), Some(200.), Turn::Right)] {
        let k0 = radius_start.map_or(0., |r| 1. / r);
        let k1 = radius_end.map_or(0., |r| 1. / r);
        let length = 4. * std::f64::consts::PI / (k0 + k1);
        let e = Element { start: [25., -40.], direction: 1.2, length, kind: Kind::Spiral { radius_start, radius_end, turn } };
        let a = Alignment {
            name: format!("{radius_start:?}-{radius_end:?}-{turn:?}"),
            start_station: 13.,
            format: StationFormat::Thousands,
            elements: vec![e],
        };
        let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
        let result = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        assert!(result.refusals.is_empty(), "{:?}", result.refusals);
        let b = &result.alignments[0].definition;
        assert!(b.elements.len() > 1);
        assert!(matches!(b.elements.first().unwrap().kind, Kind::Spiral { radius_start: r, turn: t, .. } if r == radius_start && t == turn));
        assert!(matches!(b.elements.last().unwrap().kind, Kind::Spiral { radius_end: r, turn: t, .. } if r == radius_end && t == turn));
        for fraction in [0., 0.03, 0.5, 0.97, 1.] {
            let station = 13. + length * fraction;
            let expected = a.at(station, 0.).unwrap();
            let actual = b.at(station, 0.).unwrap();
            assert!((expected.point[0] - actual.point[0]).hypot(expected.point[1] - actual.point[1]) < 1e-6);
            assert!((expected.direction - actual.direction).sin().abs() < 1e-8);
        }
    }
}

#[test]
fn subdivision_respects_whole_file_element_limit() {
    let length = 4. * std::f64::consts::PI / (1. / 100. + 1. / 200.);
    let spiral =
        Element { start: [0., 0.], direction: 0., length, kind: Kind::Spiral { radius_start: Some(100.), radius_end: Some(200.), turn: Turn::Left } };
    let end = spiral.at(length).unwrap();
    let mut elements = vec![spiral];
    for i in 0..4095 {
        elements.push(Element { start: [end.point[0] + i as f64, end.point[1]], direction: 0., length: 1., kind: Kind::Line });
    }
    // The first line must connect to the full-turn spiral and all lines to each other.
    let a = Alignment { name: "too many after subdivision".into(), start_station: 0., format: StationFormat::Thousands, elements };
    assert_eq!(a.validate().unwrap().len(), 0);
    assert_eq!(landxml::export(&[a], Unit::Meter).unwrap_err().code, "ELEMENT_LIMIT");
}

#[test]
fn nearly_parallel_tangents_without_reliable_pi_are_refused() {
    let a = Alignment {
        name: "tiny sweep".into(),
        start_station: 0.,
        format: StationFormat::Thousands,
        elements: vec![Element {
            start: [0., 0.],
            direction: 0.,
            length: 0.001,
            kind: Kind::Spiral { radius_start: Some(1e12), radius_end: Some(5e11), turn: Turn::Left },
        }],
    };
    assert_eq!(landxml::export(&[a], Unit::Meter).unwrap_err().code, "UNREPRESENTABLE_SPIRAL");
}

#[test]
fn nearly_full_turn_spiral_avoids_an_ill_conditioned_pi() {
    let length = (1. + 1e-12) * 4. * std::f64::consts::PI / (1. / 100. + 1. / 200.);
    let a = Alignment {
        name: "nearly full turn".into(),
        start_station: 100.,
        format: StationFormat::Thousands,
        elements: vec![Element {
            start: [1000., -2000.],
            direction: 0.3,
            length,
            kind: Kind::Spiral { radius_start: Some(100.), radius_end: Some(200.), turn: Turn::Left },
        }],
    };
    let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
    let result = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!(result.refusals.is_empty(), "{:?}", result.refusals);
    let b = &result.alignments[0].definition;
    assert!(b.elements.len() > 1);
    for fraction in [0., 0.5, 1.] {
        let station = 100. + fraction * length;
        let expected = a.at(station, 0.).unwrap();
        let actual = b.at(station, 0.).unwrap();
        assert!((expected.point[0] - actual.point[0]).hypot(expected.point[1] - actual.point[1]) < 1e-6);
    }
}

#[test]
fn maximum_allowed_turn_is_exchanged_without_approximation() {
    let length = 32. * std::f64::consts::PI / (1. / 100. + 1. / 200.);
    let a = Alignment {
        name: "eight turns".into(),
        start_station: -50.,
        format: StationFormat::Thousands,
        elements: vec![Element {
            start: [234.5, -678.25],
            direction: 2.7,
            length,
            kind: Kind::Spiral { radius_start: Some(100.), radius_end: Some(200.), turn: Turn::Left },
        }],
    };
    let xml = landxml::export(std::slice::from_ref(&a), Unit::Meter).unwrap();
    let result = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!(result.refusals.is_empty(), "{:?}", result.refusals);
    let b = &result.alignments[0].definition;
    assert!(b.elements.len() > 32);
    for fraction in [0., 0.01, 0.125, 0.249, 0.5, 0.751, 0.99, 1.] {
        let station = -50. + fraction * length;
        let expected = a.at(station, 0.).unwrap();
        let actual = b.at(station, 0.).unwrap();
        assert!((expected.point[0] - actual.point[0]).hypot(expected.point[1] - actual.point[1]) < 1e-6, "{fraction}");
        assert!((expected.direction - actual.direction).sin().abs() < 1e-8, "{fraction}");
    }
}

#[test]
fn high_coordinate_spiral_export_does_not_claim_a_failed_exchange() {
    for (radius_start, radius_end, length) in [(0.1, 0.2, 4. * std::f64::consts::PI / 15.), (1., 2., 4. * std::f64::consts::PI / 1.5)] {
        let a = Alignment {
            name: "high coordinate".into(),
            start_station: 0.,
            format: StationFormat::Thousands,
            elements: vec![Element {
                start: [1e8, 1e8],
                direction: 0.3,
                length,
                kind: Kind::Spiral { radius_start: Some(radius_start), radius_end: Some(radius_end), turn: Turn::Left },
            }],
        };
        match landxml::export(&[a], Unit::Meter) {
            Ok(xml) => {
                let result = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
                assert!(result.refusals.is_empty(), "{radius_start}: {:?}", result.refusals);
                assert_eq!(result.alignments.len(), 1);
            }
            Err(err) => assert_eq!(err.code, "UNREPRESENTABLE_SPIRAL"),
        }
    }
}

#[test]
fn review_spiral_precision_and_mixed_file_regressions() {
    let make = |name: &str, start, direction, length, kind| Alignment {
        name: name.into(),
        start_station: 0.,
        format: StationFormat::Thousands,
        elements: vec![Element { start, direction, length, kind }],
    };
    let kind = Kind::Spiral { radius_start: Some(100.), radius_end: Some(200.), turn: Turn::Left };
    let spiral = make("spiral", [0., 0.], 0., 10., kind);
    let line = make("line", [1e8, 1e8], 0.3, 0.001, Kind::Line);
    assert!(landxml::export(&[line, spiral], Unit::Meter).is_ok());

    let mut joined = make("joined", [0., 0.], 0., 1000., Kind::Line);
    joined.elements.push(Element { start: [1000., 0.], direction: 0., length: 4. * std::f64::consts::PI / 0.015, kind });
    assert_eq!(landxml::export(&[joined], Unit::Meter).unwrap_err().code, "UNREPRESENTABLE_SPIRAL");

    let outside_pi = make(
        "outside PI",
        [999899990., 0.],
        0.,
        (2. * std::f64::consts::PI / 3.) * 2. / (1. / 100000. + 1. / 99999.),
        Kind::Spiral { radius_start: Some(100000.), radius_end: Some(99999.), turn: Turn::Left },
    );
    assert!(landxml::export(&[outside_pi], Unit::Meter).is_ok());

    let rounded = make(
        "rounded PI",
        [1e8, 1e8],
        0.3,
        628418.4050142319,
        Kind::Spiral { radius_start: Some(100000.), radius_end: Some(99999.9599936363), turn: Turn::Left },
    );
    if let Ok(xml) = landxml::export(std::slice::from_ref(&rounded), Unit::Meter) {
        let result = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
        let restored = &result.alignments[0].definition;
        for fraction in [0.25, 0.5, 0.75, 1.] {
            let station = rounded.length().unwrap() * fraction;
            let a = rounded.at(station, 0.).unwrap().point;
            let b = restored.at(station, 0.).unwrap().point;
            assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6);
        }
    }
}
