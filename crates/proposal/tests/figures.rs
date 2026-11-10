use cadcraft_proposal::figures::{FigureSpec, series};
use serde_json::json;

fn spec(id: &str) -> FigureSpec {
    FigureSpec::parse(&serde_json::to_vec(&json!({"figure_id":id,"source_drawing":"a.dxf","layout":"Figure"})).unwrap()).unwrap()
}

#[test]
fn four_series_orders_and_deduplication_are_stable() {
    let sites = vec!["A / B".into(), "C".into(), "A / B".into()];
    let events = vec!["2025".into(), "2026".into()];
    let specs = vec![spec("F1"), spec("F2")];
    let jobs = series(&specs, &sites, &events, "per_site", "pdf").unwrap();
    assert_eq!(jobs.len(), 8);
    assert_eq!(
        jobs.iter().map(|j| j.filename.as_str()).collect::<Vec<_>>(),
        [
            "A_B_2025_F1.pdf",
            "A_B_2025_F2.pdf",
            "A_B_2026_F1.pdf",
            "A_B_2026_F2.pdf",
            "C_2025_F1.pdf",
            "C_2025_F2.pdf",
            "C_2026_F1.pdf",
            "C_2026_F2.pdf"
        ]
    );
    assert_eq!(serde_json::to_vec(&jobs).unwrap(), serde_json::to_vec(&series(&specs, &sites, &events, "per_site", "pdf").unwrap()).unwrap());
    let jobs = series(&specs, &sites, &events, "per_map_type", "png").unwrap();
    assert_eq!(jobs[0].filename, "F1_A_B_2025.png");
    assert_eq!(jobs[2].filename, "F1_C_2025.png");
    assert_eq!(jobs[4].filename, "F2_A_B_2025.png");
    let jobs = series(&specs, &sites, &events, "historical", "pdf").unwrap();
    assert_eq!(jobs[0].filename, "A_B_F1_2025.pdf");
    assert_eq!(jobs[2].filename, "C_F1_2025.pdf");
    assert_eq!(jobs[4].filename, "A_B_F1_2026.pdf");
    assert_eq!(series(&specs, &sites, &events, "combined_appendix", "pdf").unwrap()[7].filename, "Appendix_008_C_2026_F2.pdf");
}

#[test]
fn malformed_specs_empty_matrices_and_colliding_names_refuse() {
    for bad in [
        json!({"figure_id":"F","source_drawing":"a.dxf"}),
        json!({"figure_id":"F","source_drawing":"a.dxf","layout":" "}),
        json!({"figure_id":"F","source_drawing":"a.dxf","layout":"L","unknown":1}),
        json!({"figure_id":"F","source_drawing":"a.dxf","layout":"L","text":{"Site":"override"}}),
        json!({"figure_id":"F","source_drawing":"a.dxf","layout":"L","visible_layers":["x"],"hidden_layers":["X"]}),
        json!({"figure_id":"F","source_drawing":"a.dxf","layout":"L","extent_buffer_pct":-1}),
    ] {
        assert!(FigureSpec::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let specs = vec![spec("F")];
    assert!(series(&specs, &[], &["E".into()], "per_site", "pdf").is_err());
    assert!(series(&specs, &[" ".into()], &["E".into()], "per_site", "pdf").is_err());
    assert!(series(&specs, &["A/B".into(), "A_B".into()], &["E".into()], "per_site", "pdf").is_err());
    assert!(series(&[spec("F"), spec("f")], &["A".into()], &["E".into()], "per_site", "pdf").is_err());
    assert!(series(&specs, &["A".into()], &["E".into()], "other", "pdf").is_err());
    assert!(series(&specs, &["A".into()], &["E".into()], "per_site", "jpeg").is_err());
    assert!(series(&specs, &vec!["A".into(); 1001], &["E".into()], "per_site", "pdf").is_err());
}
