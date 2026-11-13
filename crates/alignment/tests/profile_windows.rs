use cadcraft_alignment::{
    Alignment, Element, Kind, StationFormat,
    profile::{DesignProfile, GroundProfile, Profiles, Pvi, Sample},
    view::{self, Adjustment, Center, Mark, Profile},
};
fn fixture(start: f64) -> (Alignment, Profiles) {
    (
        Alignment {
            name: "Road".into(),
            start_station: start,
            format: StationFormat::Hundreds,
            elements: vec![Element { start: [0., 0.], direction: 0., length: 1000., kind: Kind::Line }],
        },
        Profiles {
            design: None,
            ground: Some(GroundProfile {
                name: "Ground".into(),
                segments: vec![vec![
                    Sample { station: start, elevation: 100. },
                    Sample { station: start + 400., elevation: 120. },
                    Sample { station: start + 1000., elevation: 90. },
                ]],
            }),
        },
    )
}
#[test]
fn median_datum_uses_clipped_vertices_and_interpolated_endpoints() {
    let (a, p) = fixture(0.);
    let w = view::adjust(
        &a,
        &p,
        [250., 500.],
        &Adjustment { height: 40., profile: Profile::Ground, center: Center::Median, increment: 5., start_buffer: 0., end_buffer: 0. },
    )
    .unwrap();
    // At 250:100+.05*250=112.5; vertex at 400:120; at 500:120-.05*100=115.
    // Sorted [112.5,115,120], median=115. Datum=floor((115-20)/5)*5=95.
    assert_eq!(w.range, [250., 500.]);
    assert_eq!(w.elevation, [95., 135.]);
    let result = view::draw_range(&a, &p, [1000., 2000.], 2., w.range, Some(w.elevation)).unwrap();
    let points = result
        .marks
        .iter()
        .find_map(|m| match m {
            Mark::Polyline { points, .. } => Some(points),
            _ => None,
        })
        .unwrap();
    assert_eq!(points.first().unwrap(), &[1000., 2035.]);
    assert_eq!(points.last().unwrap(), &[1250., 2040.]);
}
#[test]
fn design_median_uses_structural_vertices_instead_of_render_samples() {
    let (a, mut p) = fixture(0.);
    p.design = Some(DesignProfile {
        name: "Design".into(),
        pvis: vec![
            Pvi { station: 0., elevation: 0., curve_length: None },
            Pvi { station: 500., elevation: 100., curve_length: Some(800.) },
            Pvi { station: 1000., elevation: 0., curve_length: None },
        ],
    });
    // Grades +.2,-.2; PVC/PVT = 100/900 at z20; apex at 500 has z60.
    // Structural elevations at stations [0,100,500,900,1000] = [0,20,60,20,0].
    // The apex is already a vertex: count it once. Sorted median=20, datum=20-40/2=0.
    let settings = Adjustment { height: 40., profile: Profile::Design, center: Center::Median, increment: 1., start_buffer: 0., end_buffer: 0. };
    assert_eq!(view::adjust(&a, &p, [0., 1000.], &settings).unwrap().elevation, [0., 40.]);
    // Exact curved ends at 200/800: 20+.2*100-.00025*100^2 = 37.5.
    // Clipped structural elevations [37.5,60,37.5], median=37.5, datum=floor(17.5)=17.
    assert_eq!(view::adjust(&a, &p, [200., 800.], &settings).unwrap().elevation, [17., 57.]);
}
#[test]
fn grid_starts_at_an_absolute_round_station() {
    for (start, first_x) in [(1234.5, 65.5), (1200., 0.)] {
        let (a, p) = fixture(start);
        let f = view::draw_range(&a, &p, [0., 0.], 1., [start, start + 1000.], None).unwrap();
        let x = f
            .marks
            .iter()
            .find_map(|m| match m {
                Mark::Line { a, b } if a[0] == b[0] => Some(a[0]),
                _ => None,
            })
            .unwrap();
        assert_eq!(x, first_x); // step=100, ceil(1234.5/100)*100=1300.
    }
}
#[test]
fn clipping_is_reported_and_selected_ground_gaps_refuse() {
    let (a, mut p) = fixture(0.);
    let f = view::draw_range(&a, &p, [0., 0.], 1., [0., 1000.], Some([105., 110.])).unwrap();
    assert!(!f.clipped.is_empty());
    assert!(f.clipped.iter().all(|v| v.profile == "ground"));
    for m in f.marks {
        if let Mark::Polyline { points, .. } = m {
            assert!(points.iter().all(|p| p[1] >= 0. && p[1] <= 5.));
        }
    }
    p.ground.as_mut().unwrap().segments = vec![
        vec![Sample { station: 0., elevation: 100. }, Sample { station: 400., elevation: 100. }],
        vec![Sample { station: 500., elevation: 100. }, Sample { station: 1000., elevation: 100. }],
    ];
    assert_eq!(
        view::adjust(
            &a,
            &p,
            [0., 1000.],
            &Adjustment { height: 40., profile: Profile::Ground, center: Center::Median, increment: 5., start_buffer: 0., end_buffer: 0. }
        )
        .unwrap_err()
        .code,
        "PROFILE_GAP"
    );
}
