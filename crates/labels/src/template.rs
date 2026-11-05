//! Batch label templates: our own JSON format choosing sources (points by layer, polyline
//! segments, TIN spot elevations), content and label kind. Owner: Codex (see
//! `docs/agents/slice-4-labels.md`).

use crate::{Rect, format::Part};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "CADCRAFT_LABELS";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub version: u8,
    pub template: String,
    pub source: String,
    pub role: String,
}

pub fn encode_owner(owner: &Owner) -> Result<Vec<String>, String> {
    if owner.version != 1 || owner.template.is_empty() || owner.template.len() > 64 || owner.source.len() > 128 || owner.role.len() > 32 {
        return Err("invalid ownership tag".into());
    }
    let text = serde_json::to_string(owner).map_err(|e| e.to_string())?;
    let mut rest = text.as_str();
    let mut chunks = Vec::new();
    while !rest.is_empty() {
        let mut cut = rest.len().min(240);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        chunks.push(rest.get(..cut).ok_or("invalid tag boundary")?.to_owned());
        rest = rest.get(cut..).ok_or("invalid tag boundary")?;
    }
    Ok(chunks)
}

pub fn decode_owner(chunks: &[String]) -> Option<Owner> {
    if chunks.len() > 8 || chunks.iter().any(|s| s.len() > 240) {
        return None;
    }
    let owner: Owner = serde_json::from_str(&chunks.concat()).ok()?;
    encode_owner(&owner).ok()?;
    Some(owner)
}

/// CADCraft's own template format, independent of any commercial schema.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub id: String,
    pub source: Source,
    pub content: Vec<Part>,
    #[serde(default)]
    pub leader: bool,
    pub height: f64,
    #[serde(default)]
    pub boundary: Option<Rect>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Points {
        #[serde(default)]
        layers: Vec<String>,
        #[serde(default)]
        codes: Vec<String>,
    },
    Segments {
        #[serde(default)]
        layers: Vec<String>,
    },
    SurfaceVertices {
        handle: String,
    },
    SurfaceGrid {
        handle: String,
        spacing: f64,
    },
}

impl Template {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 64
            || self.content.len() > 256
            || !self.height.is_finite()
            || self.height <= 0.0
            || self.boundary.is_some_and(|b| !b.is_valid())
        {
            return Err("invalid template id, content, height or boundary".into());
        }
        match &self.source {
            Source::Points { layers, codes } if layers.len() > 256 || codes.len() > 256 => Err("too many source filters".into()),
            Source::Segments { layers } if layers.len() > 256 => Err("too many layer filters".into()),
            Source::SurfaceGrid { spacing, .. } if !spacing.is_finite() || *spacing <= 0.0 => Err("invalid grid spacing".into()),
            _ => Ok(()),
        }
    }
}
