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
    let b = &imported.alignments[0];
    assert_eq!(b.name, a.name);
    assert_eq!(b.start_station, a.start_station);
    for s in [-123.5, -100., -63.5, 0., 36.5] {
        let p = a.at(s, 0.).unwrap();
        let q = b.at(s, 0.).unwrap();
        assert!((p.point[0] - q.point[0]).hypot(p.point[1] - q.point[1]) < 1e-6);
    }
    let metric = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!((metric.alignments[0].length().unwrap() - a.length().unwrap() * 0.3048).abs() < 1e-6);
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
    assert!(r.warnings.iter().any(|w| w.code == "PROFILES_NOT_SUPPORTED"));
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
