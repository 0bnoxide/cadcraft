//! LandXML 1.2 export shape.

use cadcraft_surface::Tin;
use cadcraft_surface::landxml::{ExportOptions, LinearUnit, export};

fn tin() -> Tin {
    Tin { vertices: vec![[100.0, 200.0, 1.5], [110.0, 200.0, 2.0], [100.0, 210.0, 2.5]], triangles: vec![[0, 1, 2]] }
}

fn opts() -> ExportOptions {
    ExportOptions { name: "EG <A&B>".into(), unit: LinearUnit::Meter, epsg: 26913, unix_time: None }
}

#[test]
fn writes_northing_easting_elevation_with_one_based_ids() {
    let x = export(&tin(), &opts()).unwrap();
    assert!(x.contains(r#"<LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2">"#));
    assert!(x.contains(r#"<Metric linearUnit="meter" elevationUnit="meter""#));
    assert!(x.contains(r#"epsgCode="26913""#));
    assert!(x.contains(r#"<Surface name="EG &lt;A&amp;B&gt;">"#));
    assert!(x.contains(r#"<P id="2">200 110 2</P>"#));
    assert!(x.contains("<F>1 2 3</F>"));
    let ft = export(&tin(), &ExportOptions { unit: LinearUnit::UsSurveyFoot, ..opts() }).unwrap();
    assert!(ft.contains(r#"<Imperial linearUnit="USSurveyFoot" elevationUnit="feet""#));
}

#[test]
fn refuses_what_the_handoff_contract_would_reject() {
    assert!(export(&Tin::default(), &opts()).is_err());
    assert!(export(&tin(), &ExportOptions { epsg: 0, ..opts() }).is_err());
    assert!(export(&tin(), &ExportOptions { name: "  ".into(), ..opts() }).is_err());
    assert!(export(&tin(), &ExportOptions { name: "a\nb".into(), ..opts() }).is_err());
    let mut t = tin();
    t.triangles.push([0, 1, 7]);
    assert!(export(&t, &opts()).is_err());
    let mut t = tin();
    t.vertices[2] = [120.0, 200.0, 0.0]; // collinear
    assert!(export(&t, &opts()).is_err());
    let mut t = tin();
    t.vertices[0][2] = f64::NAN;
    assert!(export(&t, &opts()).is_err());
}
