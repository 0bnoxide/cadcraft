use cadcraft_labels::{
    Pt,
    callout::{Cell, layout, leader_attachment},
};

#[test]
fn spanning_title_does_not_get_sliced_by_column_grid() {
    let cells = vec![
        Cell { row: 0, col: 0, span: 2, text: "MW-5".into(), width: 12.0, height: 2.0 },
        Cell { row: 1, col: 0, span: 1, text: "Lead".into(), width: 4.0, height: 2.0 },
        Cell { row: 1, col: 1, span: 1, text: "6.4".into(), width: 3.0, height: 2.0 },
    ];
    let g = layout(&cells, 1.0).unwrap();
    assert_eq!(g.columns.len(), 2);
    assert!(g.bounds.width() >= 14.0);
    assert_eq!(g.lines.len(), 6);
    let p = leader_attachment(g.bounds, Pt { x: -10.0, y: 0.0 });
    assert_eq!(p.x, 0.0);
}
