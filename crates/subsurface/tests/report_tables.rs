use cadcraft_subsurface::{Boring, BoringSet, Cell, boring::WellComponent, report};

#[test]
fn no_samples_keeps_header_and_blank_approval_fields_and_warns() {
    let set = BoringSet { borings: vec![Boring { boring_id: "B-1".into(), site_id: "SITE".into(), ..Default::default() }], ..Default::default() };
    let report = report::build(&set).unwrap();
    assert!(report.findings.iter().any(|f| f.code == "no_samples" && f.boring_id.as_deref() == Some("B-1")));
    let header = report.tables.iter().find(|t| t.name == "B-1 header").unwrap();
    assert!(header.rows.iter().any(|r| r.first() == Some(&Cell::Text("Site".into())) && r.get(1) == Some(&Cell::Text("SITE".into()))));
    let approval = report.tables.iter().find(|t| t.name == "B-1 approval").unwrap();
    assert_eq!(approval.header(), vec!["Role", "Name", "Signature", "Date"]);
    for (i, role) in ["Logged by", "Reviewed by", "Approved by"].into_iter().enumerate() {
        assert_eq!(approval.cell(i + 1, 0).text(), role);
        assert_eq!(approval.cell(i + 1, 1), &Cell::Empty);
        assert_eq!(approval.cell(i + 1, 2), &Cell::Empty);
        assert_eq!(approval.cell(i + 1, 3), &Cell::Empty);
    }
    let samples = report.tables.iter().find(|t| t.name == "B-1 samples").unwrap();
    assert_eq!(samples.rows.len(), 1);
}

#[test]
fn well_construction_is_sorted_by_depth_and_keeps_slot_size() {
    let set = BoringSet {
        borings: vec![Boring { boring_id: "B-1".into(), ..Default::default() }],
        wells: vec![
            WellComponent {
                boring_id: "B-1".into(),
                component: "screen".into(),
                top_depth: 10.0,
                bottom_depth: 20.0,
                slot_size: "0.010".into(),
                ..Default::default()
            },
            WellComponent { boring_id: "B-1".into(), component: "riser".into(), top_depth: 0.0, bottom_depth: 10.0, ..Default::default() },
        ],
        ..Default::default()
    };
    let report = report::build(&set).unwrap();
    let wells = report.tables.iter().find(|t| t.name == "B-1 wells").unwrap();
    assert_eq!(wells.cell(1, 2).text(), "riser");
    assert_eq!(wells.cell(2, 2).text(), "screen");
    assert_eq!(wells.cell(2, 5).text(), "0.010");
}
