use cadcraft_labels::format::{LabelExpressionType, MAX_LABEL_LEN, Part, Props, derive_label_fields, format, format_number};
use serde_json::json;

fn props(v: serde_json::Value) -> Props {
    match v {
        serde_json::Value::Object(m) => m,
        _ => Props::new(),
    }
}

fn num(key: &str, decimals: u8) -> Part {
    Part::Number { key: key.into(), decimals, round_to: None, suffix: String::new(), trim_zeros: false }
}

#[test]
fn renders_all_part_kinds_in_order() {
    let parts = vec![
        Part::Prop { key: "id".into() },
        Part::Newline,
        Part::Text { text: "EL ".into() },
        Part::Number { key: "z".into(), decimals: 2, round_to: None, suffix: " ft".into(), trim_zeros: false },
        Part::Text { text: " #".into() },
        Part::Counter { start: 1, step: 1, width: 3 },
    ];
    let p = props(json!({"id": "MW-1", "z": 101.256}));
    assert_eq!(format(&parts, &p, 4), "MW-1\nEL 101.26 ft #005");
}

#[test]
fn prop_renders_strings_numbers_bools() {
    let p = props(json!({"s": "a", "n": 2.5, "i": 7, "b": true}));
    let parts: Vec<Part> = ["s", "n", "i", "b"].iter().map(|k| Part::Prop { key: (*k).into() }).collect();
    assert_eq!(format(&parts, &p, 0), "a2.57true");
}

#[test]
fn missing_and_non_numeric_render_empty_and_skip_suffix() {
    let p = props(json!({"txt": "abc", "nul": null}));
    let parts = vec![
        Part::Text { text: "[".into() },
        Part::Prop { key: "nope".into() },
        Part::Prop { key: "nul".into() },
        Part::Number { key: "txt".into(), decimals: 1, round_to: None, suffix: "m".into(), trim_zeros: false },
        Part::Number { key: "nope".into(), decimals: 1, round_to: None, suffix: "m".into(), trim_zeros: false },
        Part::Text { text: "]".into() },
    ];
    assert_eq!(format(&parts, &p, 0), "[]");
}

#[test]
fn numeric_strings_parse() {
    let p = props(json!({"z": " 12.345 "}));
    assert_eq!(format(&[num("z", 1)], &p, 0), "12.3");
}

#[test]
fn rounding_trimming_and_negative_zero() {
    assert_eq!(format_number(12.26, 2, Some(0.5), false).as_deref(), Some("12.50"));
    assert_eq!(format_number(12.26, 2, Some(0.5), true).as_deref(), Some("12.5"));
    assert_eq!(format_number(2.0, 3, None, true).as_deref(), Some("2"));
    assert_eq!(format_number(120.0, 0, None, true).as_deref(), Some("120"));
    assert_eq!(format_number(-0.001, 2, None, false).as_deref(), Some("0.00"));
    assert_eq!(format_number(-0.4, 0, None, false).as_deref(), Some("0"));
    assert_eq!(format_number(-1.5, 1, None, false).as_deref(), Some("-1.5"));
}

#[test]
fn hostile_numbers_never_panic() {
    for r in [Some(0.0), Some(-1.0), Some(f64::NAN), Some(f64::INFINITY)] {
        assert_eq!(format_number(1.0, 2, r, false), None);
    }
    assert_eq!(format_number(f64::NAN, 2, None, false), None);
    assert_eq!(format_number(f64::INFINITY, 2, None, false), None);
    assert_eq!(format_number(f64::MAX, 2, Some(1e-300), false), None);
    // Decimals clamp to 12.
    assert_eq!(format_number(1.0, 255, None, false).as_deref(), Some("1.000000000000"));
    assert!(format_number(f64::MAX, 12, None, false).is_some());
}

#[test]
fn counter_steps_pads_and_overflows_to_empty() {
    let c = |start, step, width| vec![Part::Counter { start, step, width }];
    assert_eq!(format(&c(10, 5, 0), &Props::new(), 2), "20");
    assert_eq!(format(&c(0, -1, 3), &Props::new(), 4), "-04");
    assert_eq!(format(&c(i64::MAX, 1, 0), &Props::new(), 1), "");
    assert_eq!(format(&c(0, 2, 0), &Props::new(), u64::MAX), "");
    assert_eq!(format(&c(1, 1, 255), &Props::new(), 0).len(), 20);
}

#[test]
fn output_is_bounded_at_a_char_boundary() {
    let parts = vec![Part::Text { text: "é".repeat(MAX_LABEL_LEN) }, Part::Text { text: "tail".into() }];
    let out = format(&parts, &Props::new(), 0);
    assert!(out.len() <= MAX_LABEL_LEN && !out.contains("tail"));
}

#[test]
fn parts_round_trip_through_json_with_defaults() {
    let parts: Vec<Part> = serde_json::from_value(json!([
        {"kind": "text", "text": "N="},
        {"kind": "number", "key": "n", "decimals": 1},
        {"kind": "counter"},
        {"kind": "newline"}
    ]))
    .unwrap_or_default();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts.get(2), Some(&Part::Counter { start: 0, step: 1, width: 0 }));
    let back: Vec<Part> = serde_json::to_value(&parts).and_then(serde_json::from_value).unwrap_or_default();
    assert_eq!(back, parts);
}

// Ported from AutoGIS tests/envmon/test_label_expression_common.py.
#[test]
fn derive_label_fields_matches_reference() {
    let f = derive_label_fields("Benzene", "");
    assert_eq!(f.value_field, "Benzene_Value");
    assert_eq!(f.units_field, "Benzene_Units");
    assert_eq!(f.sl_field, "Benzene_SL");
    assert_eq!(f.id_field, "LocationID");
    assert_eq!(f.layer_base, "Benzene");

    let f = derive_label_fields("Benzene", "Env_");
    assert_eq!(f.value_field, "Env_Benzene_Value");
    assert_eq!(f.units_field, "Env_Benzene_Units");
    assert_eq!(f.sl_field, "Env_Benzene_SL");
    assert_eq!(f.id_field, "Env_LocationID");

    let f = derive_label_fields("cis-1,2-DCE/PCE", "");
    assert_eq!(f.layer_base, "cis-12-DCE_PCE");
    assert!(!f.layer_base.contains([' ', ',', '/']));
    assert_eq!(derive_label_fields("Total Lead", "").layer_base, "Total_Lead");
}

#[test]
fn label_expression_type_names_match_reference() {
    let names: Vec<String> = [
        LabelExpressionType::ResultWithUnits,
        LabelExpressionType::ExceedanceCallout,
        LabelExpressionType::NdCallout,
        LabelExpressionType::WellIdOnly,
    ]
    .iter()
    .filter_map(|t| serde_json::to_value(t).ok())
    .filter_map(|v| v.as_str().map(String::from))
    .collect();
    assert_eq!(names, ["RESULT_WITH_UNITS", "EXCEEDANCE_CALLOUT", "ND_CALLOUT", "WELL_ID_ONLY"]);
}
