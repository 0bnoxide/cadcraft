use cadcraft_alignment::{
    Alignment, Element, Kind, StationFormat, Turn,
    frames::{self, Options},
};

fn straight() -> Alignment {
    Alignment {
        name: "Road".into(),
        start_station: 0.,
        format: StationFormat::Thousands,
        elements: vec![Element { start: [0., 0.], direction: 0., length: 1000., kind: Kind::Line }],
    }
}
fn options() -> Options {
    Options { start: 0., end: 1000., paper_size: [250., 100.], scale: 1., spacing: Some(250.), increment: 10., vertex_buffer: 0., offset: 0. }
}
#[test]
fn fixed_and_automatic_straight_with_left_offset() {
    for spacing in [Some(250.), None] {
        for offset in [0., 5.] {
            let f = frames::tile(&straight(), &Options { spacing, offset, ..options() }).unwrap();
            assert_eq!(f.len(), 4);
            // 1000/250=4. Chord midpoint x=(start+end)/2, +X chord's left normal is +Y.
            for (i, frame) in f.iter().enumerate() {
                assert_eq!(frame.range, [i as f64 * 250., (i + 1) as f64 * 250.]);
                assert_eq!(frame.center, [125. + i as f64 * 250., offset]);
                assert_eq!(frame.angle, 0.);
            }
        }
    }
}
#[test]
fn buffer_retreats_and_next_spacing_uses_actual_previous_matchline() {
    let mut a = straight();
    a.elements =
        vec![Element { length: 255., ..a.elements[0].clone() }, Element { start: [255., 0.], direction: 0., length: 745., kind: Kind::Line }];
    let f = frames::tile(&a, &Options { paper_size: [300., 100.], vertex_buffer: 20., ..options() }).unwrap();
    // Candidate 250 is 5 from joint 255: retreat to floor((255-20)/10)*10=230.
    assert_eq!(f[0].range, [0., 230.]);
    assert_eq!(f[1].range, [230., 480.]);
}
#[test]
fn automatic_line_arc_line_has_independent_centres_and_contains_every_piece() {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
    let a = Alignment {
        elements: vec![
            Element { start: [0., 0.], direction: 0., length: 100., kind: Kind::Line },
            Element { start: [100., 0.], direction: 0., length: 100. * FRAC_PI_2, kind: Kind::Arc { radius: 100., turn: Turn::Left } },
            Element { start: [200., 100.], direction: FRAC_PI_2, length: 100., kind: Kind::Line },
        ],
        ..straight()
    };
    // R=100 quarter arc: end (200,100), arc length 50*pi. Width=200:
    // at 210 chord=(100+100*sin(1.1),100-100*cos(1.1)), length=196.86;
    // at 220 length=203.45, and later chords only grow. So first endpoint=210.
    let opts = Options { end: 200. + 100. * FRAC_PI_2, paper_size: [200., 200.], spacing: None, ..options() };
    let f = frames::tile(&a, &opts).unwrap();
    assert_eq!(f[0].range, [0., 210.]);
    let end = [100. + 100. * 1.1f64.sin(), 100. - 100. * 1.1f64.cos()];
    assert!((f[0].center[0] - end[0] / 2.).abs() < 1e-9);
    assert!((f[0].center[1] - end[1] / 2.).abs() < 1e-9);
    assert!((f[0].angle - end[1].atan2(end[0])).abs() < 1e-9);
    assert!(f[0].angle < FRAC_PI_4);
    assert_eq!(f.len(), 2);
    // The remaining arc ends at (200,100), then the last tangent ends at (200,200).
    // Its chord length is 145.77 < 200, so the second frame reaches the exact range end.
    assert_eq!(f[1].range, [210., 200. + 100. * FRAC_PI_2]);
    assert!((f[1].center[0] - (end[0] + 200.) / 2.).abs() < 1e-9);
    assert!((f[1].center[1] - (end[1] + 200.) / 2.).abs() < 1e-9);
    assert!((f[1].angle - (200. - end[1]).atan2(200. - end[0])).abs() < 1e-9);
    for frame in &f {
        frames::contains(&a, frame).unwrap();
    }
}
#[test]
fn hostile_settings_and_outside_pieces_refuse_without_panics() {
    for value in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(frames::tile(&straight(), &Options { scale: value, ..options() }).is_err());
        assert!(frames::tile(&straight(), &Options { increment: value, ..options() }).is_err());
        assert!(frames::tile(&straight(), &Options { paper_size: [value, 100.], ..options() }).is_err());
        assert!(frames::tile(&straight(), &Options { spacing: Some(value), ..options() }).is_err());
    }
    assert_eq!(frames::tile(&straight(), &Options { offset: 51., ..options() }).unwrap_err().code, "FRAME_OUTSIDE");
    assert_eq!(frames::tile(&straight(), &Options { spacing: Some(1.), ..options() }).unwrap_err().code, "NO_PROGRESS");
    assert!(frames::tile(&straight(), &Options { spacing: None, increment: 1e-300, ..options() }).is_err());
}
