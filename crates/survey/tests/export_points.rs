//! Behaviour fixtures ported from AutoGIS survey export, civil3d_points and cad_layer_map tests.
use cadcraft_survey::{Severity, SurveyPoint, export::*};
use std::collections::BTreeMap;

fn point(id: &str, code: &str) -> SurveyPoint {
    SurveyPoint { id: id.into(), feature_code: code.into(), northing: 2000.0, easting: 1000.0, elevation: 50.0, description: "well head".into() }
}

#[test]
fn mapped_layers_keep_all_points_and_default_codes_warn_once() {
    let points = vec![point("1", "MW"), point("2", "GCP"), point("3", "MW"), point("4", "XYZ"), point("5", ""), point("6", "XYZ")];
    let map = BTreeMap::from([("MW".into(), "MonitoringWells".into()), ("GCP".into(), "DroneControlPoints".into())]);
    let (layers, findings) = group_points_by_layer(&points, &map).unwrap();
    assert_eq!(layers["MonitoringWells"].len(), 2);
    assert_eq!(layers["DroneControlPoints"].len(), 1);
    assert_eq!(layers["Miscellaneous"].len(), 3);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.starts_with("unmapped_feature_codes:"));
    let map = BTreeMap::from([("default".into(), "Other".into())]);
    let (layers, findings) = group_points_by_layer(&[point("1", "")], &map).unwrap();
    assert_eq!(layers["Other"].len(), 1);
    assert!(findings.is_empty());
}

#[test]
fn unsafe_layer_names_merge_without_losing_points() {
    let map = BTreeMap::from([("A".into(), "../x".into()), ("B".into(), "..\\x".into())]);
    let (layers, findings) = group_points_by_layer(&[point("1", "A"), point("2", "B")], &map).unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[".._x"].len(), 2);
    assert!(findings.iter().any(|f| f.message.starts_with("unsafe_layer_name:")));
    for name in ["../../etc/pwn", "C:escape", "CON", "NUL.txt", "trailing. ", "", ".", ".."] {
        let (layers, _) = group_points_by_layer(&[point("1", "A")], &BTreeMap::from([("A".into(), name.into())])).unwrap();
        assert!(layers.keys().all(|n| !n.contains(['/', '\\', ':']) && !n.ends_with(['.', ' ']) && n != "." && n != ".." && !n.is_empty()));
    }
}

#[test]
fn layer_and_pnezd_csv_keep_axis_order_and_escape_fields() {
    let mut p = point("MW-1", "MW");
    p.description = "head, \"north\"\nsecond line".into();
    let csv = layer_csv(&[p.clone()]).unwrap();
    assert!(csv.starts_with("point_id,x,y,z,feature_code,description\nMW-1,1000,2000,50,MW,"));
    assert!(csv.contains("\"head, \"\"north\"\"\nsecond line\""));
    let (points, findings) = read_points_csv(csv.as_bytes()).unwrap();
    assert_eq!(points, [p.clone()]);
    assert!(findings.is_empty());
    let csv = pnezd_csv(&[p.clone(), point("MW-2", "MW")], 100).unwrap();
    assert!(csv.starts_with("point_number,northing,easting,elevation,description\n100,2000,1000,50,"));
    assert!(csv.contains("\n101,2000,1000,50,well head\n"));
    assert!(pnezd_csv(&[p.clone(), p], u64::MAX).is_err());
}

#[test]
fn layer_csv_preserves_an_intentionally_blank_description() {
    let mut p = point("MW-1", "MW");
    p.description.clear();
    let (points, _) = read_points_csv(layer_csv(&[p.clone()]).unwrap().as_bytes()).unwrap();
    assert_eq!(points, [p]);
}

#[test]
fn missing_elevation_is_skipped_with_contiguous_pnezd_numbers() {
    let csv = "location_id,x,y,z,description\nMW-1,1000,2000,512.5,\nMW-2,1,2,,\nMW-3,3,4,5,third\n";
    let (points, findings) = read_points_csv(csv.as_bytes()).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].description, "MW-1");
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].message.starts_with("missing_elevation:"));
    assert!(pnezd_csv(&points, 1).unwrap().contains("\n2,4,3,5,third\n"));
}

