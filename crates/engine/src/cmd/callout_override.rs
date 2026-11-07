//! Uninserted blocks carry placement metadata without becoming survey points.
use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use cadcraft_doc::{Block, Drawing};
use cadcraft_labels::{
    Pt,
    placement::{Request, offsets},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) const PREFIX: &str = "CADCRAFT_OVERRIDE_";
const LIMIT: usize = 1920;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Override {
    template: String,
    source: String,
    #[serde(default)]
    anchor: Option<[f64; 2]>,
    #[serde(default)]
    offset: Option<[f64; 2]>,
    #[serde(default)]
    preferred_quadrant: Option<String>,
    #[serde(default)]
    locked: bool,
}

impl Override {
    fn valid(&self) -> bool {
        !self.template.is_empty()
            && self.template.len() <= 64
            && !self.source.is_empty()
            && self.source.len() <= 128
            && self.anchor.iter().chain(self.offset.iter()).flatten().all(|v| v.is_finite())
            && self.preferred_quadrant.as_deref().is_none_or(|q| matches!(q, "NE" | "NW" | "SE" | "SW"))
            && (!self.locked || self.anchor.is_some())
    }
    pub(super) fn apply(&self, r: &mut Request, gap: f64) {
        if let Some([x, y]) = self.anchor {
            r.anchor = Pt { x, y };
            let [dx, dy] = self.offset.unwrap_or([0.0; 2]);
            r.origin = Some(Pt { x: x + dx, y: y + dy });
        } else if let Some(q) = self.preferred_quadrant.as_deref() {
            let i = match q {
                "NW" => 1,
                "SE" => 2,
                "SW" => 3,
                _ => 0,
            };
            if let Some(p) = offsets(r.width, r.height, gap).get(i) {
                r.origin = Some(Pt { x: r.anchor.x + p.x, y: r.anchor.y + p.y });
            }
        }
        r.locked = self.locked;
    }
}

fn decode(b: &Block) -> Option<Override> {
    if !b.name.starts_with(PREFIX) || b.description.len() > LIMIT || !b.entities.is_empty() {
        return None;
    }
    let value: Override = serde_json::from_str(&b.description).ok()?;
    value.valid().then_some(value)
}

pub(super) fn find(d: &Drawing, template: &str, source: &str) -> Option<Override> {
    d.blocks.values().filter_map(|b| decode(b)).find(|o| o.template == template && o.source == source)
}

pub(super) fn index(d: &Drawing, template: &str) -> Result<std::collections::BTreeMap<String, Override>> {
    let mut out = std::collections::BTreeMap::new();
    for value in d.blocks.values().filter_map(|b| decode(b)).filter(|o| o.template == template) {
        if out.len() >= cadcraft_labels::placement::MAX_LABELS && !out.contains_key(&value.source) {
            return Err(bad("labelbatch", "too many placement overrides"));
        }
        out.entry(value.source.clone()).or_insert(value);
    }
    Ok(out)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("calloutoverride", "Callout Override", run)
            .params("{action:set|clear|list,template?,source?,anchor?:[x,y],offset?:[x,y],preferred_quadrant?:NE|NW|SE|SW,locked?:false}"),
    ]
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let id = "calloutoverride";
    match str_param(p, "action") {
        Some("list") => {
            let d = s.doc()?;
            let rows: Vec<_> = d
                .blocks
                .values()
                .filter(|b| b.name.starts_with(PREFIX))
                .map(|b| match decode(b) {
                    None => json!({"name":b.name,"dormant":true,"malformed":true}),
                    Some(o) => {
                        let handle = o.source.split(':').next().and_then(cadcraft_doc::Handle::parse_hex);
                        let dormant = handle.is_none_or(|h| d.entity(h).is_none());
                        json!({"override":o,"dormant":dormant})
                    }
                })
                .collect();
            Ok(json!({"overrides":rows}))
        }
        Some(action @ ("set" | "clear")) => {
            let template = str_param(p, "template").ok_or_else(|| bad(id, "template required"))?;
            let source = str_param(p, "source").ok_or_else(|| bad(id, "source required"))?;
            let existing =
                s.doc()?.blocks.iter().find_map(|(name, b)| decode(b).filter(|o| o.template == template && o.source == source).map(|_| name.clone()));
            if action == "clear" {
                let cleared = existing.is_some();
                if let Some(name) = existing {
                    s.doc_mut()?.blocks.remove(&name);
                }
                return Ok(json!({"cleared":cleared}));
            }
            let mut fields = p.as_object().cloned().ok_or_else(|| bad(id, "object required"))?;
            fields.remove("action");
            let value: Override = serde_json::from_value(Value::Object(fields)).map_err(|_| bad(id, "invalid override"))?;
            let description = serde_json::to_string(&value).map_err(|_| bad(id, "invalid override"))?;
            if !value.valid() || description.len() > LIMIT {
                return Err(bad(id, "invalid or oversized override"));
            }
            let name = if let Some(name) = existing {
                name
            } else {
                (0..=2048)
                    .map(|i| format!("{PREFIX}{i}"))
                    .find(|name| !s.doc().is_ok_and(|d| d.blocks.contains_key(name)))
                    .ok_or_else(|| bad(id, "override limit"))?
            };
            let mut block = Block::new(&name);
            block.description = description;
            s.doc_mut()?.blocks.insert(name, Arc::new(block));
            Ok(json!({"set":true}))
        }
        _ => Err(bad(id, "action must be set, clear or list")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_clear_dormant_and_long_description_roundtrip() {
        let mut s = Session::new();
        let template = "\\".repeat(64);
        let source = "\"".repeat(128);
        let mut p = json!({"action":"set","template":template,"source":source,"preferred_quadrant":"SW"});
        s.execute("calloutoverride", &p).unwrap();
        p["preferred_quadrant"] = json!("NW");
        s.execute("calloutoverride", &p).unwrap();
        let rows = s.execute("calloutoverride", &json!({"action":"list"})).unwrap();
        assert_eq!(rows["overrides"].as_array().unwrap().len(), 1);
        assert_eq!(rows["overrides"][0]["dormant"], true);
        assert!(s.doc().unwrap().blocks.values().any(|b| b.description.len() > 255));
        for file in ["override.dxf", "override.dwg"] {
            let bytes = cadcraft_io::write(s.doc().unwrap(), file).unwrap();
            let d = cadcraft_io::read(&bytes, file).unwrap();
            let value = find(&d, &template, &source).unwrap();
            let mut r = Request { anchor: Pt { x: 0.0, y: 0.0 }, width: 2.0, height: 1.0, origin: None, locked: false };
            value.apply(&mut r, 1.0);
            assert_eq!(r.origin, Some(Pt { x: -3.0, y: 1.0 }));
        }
        p["action"] = json!("clear");
        p.as_object_mut().unwrap().remove("preferred_quadrant");
        s.execute("calloutoverride", &p).unwrap();
        assert!(find(s.doc().unwrap(), &template, &source).is_none());
        assert_eq!(s.execute("calloutoverride", &p).unwrap()["cleared"], false);
    }
}
