//! Standalone deliverable profiles, sheet selection and portable destination names.
use crate::{
    Issue,
    manifest::{SHEET_ROLES, Sheet},
    paths,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeliverableProfile {
    pub sheet_roles: Vec<String>,
    pub pdf_name: String,
    pub include_drawings: bool,
    pub extra_files: Vec<ExtraFile>,
}
impl Default for DeliverableProfile {
    fn default() -> Self {
        Self { sheet_roles: Vec::new(), pdf_name: "{number}_{title}.pdf".into(), include_drawings: false, extra_files: Vec::new() }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraFile {
    pub path: String,
    pub role: ExtraRole,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraRole {
    FigurePdf,
    FigurePng,
    DataCsv,
    QaReport,
    BoringLog,
    Other,
}
impl ExtraRole {
    pub fn folder(&self) -> &'static str {
        match self {
            Self::FigurePdf | Self::FigurePng => "figures",
            Self::DataCsv | Self::BoringLog => "data",
            Self::QaReport => "qa",
            Self::Other => "",
        }
    }
}
impl ExtraFile {
    pub fn destination(&self, name: &str) -> Result<String, Issue> {
        paths::component(name)?;
        Ok(if self.role.folder().is_empty() { name.into() } else { format!("{}/{name}", self.role.folder()) })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SheetOutput {
    pub sheet: Sheet,
    pub pdf: String,
}
fn invalid(message: &str) -> Issue {
    Issue::new("invalid_profile", message)
}
impl DeliverableProfile {
    pub fn parse(bytes: &[u8]) -> Result<Self, Issue> {
        if bytes.len() > 1 << 20 {
            return Err(invalid("Profile exceeds 1 MiB"));
        }
        let p: Self = serde_json::from_slice(bytes).map_err(|e| Issue::new("invalid_profile", e.to_string()))?;
        let probe = p.pdf_name.replace("{number}", "_").replace("{title}", "_").replace("{role}", "_");
        if p.pdf_name.len() > 1024 || probe.contains(['{', '}']) || !probe.to_ascii_lowercase().ends_with(".pdf") || paths::component(&probe).is_err()
        {
            return Err(invalid("PDF name must be a portable filename ending in .pdf, using only {number}, {title} and {role}"));
        }
        if p.sheet_roles.len() > SHEET_ROLES.len() || p.sheet_roles.iter().any(|r| !SHEET_ROLES.contains(&r.as_str())) {
            return Err(invalid("Unknown or excessive sheet roles"));
        }
        if p.extra_files.len() > 4095
            || p.extra_files.iter().any(|e| e.path.trim().is_empty() || e.path.len() > 4096 || e.path.chars().any(char::is_control))
        {
            return Err(invalid("Expected at most 4095 extra files with nonempty paths"));
        }
        Ok(p)
    }
    pub fn select(&self, sheets: &[Sheet]) -> Result<Vec<SheetOutput>, Issue> {
        if sheets.len() > 1000 {
            return Err(invalid("Sheet index exceeds 1000 sheets"));
        }
        for role in &self.sheet_roles {
            if !sheets.iter().any(|s| &s.role == role) {
                return Err(invalid("Selected sheet role is missing from index"));
            }
        }
        let mut names = BTreeSet::new();
        let mut selected = Vec::new();
        for s in sheets.iter().filter(|s| self.sheet_roles.is_empty() || self.sheet_roles.contains(&s.role)) {
            let mut name = String::new();
            // Substitute once: a title containing a token cannot inject a second expansion.
            let mut rest = self.pdf_name.as_str();
            while let Some(at) = rest.find('{') {
                let (head, tail) = rest.split_at(at);
                name.push_str(head);
                let end = tail.find('}').ok_or_else(|| invalid("Unclosed naming token"))?;
                let key = tail.get(1..end).unwrap_or_default();
                name.push_str(match key {
                    "number" => &s.number,
                    "title" => &s.title,
                    "role" => &s.role,
                    _ => return Err(invalid("Unknown naming token")),
                });
                rest = tail.get(end + 1..).unwrap_or_default();
            }
            name.push_str(rest);
            paths::component(&name)?;
            if !names.insert(paths::case_fold(&name)) {
                return Err(Issue::new("output_collision", "PDF destinations collide"));
            }
            selected.push(SheetOutput { sheet: s.clone(), pdf: format!("sheets/{name}") });
        }
        if selected.is_empty() {
            return Err(invalid("No sheets selected"));
        }
        Ok(selected)
    }
}
