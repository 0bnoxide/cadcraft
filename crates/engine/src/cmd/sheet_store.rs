//! Reciprocal entity identities: copying an entity never grants ownership of its copy.
use super::{alignment_store, bad};
use crate::Result;
use cadcraft_alignment::{
    frames::{Frame, MAX_FRAMES},
    view::{Adjustment, Window},
};
use cadcraft_doc::{Drawing, Handle};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub(super) const GROUP_KEY: &str = "CADCRAFT_SHEET_GROUP";
const OWNER_KEY: &str = "CADCRAFT_SHEET_OWNER";
pub(super) const MAX_MEMBERS: usize = 200_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Group {
    pub version: u32,
    pub own: String,
    pub alignment: Option<String>,
    pub paper_size: [f64; 2],
    pub scale: f64,
    pub frames: Vec<Member>,
    pub matchline: Option<Matchline>,
    pub profiles: Option<ProfileSettings>,
    #[serde(default)]
    pub sheets: Option<SheetRun>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SheetConfig {
    pub template: String,
    pub template_layout: String,
    pub arrangement: String,
    pub layout_name: String,
    pub number_start: u32,
    pub number_increment: u32,
    pub number_width: usize,
    pub north_arrow: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SheetRun {
    pub config: SheetConfig,
    pub source_hash: String,
    pub layouts: Vec<Sheet>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Sheet {
    pub name: String,
    pub marker: String,
    pub frames: Vec<u32>,
    pub members: Vec<String>,
    pub hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Matchline {
    pub block: String,
    pub attribute: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileSettings {
    pub at: [f64; 2],
    pub exaggeration: f64,
    pub adjustment: Adjustment,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Member {
    pub index: u32,
    pub handle: String,
    pub geometry: Frame,
    pub matchlines: Vec<String>,
    pub profile: Option<ProfileFigure>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileFigure {
    pub handles: Vec<String>,
    pub at: [f64; 2],
    pub window: Window,
    pub exaggeration: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    group: String,
    own: String,
    frame: u32,
    role: String,
}
pub(super) fn invalid(reason: impl std::fmt::Display) -> crate::EngineError {
    bad("sheets", format!("INVALID_FRAME_GROUP: {reason}"))
}
pub(super) fn handle(value: &str) -> Result<Handle> {
    Handle::parse_hex(value).filter(|h| h.0 != 0).ok_or_else(|| invalid("invalid member handle"))
}
pub(super) fn load(d: &Drawing, h: Handle) -> Result<Group> {
    if d.space_of(h) != Some(cadcraft_doc::Space::Model) {
        return Err(invalid("group anchor missing from model space"));
    }
    let raw = alignment_store::read_record(d, h, GROUP_KEY).map_err(invalid)?;
    let g: Group = serde_json::from_str(&raw).map_err(invalid)?;
    validate(&g)?;
    if g.own != h.hex() {
        return Err(invalid("copied or stale group anchor"));
    }
    Ok(g)
}
fn validate(g: &Group) -> Result<()> {
    if g.version != 1
        || g.frames.len() > MAX_FRAMES
        || g.paper_size.iter().any(|v| !v.is_finite() || *v <= 0.)
        || !g.scale.is_finite()
        || g.scale <= 0.
    {
        return Err(invalid("version, frame count, paper size or scale invalid"));
    }
    handle(&g.own)?;
    if let Some(a) = &g.alignment {
        handle(a)?;
    }
    let mut indices = BTreeSet::new();
    let mut members = BTreeSet::new();
    members.insert(g.own.clone());
    for frame in &g.frames {
        frame.geometry.validate().map_err(invalid)?;
        if frame
            .geometry
            .size
            .iter()
            .zip(g.paper_size)
            .any(|(size, paper)| !(*size - paper * g.scale).is_finite() || (*size - paper * g.scale).abs() > cadcraft_alignment::POSITION_TOLERANCE)
        {
            return Err(invalid("stored frame size differs from paper size times scale"));
        }
        if !indices.insert(frame.index) || frame.matchlines.len() > 4 {
            return Err(invalid("duplicate frame index or excessive matchlines"));
        }
        let profile = frame.profile.iter().flat_map(|p| &p.handles);
        for value in std::iter::once(&frame.handle).chain(&frame.matchlines).chain(profile) {
            handle(value)?;
            if !members.insert(value.clone()) || members.len() > MAX_MEMBERS {
                return Err(invalid("duplicate or excessive owned members"));
            }
        }
        if let Some(p) = &frame.profile
            && (p.handles.is_empty()
                || p.at.iter().chain(&p.window.range).chain(&p.window.elevation).any(|v| !v.is_finite())
                || !p.exaggeration.is_finite()
                || p.exaggeration <= 0.
                || p.window.range[1] <= p.window.range[0]
                || p.window.elevation[1] <= p.window.elevation[0])
        {
            return Err(invalid("invalid owned profile view"));
        }
    }
    Ok(())
}
pub(super) fn save(d: &mut Drawing, g: &Group) -> Result<()> {
    validate(g)?;
    alignment_store::save_record(d, handle(&g.own)?, GROUP_KEY, g).map_err(invalid)
}
pub(super) fn own(d: &mut Drawing, h: Handle, g: &str, frame: u32, role: &str) -> Result<()> {
    // shortcut: shared XRECORD lookups scan linearly; batch-index them when large groups are qualified.
    alignment_store::save_record(d, h, OWNER_KEY, &Owner { version: 1, group: g.into(), own: h.hex(), frame, role: role.into() }).map_err(invalid)
}
pub(super) fn check(d: &Drawing, h: Handle, g: &str, frame: u32, role: &str) -> Result<()> {
    let raw = alignment_store::read_record(d, h, OWNER_KEY).map_err(invalid)?;
    let o: Owner = serde_json::from_str(&raw).map_err(invalid)?;
    if o.version != 1 || o.group != g || o.own != h.hex() || o.frame != frame || o.role != role {
        return Err(invalid("ownership mismatch"));
    }
    Ok(())
}
pub(super) fn erase(d: &mut Drawing, values: &[String], g: &str, frame: u32, role: &str) -> Result<()> {
    for value in values {
        let h = handle(value)?;
        if d.entity(h).is_some() {
            check(d, h, g, frame, role)?;
            let record = alignment_store::record_handle_for(d, h, OWNER_KEY)?.ok_or_else(|| invalid("missing owner record"))?;
            let dict = d.foreign.entities.get(&h).and_then(|e| e.xdict).ok_or_else(|| invalid("missing owner dictionary"))?;
            let foreign = std::sync::Arc::make_mut(&mut d.foreign);
            let object = foreign.objects.iter_mut().find(|o| o.handle == dict).ok_or_else(|| invalid("missing owner dictionary"))?;
            let key = object.tags.iter().position(|t| t.code == 3 && t.value == OWNER_KEY).ok_or_else(|| invalid("missing owner key"))?;
            object.tags.drain(key..key + 2);
            let keep = object.tags.iter().any(|t| t.code == 3);
            // Delete our own records; keep any other application's dictionary entries.
            foreign.objects.retain(|o| o.handle != record && (keep || o.handle != dict));
            foreign.entities.remove(&h);
            d.remove_entity(h);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;
    #[test]
    fn corrupt_group_cannot_adopt_a_different_viewport_size() {
        let mut s = Session::new();
        let a = s.execute("alignmentcreate", &json!({"name":"Road","elements":[{"start":[0,0],"direction":0,"length":1000,"type":"line"}]})).unwrap();
        let result = s.execute("viewframecreate", &json!({"alignment":a["handle"],"paper_size":[300,100],"scale":1,"spacing":250})).unwrap();
        let h = handle(result["group"].as_str().unwrap()).unwrap();
        let mut g = load(s.doc().unwrap(), h).unwrap();
        g.frames[0].geometry.size[0] = 400.;
        alignment_store::save_record(s.doc_mut().unwrap(), h, GROUP_KEY, &g).unwrap();
        assert!(load(s.doc().unwrap(), h).is_err());
    }
}
