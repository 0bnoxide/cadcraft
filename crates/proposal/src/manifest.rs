//! Standards manifest: our own JSON format, parsed fail-closed. Owner: Codex (see
//! `docs/agents/slice-7-proposal.md`). Behaviour reference: AutoGIS-Civil3D `StandardsManifest.cs`.

use crate::{Issue, paths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL_ROLES: [&str; 3] = ["BaseModel", "ExistingConditionsModel", "ProposedDesignModel"];
pub const SHEET_ROLES: [&str; 3] = ["SiteOverviewSheet", "ExistingConditionsSheet", "ProposedSitePlanSheet"];
pub const SUPPORT_ROLES: [&str; 6] = ["ProjectConfiguration", "SourceRegister", "AssumptionsLog", "DecisionLog", "CreationPlan", "CreationReceipt"];
pub const INPUT_FIELDS: [&str; 10] = [
    "client_name",
    "site_name",
    "proposal_year",
    "orientation",
    "sheet_size",
    "client_number",
    "project_number",
    "proposal_number",
    "site_address",
    "project_manager",
];
pub const SOURCE_ROLES: [&str; 5] = ["survey", "record_drawing", "gis_export", "imagery", "other"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BaseRoot {
    pub year: i32,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub role: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub role: String,
    pub path: String,
    pub number: String,
    pub title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PropertyMapping {
    pub input: String,
    pub property: String,
    pub title_block_attribute: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Placeholder {
    pub sheet_role: String,
    pub label: String,
    pub kind: String,
    pub rectangle: Rectangle,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plot {
    pub device: String,
    pub media: String,
    pub style_sheet: String,
    pub units: String,
    pub scale: f64,
    pub rotation: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub orientation: String,
    pub size: String,
    pub template: String,
    pub layout: String,
    pub title_block: String,
    pub page_setup: String,
    pub plot: Plot,
    pub printable_area: Rectangle,
    pub placeholders: Vec<Placeholder>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Xref {
    pub host_role: String,
    pub reference_role: String,
    pub mode: String,
    pub insertion: Point3,
    pub scale: f64,
    pub rotation: f64,
}

/// Version 1 snake_case JSON. Template paths are absolute or relative to the manifest's directory.
/// Construction must use `parse`; fields are read-only outside this crate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub(crate) version: u32,
    pub(crate) base_roots: Vec<BaseRoot>,
    pub(crate) folders: Vec<String>,
    pub(crate) model_template: String,
    pub(crate) models: Vec<Model>,
    pub(crate) sheets: Vec<Sheet>,
    pub(crate) sheet_index_path: String,
    pub(crate) support_records: Vec<Model>,
    pub(crate) data_shortcut_path: String,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) property_mappings: Vec<PropertyMapping>,
    pub(crate) xrefs: Vec<Xref>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) drawing_settings: Option<crate::settings::DrawingSettings>,
}

fn invalid(message: &str) -> Issue {
    Issue::new("invalid_manifest", message)
}
pub fn text(value: &str) -> bool {
    !value.is_empty() && value.trim() == value && !value.chars().any(char::is_control) && value.len() <= 4096
}
fn unique<'a>(mut values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = BTreeSet::new();
    values.all(|v| seen.insert(paths::case_fold(v)))
}
fn roles<'a>(values: impl Iterator<Item = &'a str>, expected: &[&str]) -> bool {
    let actual: Vec<_> = values.collect();
    actual.len() == expected.len() && expected.iter().all(|e| actual.contains(e))
}
fn drawing(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".dxf") || path.to_ascii_lowercase().ends_with(".dwg")
}
fn template(path: &mut String) -> Result<(), Issue> {
    *path = paths::absolute(path).or_else(|_| paths::relative(path))?;
    if !drawing(path) {
        return Err(invalid("Templates must be DXF or DWG"));
    }
    Ok(())
}
impl Rectangle {
    fn valid(&self) -> bool {
        [self.x, self.y, self.width, self.height, self.x + self.width, self.y + self.height].iter().all(|n| n.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
    fn contains(&self, other: &Self) -> bool {
        other.valid()
            && other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Vec<Issue>> {
        if bytes.len() > 1 << 20 {
            return Err(vec![invalid("Manifest exceeds 1 MiB")]);
        }
        let mut m: Self = serde_json::from_slice(bytes).map_err(|e| {
            vec![Issue::new(if e.to_string().contains("duplicate field") { "duplicate_property" } else { "invalid_json" }, e.to_string())]
        })?;
        if !matches!(m.version, 1 | 2) {
            return Err(vec![Issue::new("unsupported_version", "Only manifest versions 1 and 2 are supported")]);
        }
        m.validate().map_err(|e| vec![e])?;
        Ok(m)
    }
    pub fn templates(&self) -> Vec<&str> {
        let mut values: BTreeSet<_> = self.profiles.iter().map(|p| p.template.as_str()).collect();
        values.insert(&self.model_template);
        values.into_iter().collect()
    }
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn source_location(&self) -> &str {
        self.source_location.as_deref().unwrap_or("Support/Sources")
    }
    pub fn drawing_settings(&self) -> Option<&crate::settings::DrawingSettings> {
        self.drawing_settings.as_ref()
    }
    pub fn models(&self) -> &[Model] {
        &self.models
    }
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }
    pub fn mappings(&self) -> &[PropertyMapping] {
        &self.property_mappings
    }
    pub fn support_records(&self) -> &[Model] {
        &self.support_records
    }
    pub fn xrefs(&self) -> &[Xref] {
        &self.xrefs
    }
    pub fn sheet_index_path(&self) -> &str {
        &self.sheet_index_path
    }
    pub fn profile(&self, orientation: &str, size: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.orientation == orientation && p.size == size)
    }
    fn validate(&mut self) -> Result<(), Issue> {
        if self.version == 1 {
            if self.source_roles.is_some() || self.source_location.is_some() || self.drawing_settings.is_some() {
                return Err(invalid("Follow-on fields require manifest version 2"));
            }
        } else {
            let source_roles = self.source_roles.as_mut().ok_or_else(|| invalid("Version 2 requires source_roles"))?;
            if !roles(source_roles.iter().map(String::as_str), &SOURCE_ROLES) {
                return Err(invalid("Expected the five source roles"));
            }
            source_roles.sort();
            let location = paths::relative(self.source_location())?;
            self.source_location = Some(location);
            self.drawing_settings.as_ref().ok_or_else(|| invalid("Version 2 requires drawing_settings"))?.validate()?;
        }
        if [
            self.base_roots.len(),
            self.folders.len(),
            self.models.len(),
            self.sheets.len(),
            self.support_records.len(),
            self.profiles.len(),
            self.property_mappings.len(),
            self.xrefs.len(),
        ]
        .iter()
        .any(|n| *n > 1024)
        {
            return Err(invalid("Collection exceeds 1024 entries"));
        }
        if self.base_roots.is_empty() || self.profiles.is_empty() {
            return Err(invalid("Roots and profiles are required"));
        }
        for root in &mut self.base_roots {
            root.path = paths::absolute(&root.path)?;
            if !(1..=9999).contains(&root.year) {
                return Err(invalid("Invalid year"));
            }
        }
        let years: BTreeSet<_> = self.base_roots.iter().map(|r| r.year).collect();
        if years.len() != self.base_roots.len() {
            return Err(invalid("Duplicate year"));
        }
        template(&mut self.model_template)?;
        for f in &mut self.folders {
            *f = paths::relative(f)?;
        }
        self.data_shortcut_path = paths::relative(&self.data_shortcut_path)?;
        self.sheet_index_path = paths::relative(&self.sheet_index_path)?;
        if !self.sheet_index_path.ends_with(".json") {
            return Err(invalid("Sheet index must be JSON"));
        }
        for model in &mut self.models {
            model.path = paths::relative(&model.path)?;
            if !drawing(&model.path) {
                return Err(invalid("Model output must be DXF or DWG"));
            }
        }
        for sheet in &mut self.sheets {
            sheet.path = paths::relative(&sheet.path)?;
            if !drawing(&sheet.path) || !text(&sheet.number) || !text(&sheet.title) {
                return Err(invalid("Invalid sheet"));
            }
        }
        for support in &mut self.support_records {
            support.path = paths::relative(&support.path)?;
            if !support.path.ends_with(".json") {
                return Err(invalid("Support output must be JSON"));
            }
        }
        if !roles(self.models.iter().map(|m| m.role.as_str()), &MODEL_ROLES)
            || !roles(self.sheets.iter().map(|m| m.role.as_str()), &SHEET_ROLES)
            || !roles(self.support_records.iter().map(|m| m.role.as_str()), &SUPPORT_ROLES)
            || !unique(self.sheets.iter().map(|s| s.number.as_str()))
        {
            return Err(invalid("Missing or duplicate roles/numbers"));
        }
        if !self.property_mappings.iter().any(|p| p.input == "client_number")
            || !self.property_mappings.iter().all(|p| INPUT_FIELDS.contains(&p.input.as_str()) && text(&p.property) && text(&p.title_block_attribute))
            || !unique(self.property_mappings.iter().map(|p| p.input.as_str()))
            || !unique(self.property_mappings.iter().map(|p| p.property.as_str()))
            || !unique(self.property_mappings.iter().map(|p| p.title_block_attribute.as_str()))
        {
            return Err(invalid("Invalid property mappings"));
        }
        let mut profiles = BTreeSet::new();
        for p in &mut self.profiles {
            template(&mut p.template)?;
            if !profiles.insert((paths::case_fold(&p.orientation), paths::case_fold(&p.size)))
                || !matches!(p.orientation.as_str(), "Landscape" | "Portrait")
                || [&p.size, &p.layout, &p.title_block, &p.page_setup, &p.plot.device, &p.plot.media, &p.plot.style_sheet].iter().any(|s| !text(s))
                || !matches!(p.plot.units.as_str(), "Millimeters" | "Inches")
                || !p.plot.scale.is_finite()
                || p.plot.scale <= 0.0
                || !matches!(p.plot.rotation, 0 | 90 | 180 | 270)
                || !p.printable_area.valid()
                || p.placeholders.len() != 4
            {
                return Err(invalid("Invalid sheet profile"));
            }
            for role in SHEET_ROLES {
                let placeholders: Vec<_> = p.placeholders.iter().filter(|v| v.sheet_role == role).collect();
                if placeholders.len() != if role == "SiteOverviewSheet" { 2 } else { 1 } {
                    return Err(invalid("Missing starter placeholders"));
                }
                if role == "SiteOverviewSheet"
                    && !["SITE LOCATION", "SITE OVERVIEW"].iter().all(|label| placeholders.iter().any(|v| v.label == *label))
                {
                    return Err(invalid("Missing overview labels"));
                }
            }
            for v in &p.placeholders {
                if !text(&v.label) || !matches!(v.kind.as_str(), "Boundary" | "DisabledViewport") || !p.printable_area.contains(&v.rectangle) {
                    return Err(invalid("Invalid placeholder"));
                }
            }
            p.placeholders.sort_by(|a, b| (&a.sheet_role, &a.label).cmp(&(&b.sheet_role, &b.label)));
        }
        let mut entries: Vec<_> = self.folders.iter().map(|p| (p.clone(), false)).collect();
        entries.extend(
            self.models
                .iter()
                .map(|m| m.path.clone())
                .chain(self.sheets.iter().map(|m| m.path.clone()))
                .chain(self.support_records.iter().map(|m| m.path.clone()))
                .chain(std::iter::once(self.sheet_index_path.clone()))
                .map(|p| (p, true)),
        );
        if let Some(issue) = paths::collisions(&entries).into_iter().next() {
            return Err(issue);
        }
        if self.version == 2 {
            let mut source_entries = entries.clone();
            // A source folder may already be declared; it must never be inside an output file.
            source_entries.retain(|(p, file)| *file || paths::case_fold(p) != paths::case_fold(self.source_location()));
            source_entries.push((self.source_location().to_string(), false));
            if let Some(issue) = paths::collisions(&source_entries).into_iter().next() {
                return Err(issue);
            }
        }
        let folders: BTreeSet<_> = self.folders.iter().map(|p| paths::case_fold(p)).collect();
        if !folders.contains(&paths::case_fold(&self.data_shortcut_path))
            || entries.iter().any(|(p, _)| p.rsplit_once('/').is_some_and(|(parent, _)| !folders.contains(&paths::case_fold(parent))))
        {
            return Err(invalid("Every output parent and shortcut folder must be declared"));
        }
        // Use declared folder casing on case-sensitive hosts too.
        self.folders.sort_by_key(|p| p.matches('/').count());
        let mut declared = BTreeMap::new();
        for folder in &mut self.folders {
            *folder = canonical_parent(folder, &declared);
            declared.insert(paths::case_fold(folder), folder.clone());
        }
        for path in self
            .models
            .iter_mut()
            .map(|m| &mut m.path)
            .chain(self.sheets.iter_mut().map(|m| &mut m.path))
            .chain(self.support_records.iter_mut().map(|m| &mut m.path))
            .chain(std::iter::once(&mut self.sheet_index_path))
        {
            *path = canonical_parent(path, &declared);
        }
        if let Some(path) = declared.get(&paths::case_fold(&self.data_shortcut_path)) {
            self.data_shortcut_path = path.clone();
        }
        let expected: BTreeSet<_> = SHEET_ROLES
            .iter()
            .flat_map(|s| MODEL_ROLES.iter().map(move |m| ((*s).to_string(), (*m).to_string())))
            .chain(MODEL_ROLES.iter().take(2).map(|m| ("ProposedDesignModel".into(), (*m).into())))
            .collect();
        let actual: BTreeSet<_> = self.xrefs.iter().map(|x| (x.host_role.clone(), x.reference_role.clone())).collect();
        if self.xrefs.len() != 11
            || actual != expected
            || self.xrefs.iter().any(|x| {
                x.mode != "Overlay"
                    || x.scale <= 0.0
                    || ![x.scale, x.rotation, x.insertion.x, x.insertion.y, x.insertion.z].iter().all(|v| v.is_finite())
            })
        {
            return Err(Issue::new("invalid_references", "Expected the complete direct overlay graph"));
        }
        self.base_roots.sort_by_key(|r| r.year);
        self.folders.sort();
        self.models.sort_by(|a, b| a.role.cmp(&b.role));
        self.sheets.sort_by(|a, b| a.role.cmp(&b.role));
        self.support_records.sort_by(|a, b| a.role.cmp(&b.role));
        self.profiles.sort_by(|a, b| (&a.orientation, &a.size).cmp(&(&b.orientation, &b.size)));
        self.property_mappings.sort_by(|a, b| a.input.cmp(&b.input));
        self.xrefs.sort_by(|a, b| (&a.host_role, &a.reference_role).cmp(&(&b.host_role, &b.reference_role)));
        Ok(())
    }
}

fn canonical_parent(path: &str, folders: &BTreeMap<String, String>) -> String {
    match path.rsplit_once('/') {
        Some((parent, name)) => format!("{}/{name}", folders.get(&paths::case_fold(parent)).map(String::as_str).unwrap_or(parent)),
        None => path.to_string(),
    }
}
