use cadcraft_alignment::{
    Alignment, Element, Kind, StationFormat, Turn,
    profile::{DesignProfile, Pvi},
    sampling,
};
fn route(length: f64) -> Alignment {
    Alignment {
        name: "route".into(),
        start_station: 0.,
        format: StationFormat::Thousands,
        elements: vec![Element { start: [0., 0.], direction: 0., length, kind: Kind::Line }],
    }
}
fn profile(e: f64) -> DesignProfile {
    DesignProfile {
        name: "design".into(),
        pvis: vec![
            Pvi { station: 0., elevation: 0., curve_length: None },
            Pvi { station: 200., elevation: e, curve_length: Some(200.) },
            Pvi { station: 400., elevation: 2., curve_length: None },
        ],
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}")
}
#[test]
fn crest_and_sag_known_answers() {
    // Crest g1=0.03,g2=-0.02,L200: PVC(100,3), z=3+0.03x-0.000125x².
    // Sag g1=-0.02,g2=0.03: PVC(100,-2), z=-2-0.02x+0.000125x².
    for (p, z, g, extreme) in [
        (profile(6.), [3., 4.1875, 4.75, 4.6875, 4.], [0.03, 0.0175, 0.005, -0.0075, -0.02], [220., 4.8]),
        (profile(-4.), [-2., -2.6875, -2.75, -2.1875, -1.], [-0.02, -0.0075, 0.005, 0.0175, 0.03], [180., -2.8]),
    ] {
        p.validate(&route(400.)).unwrap();
        for (i, s) in [100., 150., 200., 250., 300.].into_iter().enumerate() {
            let v = p.at(s).unwrap();
            close(v.elevation, z[i]);
            close(v.grade, g[i]);
        }
        let extrema = p.extrema().unwrap();
        assert_eq!(extrema.len(), 1);
        close(extrema[0].station, extreme[0]);
        close(extrema[0].elevation, extreme[1]);
        assert!(p.at(-1.).is_err());
    }
}
#[test]
fn profile_validation_and_later_tangent_rule() {
    let mut p = profile(6.);
    p.pvis[1].curve_length = None;
    close(p.at(200.).unwrap().grade, -0.02);
    for l in [0., -1., f64::NAN, 500.] {
        p.pvis[1].curve_length = Some(l);
        assert!(p.validate(&route(400.)).is_err());
    }
    p = profile(6.);
    p.pvis[2].station = 200.;
    assert!(p.validate(&route(400.)).is_err());
}
#[test]
fn exact_tin_crossings_and_gaps() {
    let mut a = route(20.);
    a.elements[0].start = [-5., 5.];
    let vertices = [[0., 0., 0.], [10., 0., 10.], [10., 10., 30.], [0., 10., 20.]];
    let g = sampling::sample(&a, &vertices, &[[0, 1, 2], [0, 2, 3]], 0.001).unwrap();
    assert_eq!(g.segments.len(), 1);
    assert_eq!(g.segments[0].len(), 3);
    for (p, (s, z)) in g.segments[0].iter().zip([(5., 10.), (10., 15.), (15., 20.)]) {
        close(p.station, s);
        close(p.elevation, z);
    }
    let mut a = route(8.);
    a.elements[0].start = [-1., 0.];
    let vertices = [[0., -1., 0.], [2., -1., 0.], [2., 1., 0.], [0., 1., 0.], [4., -1., 0.], [6., -1., 0.], [6., 1., 0.], [4., 1., 0.]];
    let g = sampling::sample(&a, &vertices, &[[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]], 0.001).unwrap();
    assert_eq!(g.segments.len(), 2);
    close(g.segments[0].last().unwrap().station, 3.);
    close(g.segments[1][0].station, 5.);
}
#[test]
fn arc_ground_is_not_end_to_end_linear_interpolation() {
    let mut a = route(1.);
    a.elements[0] = Element {
        start: [1., 0.],
        direction: std::f64::consts::FRAC_PI_2,
        length: std::f64::consts::FRAC_PI_2,
        kind: Kind::Arc { radius: 1., turn: Turn::Left },
    };
    let g = sampling::sample(&a, &[[-2., -2., -2.], [4., -2., 4.], [-2., 4., -2.]], &[[0, 1, 2]], 0.0001).unwrap();
    let mid = std::f64::consts::FRAC_PI_4;
    let pair = g.segments[0].windows(2).find(|p| p[0].station <= mid && p[1].station >= mid).unwrap();
    let t = (mid - pair[0].station) / (pair[1].station - pair[0].station);
    let z = pair[0].elevation + t * (pair[1].elevation - pair[0].elevation);
    assert!((z - std::f64::consts::FRAC_1_SQRT_2).abs() <= 0.0001);
    assert!(sampling::sample(&a, &[[0., 0., 0.]], &[[0, 1, 2]], 0.1).is_err());
    assert!(sampling::sample(&a, &[[0., 0., 0.]; 3], &[[0, 1, 2]], 0.).is_err());
}