#[test]
fn landxml_carries_nez_metadata_and_escapes_attributes() {
    let mut p = point("MW<&\"", "MW");
    p.description = "well & head".into();
    let xml = landxml(&[p], &XmlOptions { crs: "EPSG:2256", linear_unit: "foot", date: "2026-11-03", time: "12:00:00" }).unwrap();
    assert!(xml.contains("xmlns=\"http://www.landxml.org/schema/LandXML-1.2\""));
    assert!(xml.contains("epsgCode=\"2256\""));
    assert!(xml.contains("linearUnit=\"foot\""));
    assert!(xml.contains("name=\"MW&lt;&amp;&quot;\" code=\"MW\" desc=\"well &amp; head\">2000 1000 50</CgPoint>"));
    for (crs, unit) in [("NAD83 State Plane", "foot"), ("", "foot"), ("EPSG:2256", ""), ("EPSG:2256", "feet")] {
        assert!(landxml(&[], &XmlOptions { crs, linear_unit: unit, date: "2026-11-03", time: "12:00:00" }).is_err());
    }
}

#[test]
fn mapping_resolution_preserves_zero_color_and_checks_selected_layers_and_crs() {
    let config = BTreeMap::from([
        ("wells".into(), LayerStyle { cad_layer: "V-WELL".into(), color: Some(0), linetype: Some("DASHED".into()) }),
        ("extra".into(), LayerStyle { cad_layer: "X".into(), ..Default::default() }),
    ]);
    let plan = resolve_cad_plan(&["wells".into(), "orphan".into()], &config, "");
    assert_eq!(plan.mappings.len(), 1);
    assert_eq!(plan.mappings[0].1.color, Some(0));
    assert_eq!(plan.unmapped, ["orphan"]);
    assert!(plan.findings.iter().all(|f| f.severity == Severity::Error));
    assert!(plan.findings[0].message.starts_with("missing_crs:"));
    assert!(mapping_csv(&plan).contains("wells,V-WELL,0,DASHED"));
    assert!(resolve_cad_plan(&["wells".into()], &config, "EPSG:2256").findings.is_empty());
}

#[test]
fn hostile_csv_and_coordinates_are_rejected_without_panics() {
    for csv in ["x,y,z\n1,2,3\n", "point_id,x,y,z\nA,NaN,2,3\n", "point_id,x,y,z\n\"unterminated,1,2,3", "point_id,x,y,z\nA,1,2,3,extra\n"] {
        assert!(read_points_csv(csv.as_bytes()).is_err(), "{csv}");
    }
    let mut p = point("1", "A");
    p.elevation = f64::NAN;
    assert!(layer_csv(&[p]).is_err());
    let huge = format!("point_id,x,y,z\n{},1,2,3", "x".repeat(1_048_577));
    assert!(read_points_csv(huge.as_bytes()).is_err());
}

#[test]
fn layer_findings_and_colliding_points_keep_first_seen_order() {
    let mapping = BTreeMap::from([("A".into(), "../x".into()), ("B".into(), "..\\x".into())]);
    let (layers, findings) = group_points_by_layer(&[point("B-first", "B"), point("A-second", "A")], &mapping).unwrap();
    assert_eq!(layers[".._x"].iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["B-first", "A-second"]);
    assert!(findings[0].message.contains("..\\\\x"));
}

#[test]
fn csv_stops_at_a_million_rows_and_propagates_read_failures() {
    let csv = format!("point_id,x,y,z\n{}", "p,0,0,\n".repeat(1_000_001));
    let error = read_points_csv(csv.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("1,000,000"));
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("reader failed"))
        }
    }
    assert!(read_points_csv(Broken).unwrap_err().to_string().contains("reader failed"));
}

#[test]
fn landxml_refuses_invalid_calendar_metadata() {
    for (date, time) in [("tomorrow", "noon"), ("2026-02-29", "12:00:00"), ("2026-11-03", "25:00:00")] {
        assert!(landxml(&[], &XmlOptions { crs: "EPSG:2256", linear_unit: "foot", date, time }).is_err());
    }
    assert!(landxml(&[], &XmlOptions { crs: "EPSG:2256", linear_unit: "meter", date: "2024-02-29", time: "23:59:59" }).is_ok());
}
