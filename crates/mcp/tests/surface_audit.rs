//! Audit malformed surfaces through the real JSON-RPC server; reporting must not require a valid TIN.

use std::io::Cursor;

use cadcraft_engine::doc::{EntityKind, PolyfaceMesh};
use cadcraft_engine::geom::Vec3;
use cadcraft_mcp::{Headless, Server};
use serde_json::{Value, json};

fn vertices() -> Vec<Vec3> {
    vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 0.0)]
}

fn audit(mesh: PolyfaceMesh, expected: &[&str], triangles: usize) -> Value {
    let mut backend = Headless::default();
    let handle = backend.session.add_entity(EntityKind::PolyfaceMesh(mesh)).unwrap().hex();
    let params = json!({"command": "surfaceaudit", "params": {"mode": "tin", "surface": handle, "boundary": [[-1,-1],[20,-1],[20,20],[-1,20]]}});
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "inspect_drawing", "arguments": {"entities": true}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "execute", "arguments": params}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "execute", "arguments": params}}),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "inspect_drawing", "arguments": {"entities": true}}}),
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "execute", "arguments": {"command": "tinexport", "params": {"surface": handle, "epsg": 26913}}}}),
        json!({"jsonrpc": "2.0", "id": 7, "method": "ping"}),
    ];
    let input = requests.iter().map(|r| format!("{r}\n")).collect::<String>();
    let mut output = Vec::new();
    Server::new(Box::new(backend)).serve(Cursor::new(input), &mut output).unwrap();
    let replies: Vec<Value> = String::from_utf8(output).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(replies.len(), 7);
    let result = |id| replies.iter().find(|r| r["id"] == id).unwrap()["result"].clone();
    assert_eq!(result(1)["serverInfo"]["name"], "cadcraft");
    assert_eq!(result(2), result(5), "audit must preserve geometry, selection, dirty flag and undo history");
    assert_eq!(result(3), result(4), "audit must be deterministic");
    assert_eq!(result(7), json!({}), "malformed geometry must not stop the server");
    let response = result(3);
    assert_ne!(response["isError"], true, "expected an audit receipt: {response}");
    let value: Value = serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
    let codes: Vec<&str> = value["issues"].as_array().unwrap().iter().map(|i| i["code"].as_str().unwrap()).collect();
    assert_eq!(codes, expected, "{value}");
    assert_eq!(value["receipt"]["check"], "tin");
    assert_eq!(value["receipt"]["inputs"]["surface"], handle);
    assert_eq!(value["receipt"]["counts"]["triangles"], triangles);
    assert_eq!(value["receipt"]["counts"]["errors"], expected.len());
    assert_eq!(value["receipt"]["counts"]["warnings"], 0);
    if expected.is_empty() {
        assert_ne!(result(6)["isError"], true, "valid TIN export still works");
    } else {
        assert_eq!(result(6)["isError"], true, "audit must not relax strict export validation");
    }
    value
}

#[test]
fn empty_tin_returns_audit_receipt() {
    for vertices in [vertices(), vec![]] {
        audit(PolyfaceMesh { vertices, faces: vec![] }, &["TIN-EMPTY"], 0);
    }
}

#[test]
fn repeated_index_returns_bad_triangle_receipt() {
    audit(PolyfaceMesh { vertices: vertices(), faces: vec![[1, 1, 2, 0]] }, &["TIN-BAD-TRIANGLE"], 1);
}

#[test]
fn missing_index_returns_bad_triangle_receipt() {
    audit(PolyfaceMesh { vertices: vertices(), faces: vec![[1, 2, 9, 0]] }, &["TIN-BAD-TRIANGLE"], 1);
}

#[test]
fn nonfinite_vertex_returns_bad_triangle_receipt() {
    let mut points = vertices();
    points[2].z = f64::NAN;
    let out = audit(PolyfaceMesh { vertices: points, faces: vec![[1, 2, 3, 0]] }, &["TIN-BAD-TRIANGLE"], 1);
    assert_eq!(out["issues"][0]["at"], Value::Null);
}

#[test]
fn collinear_face_returns_bad_triangle_receipt() {
    let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)];
    audit(PolyfaceMesh { vertices: points, faces: vec![[1, 2, 3, 0]] }, &["TIN-BAD-TRIANGLE"], 1);
}

#[test]
fn short_faces_and_minimum_signed_index_are_reported() {
    for face in [[0, 1, 2, 3], [1, 0, 2, 3], [1, 2, 0, 4], [i32::MIN, 2, 3, 0]] {
        audit(PolyfaceMesh { vertices: vertices(), faces: vec![face] }, &["TIN-BAD-TRIANGLE"], 1);
    }
}

#[test]
fn malformed_faces_do_not_hide_later_findings() {
    let out = audit(
        PolyfaceMesh { vertices: vertices(), faces: vec![[1, 2, 9, 0], [1, 2, 3, 4], [1, 1, 2, 0]] },
        &["TIN-BAD-TRIANGLE", "TIN-BAD-TRIANGLE"],
        4,
    );
    assert_eq!(out["issues"][0]["refs"], json!(["0"]));
    assert_eq!(out["issues"][1]["refs"], json!(["3"]));
}

#[test]
fn clockwise_quad_and_hidden_edges_remain_valid() {
    audit(PolyfaceMesh { vertices: vertices(), faces: vec![[1, -4, 3, 2]] }, &[], 2);
}

#[test]
fn nearly_collinear_face_returns_bad_triangle_receipt() {
    let points = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 1e-13, 0.0)];
    audit(PolyfaceMesh { vertices: points, faces: vec![[1, 2, 3, 0]] }, &["TIN-BAD-TRIANGLE"], 1);
}

#[test]
fn missing_fourth_quad_corner_reports_only_the_bad_triangle() {
    let out = audit(PolyfaceMesh { vertices: vertices(), faces: vec![[1, 2, 3, 9]] }, &["TIN-BAD-TRIANGLE"], 2);
    assert_eq!(out["issues"][0]["refs"], json!(["1"]));
}