#[test]
fn profiles_roundtrip_with_standard_gap_representation() {
    use cadcraft_alignment::{
        landxml::{self, Route, Unit},
        profile::{GroundProfile, Profiles, Sample},
    };
    let profiles = Profiles {
        design: Some(profile(6.)),
        ground: Some(GroundProfile {
            name: "Ground".into(),
            segments: vec![
                vec![Sample { station: 0., elevation: 1. }, Sample { station: 100., elevation: 2. }],
                vec![Sample { station: 300., elevation: 3. }, Sample { station: 400., elevation: 4. }],
            ],
        }),
    };
    let xml = landxml::export_routes(&[Route { definition: route(400.), profiles: profiles.clone() }], Unit::Meter).unwrap();
    assert_eq!(xml.matches("<PntList2D>").count(), 2);
    let imported = landxml::import(xml.as_bytes(), Unit::Meter).unwrap();
    assert!(imported.refusals.is_empty(), "{:?}", imported.refusals);
    assert_eq!(imported.alignments[0].profiles, profiles);
    let bad = xml.replace("ParaCurve", "CircCurve");
    let refused = landxml::import(bad.as_bytes(), Unit::Meter).unwrap();
    assert!(refused.alignments.is_empty());
    assert_eq!(refused.refusals[0].code, "UNSUPPORTED_VERTICAL_CURVE");
}

#[test]
fn two_hundred_thousand_triangles_and_fifty_elements_fit_work_budget() {
    let mut vertices = Vec::new();
    for y in 0..=200 {
        for x in 0..=500 {
            vertices.push([f64::from(x), f64::from(y), f64::from(x + y)]);
        }
    }
    let mut triangles = Vec::new();
    for y in 0..200u32 {
        for x in 0..500u32 {
            let a = y * 501 + x;
            triangles.extend([[a, a + 1, a + 502], [a, a + 502, a + 501]]);
        }
    }
    let mut a = route(500.);
    a.elements = (0..50).map(|i| Element { start: [f64::from(i) * 10., 100.25], direction: 0., length: 10., kind: Kind::Line }).collect();
    let g = sampling::sample(&a, &vertices, &triangles, 0.001).unwrap();
    assert_eq!(g.segments.len(), 1);
    close(g.segments[0].last().unwrap().station, 500.);
    close(g.segments[0].last().unwrap().elevation, 600.25);
}

