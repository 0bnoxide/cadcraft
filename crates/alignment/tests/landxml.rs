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
