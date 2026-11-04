use cadcraft_droneqa::redact::redact;
use serde_json::json;

#[test]
fn nested_and_free_text_credentials_are_scrubbed() {
    let source = json!({"access_token":"synthetic-one", "nested":[{"Pilot":"Private Name"}], "message":"Authorization: Bearer synthetic-two token=synthetic-three https://example.test/x?sig=synthetic-four", "repr":"{'access_token': 'synthetic-five'}"});
    let result = redact(&source, &["pilot".into()]);
    let text = result.to_string();
    assert!(!text.contains("synthetic"));
    assert!(!text.contains("Private Name"));
    assert_eq!(result["access_token"], "[REDACTED]");
    assert_eq!(source["access_token"], "synthetic-one");
}

#[test]
fn public_values_survive() {
    let source = json!({"count":3,"ok":true,"url":"https://example.test/public", "empty":null});
    assert_eq!(redact(&source, &[]), source);
}

#[test]
fn deeply_nested_values_fail_closed() {
    let mut value = json!("private");
    for _ in 0..80 {
        value = json!([value]);
    }
    assert!(!redact(&value, &[]).to_string().contains("private"));
}