#[test]
fn view_keeps_gaps_and_exaggerates_without_changing_stations() {
    use cadcraft_alignment::{
        profile::{GroundProfile, Profiles, Sample},
        view::{self, Mark},
    };
    let a = route(400.);
    let p = Profiles {
        design: Some(profile(6.)),
        ground: Some(GroundProfile {
            name: "Ground".into(),
            segments: vec![
                vec![Sample { station: 0., elevation: 1. }, Sample { station: 100., elevation: 2. }],
                vec![Sample { station: 300., elevation: 3. }, Sample { station: 400., elevation: 4. }],
            ],
        }),
    };
    let marks = view::draw(&a, &p, [1000., 2000.], 2.).unwrap();
    let ground = marks
        .iter()
        .filter_map(|m| match m {
            Mark::Polyline { layer: "PROFILE-GROUND", points } => Some(points),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ground.len(), 2);
    close(ground[0][0][0], 1000.);
    close(ground[0][1][0], 1100.);
    close(ground[0][1][1] - ground[0][0][1], 2.);
    assert!(marks.iter().any(|m| matches!(m, Mark::Text { .. })));
    assert!(view::draw(&a, &Profiles::default(), [0., 0.], 1.).is_err());
    assert!(view::draw(&a, &p, [0., 0.], 0.).is_err());
}

#[test]
fn true_arc_and_spiral_edge_crossings_are_retained() {
    let mut a = route(1.);
    a.elements[0] = Element {
        start: [1., 0.],
        direction: std::f64::consts::FRAC_PI_2,
        length: std::f64::consts::FRAC_PI_2,
        kind: Kind::Arc { radius: 1., turn: Turn::Left },
    };
    let g = sampling::sample(&a, &[[-1., -1., -1.], [0.5, -1., 0.5], [0.5, 2., 0.5], [-1., 2., -1.]], &[[0, 1, 2], [0, 2, 3]], 0.001).unwrap();
    close(g.segments[0][0].station, std::f64::consts::PI / 3.);
    // Half of L60/R300 entry is L30/R600: classic Fresnel x with t=.025.
    let t = 0.025f64;
    let x = 30. * (1. - t.powi(2) / 10. + t.powi(4) / 216. - t.powi(6) / 9360.);
    let mut a = route(60.);
    a.elements[0].kind = Kind::Spiral { radius_start: None, radius_end: Some(300.), turn: Turn::Left };
    let g = sampling::sample(&a, &[[-1., -1., -1.], [x, -1., x], [x, 3., x], [-1., 3., -1.]], &[[0, 1, 2], [0, 2, 3]], 0.001).unwrap();
    close(g.segments[0].last().unwrap().station, 30.);
}

#[test]
fn isolated_vertex_contact_is_a_point_segment_not_a_bridge() {
    let mut a = route(2.);
    a.elements[0].start = [-1., 0.];
    let g = sampling::sample(&a, &[[0., 0., 7.], [1., 1., 7.], [-1., 1., 7.]], &[[0, 1, 2]], 0.001).unwrap();
    assert_eq!(g.segments.len(), 1);
    assert_eq!(g.segments[0].len(), 1);
    close(g.segments[0][0].station, 1.);
    close(g.segments[0][0].elevation, 7.);
}

#[test]
fn a_narrow_gap_is_never_merged_by_crossing_deduplication() {
    let mut a = route(3.);
    a.elements[0].start = [0., 0.];
    let x = 1. + 5e-10;
    let vertices = [[0., -1., 0.], [1., -1., 0.], [1., 1., 0.], [0., 1., 0.], [x, -1., 0.], [2., -1., 0.], [2., 1., 0.], [x, 1., 0.]];
    let g = sampling::sample(&a, &vertices, &[[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]], 0.01).unwrap();
    assert_eq!(g.segments.len(), 2);
    assert!(g.segments[0].last().unwrap().station < g.segments[1][0].station);
}

#[test]
fn curves_may_abut_but_never_overlap_and_flat_curves_have_no_extremum() {
    let mut p = profile(6.);
    p.pvis[2].curve_length = Some(200.);
    p.pvis.push(Pvi { station: 600., elevation: 3., curve_length: None });
    p.validate(&route(600.)).unwrap();
    close(p.at(300.).unwrap().grade, -0.02);
    p.pvis[1].curve_length = Some(201.);
    p.pvis[2].curve_length = Some(201.);
    assert_eq!(p.validate(&route(600.)).unwrap_err().code, "OVERLAPPING_CURVES");
    let mut p = profile(2.);
    p.pvis[2].elevation = 4.;
    assert!(p.extrema().unwrap().is_empty());
    p.pvis[2].station = 401.;
    assert!(p.validate(&route(400.)).is_err());
    for value in [f64::NAN, f64::INFINITY] {
        p.pvis[0].station = value;
        assert!(p.validate(&route(500.)).is_err());
    }
}
