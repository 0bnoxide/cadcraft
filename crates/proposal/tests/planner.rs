#[path = "fixture.rs"]
mod sample;
use cadcraft_proposal::{
    Inputs,
    approval::{check_approval, plan_hash},
    manifest::Manifest,
    planner::build,
};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn deterministic_complete_plan_and_stale_approval() {
    let m = Manifest::parse(&serde_json::to_vec(&sample::fixture()).unwrap()).unwrap();
    let inputs = Inputs {
        client_name: "Client".into(),
        site_name: "Site".into(),
        proposal_year: 2026,
        orientation: "Landscape".into(),
        sheet_size: "A3".into(),
        ..Inputs::default()
    };
    let hashes = BTreeMap::from([("model.dxf".into(), "a".repeat(64)), ("sheet.dxf".into(), "b".repeat(64)), ("manifest".into(), "c".repeat(64))]);
    let first = build(&inputs, &m, &hashes).unwrap();
    let second = build(&inputs, &m, &hashes).unwrap();
    let hash = plan_hash(&first).unwrap();
    assert_eq!(hash, plan_hash(&second).unwrap());
    assert!(check_approval(&first, Some(&hash)).is_ok());
    assert_eq!(check_approval(&first, None).unwrap_err().code, "approval_required");
    assert_eq!(check_approval(&first, Some(" ")).unwrap_err().code, "approval_required");
    assert_eq!(check_approval(&first, Some(&hash.to_uppercase())).unwrap_err().code, "stale_approval");
    let mut changed = hashes.clone();
    changed.insert("model.dxf".into(), format!("d{}", "a".repeat(63)));
    assert!(build(&inputs, &m, &BTreeMap::new()).is_err());
    assert_eq!(check_approval(&build(&inputs, &m, &changed).unwrap(), Some(&hash)).unwrap_err().code, "stale_approval");
    let mut complete = BTreeSet::new();
    for a in &first.actions {
        assert!(a.dependencies.iter().all(|id| complete.contains(id)));
        assert!(complete.insert(a.id.clone()));
    }
    assert_eq!(first.actions.iter().filter(|a| matches!(a.operation, cadcraft_proposal::planner::Operation::Xref { .. })).count(), 11);
    assert!(matches!(first.actions.last().unwrap().operation,cadcraft_proposal::planner::Operation::Support{ref role} if role=="CreationReceipt"));
    let mut bad = inputs.clone();
    bad.site_name = "../escape".into();
    assert!(build(&bad, &m, &hashes).is_err());
    bad = inputs.clone();
    bad.proposal_year = 2027;
    assert!(build(&bad, &m, &hashes).is_err());
    bad = inputs.clone();
    bad.client_number = Some("C1".into());
    let update = build(&bad, &m, &hashes).unwrap();
    assert_eq!(update.final_root, first.final_root);
    assert_ne!(plan_hash(&update).unwrap(), hash);
}

#[test]
fn nested_folder_dependencies_preserve_declared_casing() {
    let mut m = sample::fixture();
    m["folders"].as_array_mut().unwrap().push("support/Nested".into());
    m["support_records"][0]["path"] = "support/Nested/project.json".into();
    m["support_records"][1]["path"] = "support/sources.json".into();
    let m = Manifest::parse(&serde_json::to_vec(&m).unwrap()).unwrap();
    let inputs = Inputs {
        client_name: "Client".into(),
        site_name: "Site".into(),
        proposal_year: 2026,
        orientation: "Landscape".into(),
        sheet_size: "A3".into(),
        ..Inputs::default()
    };
    let hashes = BTreeMap::from([("model.dxf".into(), "a".repeat(64)), ("sheet.dxf".into(), "b".repeat(64)), ("manifest".into(), "c".repeat(64))]);
    let plan = build(&inputs, &m, &hashes).unwrap();
    let mut done = BTreeSet::new();
    for a in &plan.actions {
        assert!(a.dependencies.iter().all(|id| done.contains(id)));
        done.insert(a.id.clone());
        assert!(!a.relative_path.starts_with("support/"));
    }
    assert_eq!(plan.actions.iter().filter(|a| a.relative_path == "Support").count(), 1);
    assert!(plan.actions.iter().any(|a| a.relative_path == "Support/Nested/project.json"));
}

#[test]
fn combined_project_folder_must_fit_one_portable_component() {
    let m = Manifest::parse(&serde_json::to_vec(&sample::fixture()).unwrap()).unwrap();
    let hashes = BTreeMap::from([("model.dxf".into(), "a".repeat(64)), ("sheet.dxf".into(), "b".repeat(64)), ("manifest".into(), "c".repeat(64))]);
    let mut inputs = Inputs {
        client_name: "a".repeat(200),
        site_name: "b".repeat(100),
        proposal_year: 2026,
        orientation: "Landscape".into(),
        sheet_size: "A3".into(),
        ..Inputs::default()
    };
    assert_eq!(build(&inputs, &m, &hashes).unwrap_err().first().unwrap().code, "invalid_input");
    inputs.client_name = "a".repeat(125);
    inputs.site_name = "b".repeat(127);
    assert!(build(&inputs, &m, &hashes).is_ok());
    inputs.site_name.push('b');
    assert!(build(&inputs, &m, &hashes).is_err());
}
