use cadcraft_proposal::{
    deliverables::{DeliverableProfile, ExtraRole},
    manifest::Sheet,
};
use serde_json::json;
fn sheets() -> Vec<Sheet> {
    vec![
        Sheet { role: "SiteOverviewSheet".into(), path: "Sheets/1.dxf".into(), number: "S1".into(), title: "Site Overview".into() },
        Sheet { role: "ExistingConditionsSheet".into(), path: "Sheets/2.dxf".into(), number: "S2".into(), title: "Existing".into() },
    ]
}
#[test]
fn defaults_select_all_sheets_and_roles_control_extra_folders() {
    let p = DeliverableProfile::parse(b"{}").unwrap();
    assert!(!p.include_drawings);
    let outputs = p.select(&sheets()).unwrap();
    assert_eq!(outputs.iter().map(|s| s.pdf.as_str()).collect::<Vec<_>>(), ["sheets/S1_Site Overview.pdf", "sheets/S2_Existing.pdf"]);
    let p=DeliverableProfile::parse(br#"{"sheet_roles":["ExistingConditionsSheet"],"pdf_name":"{number}.pdf","include_drawings":true,"extra_files":[{"path":"a.pdf","role":"figure_pdf"},{"path":"b.csv","role":"data_csv"}]}"#).unwrap();
    assert_eq!(p.select(&sheets()).unwrap()[0].pdf, "sheets/S2.pdf");
    assert_eq!(p.extra_files[0].destination("a.pdf").unwrap(), "figures/a.pdf");
    assert_eq!(p.extra_files[1].destination("b.csv").unwrap(), "data/b.csv");
    assert_eq!(ExtraRole::FigurePng.folder(), "figures");
    assert_eq!(ExtraRole::QaReport.folder(), "qa");
    assert_eq!(ExtraRole::BoringLog.folder(), "data");
    assert_eq!(ExtraRole::Other.folder(), "");
}
#[test]
fn unknown_fields_tokens_roles_unsafe_names_and_collisions_refuse() {
    for v in [
        json!({"typo":true}),
        json!({"sheet_roles":["Missing"]}),
        json!({"pdf_name":"{missing}.pdf"}),
        json!({"pdf_name":"{number}.png"}),
        json!({"pdf_name":"../{number}.pdf"}),
        json!({"extra_files":[{"path":"a","role":"compliance_table"}]}),
        json!({"extra_files":[{"path":"","role":"other"}]}),
    ] {
        assert!(DeliverableProfile::parse(&serde_json::to_vec(&v).unwrap()).is_err(), "{v}");
    }
    let p = DeliverableProfile::parse(br#"{"pdf_name":"fixed.pdf"}"#).unwrap();
    assert_eq!(p.select(&sheets()).unwrap_err().code, "output_collision");
    let mut bad = sheets();
    bad[0].title = "bad/name".into();
    assert!(DeliverableProfile::parse(b"{}").unwrap().select(&bad).is_err());
    assert!(DeliverableProfile::parse(br#"{"sheet_roles":["ProposedSitePlanSheet"]}"#).unwrap().select(&sheets()).is_err());
}
