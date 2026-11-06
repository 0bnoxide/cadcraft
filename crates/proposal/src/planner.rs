//! Project plan: ordered, dependency-linked actions built from inputs and a manifest. Owner:
//! Codex (see `docs/agents/slice-7-proposal.md`). Behaviour reference: AutoGIS-Civil3D
//! `ProposalPlanner.cs`.

use crate::{
    Inputs, Issue,
    manifest::{Manifest, Profile, Sheet, Xref, text},
    paths,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub version: u32,
    pub final_root: String,
    pub inputs: Inputs,
    pub manifest: Manifest,
    pub fingerprints: BTreeMap<String, String>,
    pub actions: Vec<Action>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Action {
    pub id: String,
    pub relative_path: String,
    pub dependencies: Vec<String>,
    pub operation: Operation,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Folder { data_shortcut: bool },
    Model { role: String, template: String },
    Sheet { sheet: Sheet, profile: Box<Profile> },
    SheetIndex,
    Xref { reference: Xref, relative_reference_path: String },
    Support { role: String },
}

pub fn input_values(inputs: &Inputs) -> BTreeMap<String, Option<String>> {
    BTreeMap::from([
        ("client_name".into(), Some(inputs.client_name.clone())),
        ("site_name".into(), Some(inputs.site_name.clone())),
        ("proposal_year".into(), Some(inputs.proposal_year.to_string())),
        ("orientation".into(), Some(inputs.orientation.clone())),
        ("sheet_size".into(), Some(inputs.sheet_size.clone())),
        ("client_number".into(), inputs.client_number.clone()),
        ("project_number".into(), inputs.project_number.clone()),
        ("proposal_number".into(), inputs.proposal_number.clone()),
        ("site_address".into(), inputs.site_address.clone()),
        ("project_manager".into(), inputs.project_manager.clone()),
    ])
}

pub fn build(inputs: &Inputs, m: &Manifest, fingerprints: &BTreeMap<String, String>) -> Result<Plan, Vec<Issue>> {
    let invalid = |message| vec![Issue::new("invalid_input", message)];
    if paths::component(&inputs.client_name).is_err()
        || paths::component(&inputs.site_name).is_err()
        || !(1..=9999).contains(&inputs.proposal_year)
        || input_values(inputs).values().flatten().any(|v| !text(v))
    {
        return Err(invalid("Invalid proposal inputs"));
    }
    let name = format!("{} - {}", inputs.client_name, inputs.site_name);
    paths::component(&name).map_err(|_| invalid("Combined project name exceeds portable limits"))?;
    let root = m.base_roots.iter().find(|r| r.year == inputs.proposal_year).ok_or_else(|| invalid("Unsupported proposal year"))?;
    let profile = m
        .profiles
        .iter()
        .find(|p| p.orientation == inputs.orientation && p.size == inputs.sheet_size)
        .ok_or_else(|| invalid("Unsupported sheet profile"))?;
    if !fingerprints.contains_key("manifest")
        || m.templates().iter().any(|name| !fingerprints.contains_key(*name))
        || fingerprints.values().any(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(invalid("Missing or invalid source fingerprints"));
    }
    let final_root = format!("{}/{name}", root.path.trim_end_matches('/'));
    paths::absolute(&final_root).map_err(|e| vec![e])?;
    let mut actions = Vec::new();
    let mut folders = m.folders.clone();
    folders.sort_by_key(|a| (a.matches('/').count(), paths::case_fold(a)));
    let mut folder_ids = BTreeMap::new();
    for (index, path) in folders.into_iter().enumerate() {
        let id = format!("10:folder/{index:05}/{path}");
        let dependencies = parent_dependency(&path, &folder_ids);
        folder_ids.insert(paths::case_fold(&path), id.clone());
        actions.push(Action {
            id,
            relative_path: path.clone(),
            dependencies,
            operation: Operation::Folder { data_shortcut: paths::case_fold(&path) == paths::case_fold(&m.data_shortcut_path) },
        });
    }
    let mut drawing_ids = BTreeMap::new();
    let mut drawing_paths = BTreeMap::new();
    for model in &m.models {
        let id = format!("20:model/{}", model.role);
        drawing_ids.insert(model.role.clone(), id.clone());
        drawing_paths.insert(model.role.clone(), model.path.clone());
        actions.push(Action {
            id,
            relative_path: model.path.clone(),
            dependencies: parent_dependency(&model.path, &folder_ids),
            operation: Operation::Model { role: model.role.clone(), template: m.model_template.clone() },
        });
    }
    for sheet in &m.sheets {
        let id = format!("30:sheet/{}", sheet.role);
        drawing_ids.insert(sheet.role.clone(), id.clone());
        drawing_paths.insert(sheet.role.clone(), sheet.path.clone());
        let mut selected = profile.clone();
        selected.placeholders.retain(|p| p.sheet_role == sheet.role);
        actions.push(Action {
            id,
            relative_path: sheet.path.clone(),
            dependencies: parent_dependency(&sheet.path, &folder_ids),
            operation: Operation::Sheet { sheet: sheet.clone(), profile: Box::new(selected) },
        });
    }
    actions.push(Action {
        id: "40:sheet-index".into(),
        relative_path: m.sheet_index_path.clone(),
        dependencies: drawing_ids.values().cloned().chain(parent_dependency(&m.sheet_index_path, &folder_ids)).collect(),
        operation: Operation::SheetIndex,
    });
    for x in &m.xrefs {
        let host = drawing_paths.get(&x.host_role).ok_or_else(|| invalid("Unknown host role"))?;
        let target = drawing_paths.get(&x.reference_role).ok_or_else(|| invalid("Unknown reference role"))?;
        let dependencies = [&x.host_role, &x.reference_role].iter().filter_map(|r| drawing_ids.get(*r).cloned()).collect();
        actions.push(Action {
            id: format!("60:xref/{}/{}", x.host_role, x.reference_role),
            relative_path: host.clone(),
            dependencies,
            operation: Operation::Xref { reference: x.clone(), relative_reference_path: relative_reference(host, target) },
        });
    }
    for record in m.support_records.iter().filter(|r| r.role != "CreationReceipt") {
        actions.push(Action {
            id: format!("80:support/{}", record.role),
            relative_path: record.path.clone(),
            dependencies: parent_dependency(&record.path, &folder_ids),
            operation: Operation::Support { role: record.role.clone() },
        });
    }
    actions.sort_by(|a, b| a.id.cmp(&b.id));
    let receipt = m.support_records.iter().find(|r| r.role == "CreationReceipt").ok_or_else(|| invalid("Receipt role missing"))?;
    actions.push(Action {
        id: "99:receipt".into(),
        relative_path: receipt.path.clone(),
        dependencies: actions.iter().map(|a| a.id.clone()).collect(),
        operation: Operation::Support { role: receipt.role.clone() },
    });
    for a in &actions {
        paths::absolute(&format!("{final_root}/{}", a.relative_path)).map_err(|e| vec![e])?;
    }
    Ok(Plan { version: 1, final_root, inputs: inputs.clone(), manifest: m.clone(), fingerprints: fingerprints.clone(), actions })
}
fn parent_dependency(path: &str, folders: &BTreeMap<String, String>) -> Vec<String> {
    path.rsplit_once('/').and_then(|(p, _)| folders.get(&paths::case_fold(p))).cloned().into_iter().collect()
}
pub fn relative_reference(host: &str, target: &str) -> String {
    let mut from: Vec<_> = host.split('/').collect();
    from.pop();
    let to: Vec<_> = target.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| paths::case_fold(a) == paths::case_fold(b)).count();
    std::iter::repeat_n("..", from.len().saturating_sub(common)).chain(to.iter().skip(common).copied()).collect::<Vec<_>>().join("/")
}
