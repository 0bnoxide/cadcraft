//! Explicit-role sheet templates and ownership-limited current-drawing updates.
use super::{CommandSpec, bad};
use crate::{Result, Session};
use serde_json::Value;
pub fn specs() -> Vec<CommandSpec> {
    vec![
    CommandSpec::new("sheetcreate","Create Sheets",create).params("{group,template,template_layout,arrangement:plan_profile|plan_plan|profile_profile|plan_only|profile_only,layout_name?:Sheet-{n},number_start?:1,number_increment?:1,number_width?:0,north_arrow?,storage?:current} -> {layouts,warnings}"),
    CommandSpec::new("sheetupdate","Update Sheets",update).params("{group,add_frames?:[handle]} -> {changed,layouts,gaps_overlaps,warnings}; updates only owned sheets"),
]
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::run(s, p, false)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("sheetcreate", "template paths are unavailable on the web"))
    }
}
fn update(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::run(s, p, true)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("sheetupdate", "template paths are unavailable on the web"))
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod native {
    use super::{Result, Session, Value, bad};
    use crate::cmd::{
        alignment_cmd::selected,
        alignment_store, figureexport, file, merge, sheet_frames,
        sheet_store::{self as store, Group, Sheet, SheetConfig, SheetRun},
    };
    use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Layout, Point, Space};
    use cadcraft_geom::{Vec2, Vec3};
    use cadcraft_proposal::figures::{Qa, Severity};
    use serde::Serialize;
    use serde_json::json;
    use std::{
        collections::{BTreeMap, BTreeSet},
        io::Write,
        path::Path,
        sync::Arc,
    };
    const CMD: &str = "sheets";
    const MAX_TEMPLATE_ENTITIES: usize = 10_000;
    pub(super) fn digest(value: &impl Serialize) -> Result<String> {
        struct Capped(Vec<u8>);
        impl Write for Capped {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(b.len()) > 16 << 20 {
                    return Err(std::io::Error::other("sheet data exceeds 16MiB"));
                }
                self.0.extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut bytes = Capped(Vec::new());
        serde_json::to_writer(&mut bytes, value).map_err(|e| bad(CMD, e.to_string()))?;
        Ok(cadcraft_proposal::approval::sha256(&bytes.0))
    }
    fn string(p: &Value, key: &str, default: Option<&str>) -> Result<String> {
        let value = match p.get(key) {
            None => default,
            Some(Value::String(v)) => Some(v.as_str()),
            _ => None,
        }
        .ok_or_else(|| bad(CMD, format!("{key} must be a string")))?;
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(bad(CMD, format!("invalid {key}")));
        }
        Ok(value.into())
    }
    fn unsigned(p: &Value, key: &str, default: u32) -> Result<u32> {
        p.get(key).map_or(Ok(default), |v| {
            v.as_u64().and_then(|v| u32::try_from(v).ok()).ok_or_else(|| bad(CMD, format!("{key} requires an unsigned integer")))
        })
    }
    fn config(p: &Value) -> Result<SheetConfig> {
        let width = unsigned(p, "number_width", 0)? as usize;
        let increment = unsigned(p, "number_increment", 1)?;
        if width > 16 || increment == 0 {
            return Err(bad(CMD, "number_width must be 0..16 and number_increment positive"));
        }
        Ok(SheetConfig {
            template: std::fs::canonicalize(string(p, "template", None)?)
                .map_err(|e| bad(CMD, format!("TEMPLATE_READ: {e}")))?
                .to_string_lossy()
                .into_owned(),
            template_layout: string(p, "template_layout", None)?,
            arrangement: string(p, "arrangement", None)?,
            layout_name: string(p, "layout_name", Some("Sheet-{n}"))?,
            number_start: unsigned(p, "number_start", 1)?,
            number_increment: increment,
            number_width: width,
            north_arrow: p.get("north_arrow").map(|_| string(p, "north_arrow", None)).transpose()?,
        })
    }
    fn roles(arrangement: &str, site: bool) -> Result<Vec<&'static str>> {
        let result = match arrangement {
            "plan_profile" if !site => vec!["SHEET-PLAN-1", "SHEET-PROFILE-1"],
            "plan_plan" => vec!["SHEET-PLAN-1", "SHEET-PLAN-2"],
            "profile_profile" if !site => vec!["SHEET-PROFILE-1", "SHEET-PROFILE-2"],
            "plan_only" => vec!["SHEET-PLAN-1"],
            "profile_only" if !site => vec!["SHEET-PROFILE-1"],
            _ => return Err(bad(CMD, "INVALID_ARRANGEMENT: arrangement unavailable for this group")),
        };
        Ok(result)
    }
    fn two_up(c: &SheetConfig) -> bool {
        matches!(c.arrangement.as_str(), "plan_plan" | "profile_profile")
    }
    fn template(c: &SheetConfig, g: &Group) -> Result<(Drawing, Layout, String)> {
        let path = Path::new(&c.template);
        if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("dxf") || s.eq_ignore_ascii_case("dwg")) {
            return Err(bad(CMD, "TEMPLATE_FORMAT: DWG or DXF required"));
        }
        let bytes = crate::capped_read::read_capped(path, file::MAX_DRAWING_BYTES).map_err(|e| bad(CMD, format!("TEMPLATE_READ: {e}")))?;
        let hooks = file::io().ok_or_else(|| bad(CMD, "TEMPLATE_IO: drawing reader unavailable"))?;
        let d = (hooks.read)(&bytes, &c.template).map_err(|e| bad(CMD, format!("TEMPLATE_READ: {e}")))?;
        if !d.read_notes.is_empty() || !d.skipped_records.is_empty() {
            return Err(bad(CMD, "TEMPLATE_LOSS: reader reported omitted content"));
        }
        let l = d.layout(&c.template_layout).cloned().ok_or_else(|| bad(CMD, "TEMPLATE_LAYOUT: named layout missing"))?;
        if l.entities.len() > MAX_TEMPLATE_ENTITIES {
            return Err(bad(CMD, "TEMPLATE_LIMIT: more than 10000 layout entities"));
        }
        let expected = roles(&c.arrangement, g.alignment.is_none())?;
        let mut seen = BTreeSet::new();
        let mut arrows = BTreeSet::new();
        for e in l.entities.iter() {
            check_entity(&d, e)?;
            match &e.kind {
                EntityKind::Viewport(v) if v.id != 1 => {
                    let role = e.common.layer.as_str();
                    if !expected.contains(&role) || !seen.insert(role.to_string()) {
                        return Err(bad(CMD, "TEMPLATE_ROLES: unexpected or duplicate viewport role"));
                    }
                    if !v.width.is_finite() || !v.height.is_finite() || v.width <= 0. || v.height <= 0. || !v.center.is_finite() || v.clip.is_some() {
                        return Err(bad(CMD, "TEMPLATE_VIEWPORT: finite positive unclipped viewport required"));
                    }
                    if role.contains("PLAN") && ((v.width - g.paper_size[0]).abs() > 1e-6 || (v.height - g.paper_size[1]).abs() > 1e-6) {
                        return Err(bad(CMD, "TEMPLATE_SIZE: plan viewport differs from frame paper size"));
                    }
                }
                EntityKind::Insert(i) if e.common.layer.starts_with("SHEET-NORTH-") => {
                    if let Some(name) = &c.north_arrow {
                        let slot = e.common.layer.strip_prefix("SHEET-NORTH-").unwrap_or_default();
                        if !i.block.eq_ignore_ascii_case(name)
                            || !expected.contains(&format!("SHEET-PLAN-{slot}").as_str())
                            || !arrows.insert(slot.to_string())
                        {
                            return Err(bad(CMD, "TEMPLATE_NORTH: arrow block/slot mismatch"));
                        }
                    }
                }
                _ => {}
            }
        }
        if seen.len() != expected.len() {
            return Err(bad(CMD, "TEMPLATE_ROLES: required viewport role missing"));
        }
        if let Some(name) = &c.north_arrow {
            if d.block(name).is_none() {
                return Err(bad(CMD, "TEMPLATE_BLOCK: north arrow block missing"));
            }
            for role in expected.iter().filter(|r| r.contains("PLAN")) {
                if !arrows.contains(role.rsplit('-').next().unwrap_or_default()) {
                    return Err(bad(CMD, "TEMPLATE_NORTH: required arrow insert missing"));
                }
            }
        }
        Ok((d, l, cadcraft_proposal::approval::sha256(&bytes)))
    }
    fn block_ref(e: &Entity) -> Option<&str> {
        match &e.kind {
            EntityKind::Insert(i) => Some(&i.block),
            EntityKind::Dimension(d) => d.block.as_deref(),
            EntityKind::MLeader(m) => m.block.as_ref().map(|b| b.block.as_str()),
            _ => None,
        }
    }
    fn check_entity(d: &Drawing, e: &Entity) -> Result<()> {
        if matches!(e.kind, EntityKind::Unknown(_) | EntityKind::Image(_)) || d.foreign.entities.get(&e.handle).is_some_and(|x| !x.is_empty()) {
            return Err(bad(CMD, "TEMPLATE_REFERENCE: unsupported external or extension data on template entity"));
        }
        if let EntityKind::Dimension(dim) = &e.kind
            && !dim.assoc.is_empty()
        {
            return Err(bad(CMD, "TEMPLATE_REFERENCE: associative dimension source references cannot be copied"));
        }
        if e.common.xdata.iter().any(|t| t.code == 1005) || matches!(&e.kind,EntityKind::Viewport(v) if v.clip.is_some()) {
            return Err(bad(CMD, "TEMPLATE_REFERENCE: source handle references require an unclipped self-contained template"));
        }
        Ok(())
    }
    fn import(d: &mut Drawing, src: &Drawing, l: &Layout, warnings: &mut Vec<Qa>) -> Result<()> {
        let mut queue: Vec<(String, usize, bool)> =
            l.entities.iter().filter_map(|e| block_ref(e)).map(|s| (s.to_ascii_uppercase(), 0, false)).collect();
        let mut needed = BTreeSet::new();
        // Keep the active path separate from completed blocks so cycles are refused.
        let mut active = BTreeSet::new();
        let mut count = 0usize;
        while let Some((name, depth, leaving)) = queue.pop() {
            if leaving {
                active.remove(&name);
                needed.insert(name);
                continue;
            }
            if active.contains(&name) {
                return Err(bad(CMD, "TEMPLATE_CYCLE: recursive block reference"));
            }
            if needed.contains(&name) {
                continue;
            }
            if depth >= 32 || needed.len().saturating_add(active.len()) >= 4096 {
                return Err(bad(CMD, "TEMPLATE_LIMIT: block depth/count limit"));
            }
            let b = src.block(&name).ok_or_else(|| bad(CMD, "TEMPLATE_BLOCK: referenced block missing"))?;
            if b.xref_path.is_some() || src.foreign.records.get(&("BLOCK".into(), name.clone())).is_some_and(|x| !x.is_empty()) {
                return Err(bad(CMD, "TEMPLATE_REFERENCE: external block references require a self-contained template"));
            }
            active.insert(name.clone());
            queue.push((name, depth, true));
            count = count.saturating_add(b.entities.len());
            if count > 100_000 {
                return Err(bad(CMD, "TEMPLATE_LIMIT: too many block entities"));
            }
            for e in b.entities.iter() {
                check_entity(src, e)?;
                if let Some(name) = block_ref(e) {
                    queue.push((name.to_ascii_uppercase(), depth + 1, false));
                }
            }
        }
        for name in &needed {
            let b = src.block(name).ok_or_else(|| bad(CMD, "TEMPLATE_BLOCK: block disappeared"))?;
            if let Some(existing) = d.block(name) {
                let left = existing.entities.iter().map(|e| (&e.common, &e.kind)).collect::<Vec<_>>();
                let right = b.entities.iter().map(|e| (&e.common, &e.kind)).collect::<Vec<_>>();
                let mut metadata = (**b).clone();
                metadata.name = existing.name.clone();
                metadata.entities = existing.entities.clone();
                if **existing != metadata || digest(&left)? != digest(&right)? {
                    return Err(bad(CMD, format!("BLOCK_COLLISION: {} has another definition", b.name)));
                }
            } else {
                let mut copy = (**b).clone();
                copy.entities = Default::default();
                for e in b.entities.iter() {
                    let mut e = (**e).clone();
                    e.handle = d.new_handle();
                    copy.entities.push(e);
                }
                d.blocks.insert(copy.name.clone(), Arc::new(copy));
            }
        }
        macro_rules! tables {
            ($field:ident) => {{
                if src.$field.len() > 4096 || d.$field.len() > 100_000 {
                    return Err(bad(CMD, "TEMPLATE_LIMIT: oversized symbol table"));
                }
                let current: BTreeMap<_, _> = d.$field.iter().map(|v| (v.name.to_ascii_uppercase(), v)).collect();
                for v in &src.$field {
                    if let Some(old) = current.get(&v.name.to_ascii_uppercase())
                        && *old != v
                    {
                        warnings.push(Qa::new(Severity::Warning, "table_definition_kept", &v.name, "Current drawing definition retained"));
                    }
                }
                merge::merge_named(&mut d.$field, &src.$field, |v| &v.name);
            }};
        }
        tables!(layers);
        tables!(linetypes);
        tables!(text_styles);
        tables!(dim_styles);
        tables!(mleader_styles);
        tables!(table_styles);
        tables!(mline_styles);
        Ok(())
    }
    fn values(g: &Group, a: Option<&alignment_store::Stored>, indices: &[usize], number: &str, count: usize) -> BTreeMap<String, String> {
        let mut out = BTreeMap::from([
            ("SheetNumber".into(), number.into()),
            ("SheetCount".into(), count.to_string()),
            ("Alignment".into(), a.map_or_else(String::new, |a| a.definition.name.clone())),
        ]);
        let range = indices
            .iter()
            .filter_map(|i| g.frames.get(*i))
            .fold([f64::INFINITY, f64::NEG_INFINITY], |r, f| [r[0].min(f.geometry.range[0]), r[1].max(f.geometry.range[1])]);
        for (key, station) in [("StationStart", range[0]), ("StationEnd", range[1])] {
            out.insert(key.into(), a.and_then(|a| a.definition.format.display(station).ok()).unwrap_or_default());
        }
        out
    }
    fn name(c: &SheetConfig, number: &str, values: &BTreeMap<String, String>, warnings: &mut Vec<Qa>) -> Result<String> {
        let name = figureexport::native::substitute(&c.layout_name.replace("{n}", number), values, warnings, "layout name", false)
            .map_err(|e| bad(CMD, e))?;
        if name.trim().is_empty()
            || name.len() > 255
            || name.chars().any(|c| c.is_control() || "<>/\\:*?\"|[]".contains(c))
            || name.eq_ignore_ascii_case("Model")
        {
            return Err(bad(CMD, "INVALID_SHEET_NAME: invalid layout name"));
        }
        Ok(name)
    }
    fn member_hash(d: &Drawing, s: &Sheet) -> Result<String> {
        let layout = d.layout(&s.name).ok_or_else(|| bad(CMD, "OWNERSHIP: owned layout missing"))?;
        let mut data = Vec::new();
        for h in &s.members {
            let h = store::handle(h)?;
            let e = layout.entities.get(h).ok_or_else(|| bad(CMD, "OWNERSHIP: owned sheet entity missing"))?;
            data.push((&e.common, &e.kind));
        }
        digest(&(&layout.page, layout.view, layout.limits, data))
    }
    fn owned(d: &Drawing, g: &Group, run: &SheetRun) -> Result<bool> {
        if run.layouts.len() > 4096 {
            return Err(bad(CMD, "OWNERSHIP: sheet count limit"));
        }
        let mut unchanged = true;
        let mut names = BTreeSet::new();
        let mut handles = BTreeSet::new();
        for sheet in &run.layouts {
            if !names.insert(sheet.name.to_ascii_uppercase())
                || sheet.members.len() > store::MAX_MEMBERS
                || sheet.frames.is_empty()
                || sheet.frames.len() > 2
                || !sheet.members.contains(&sheet.marker)
            {
                return Err(bad(CMD, "OWNERSHIP: invalid sheet record"));
            }
            let first = *sheet.frames.first().ok_or_else(|| bad(CMD, "OWNERSHIP: no sheet frame"))?;
            let l = d.layout(&sheet.name).ok_or_else(|| bad(CMD, "OWNERSHIP: owned layout missing"))?;
            for h in &sheet.members {
                let h = store::handle(h)?;
                if !handles.insert(h) || !l.entities.contains(h) {
                    return Err(bad(CMD, "OWNERSHIP: copied, missing or duplicate sheet entity"));
                }
                store::check(d, h, &g.own, first, "sheet")?;
            }
            unchanged &= member_hash(d, sheet)? == sheet.hash;
        }
        Ok(unchanged)
    }
    fn prune(d: &mut Drawing, g: &mut Group) -> Result<()> {
        let mut kept = Vec::new();
        for f in std::mem::take(&mut g.frames) {
            if d.entity(store::handle(&f.handle)?).is_some() {
                kept.push(f);
            } else {
                store::erase(d, &f.matchlines, &g.own, f.index, "matchline")?;
                if let Some(p) = &f.profile {
                    store::erase(d, &p.handles, &g.own, f.index, "profile")?;
                }
            }
        }
        g.frames = kept;
        Ok(())
    }
    fn refresh(d: &Drawing, g: &mut Group, a: Option<&alignment_store::Stored>) -> Result<()> {
        if let Some(a) = a {
            sheet_frames::refresh_frames(d, g, a)?;
        } else {
            for f in &mut g.frames {
                let h = store::handle(&f.handle)?;
                store::check(d, h, &g.own, f.index, "frame")?;
                let geometry = sheet_frames::site_geometry(d, h)?;
                if geometry.size.iter().zip(g.paper_size).any(|(s, p)| (s - p * g.scale).abs() > 1e-6) {
                    return Err(bad(CMD, "FRAME_STRETCHED: site frame size changed"));
                }
                f.geometry = geometry;
            }
        }
        Ok(())
    }
    fn add_frames(d: &mut Drawing, g: &mut Group, p: &Value, a: Option<&alignment_store::Stored>) -> Result<()> {
        let Some(raw) = p.get("add_frames") else {
            return Ok(());
        };
        let handles = raw
            .as_array()
            .filter(|v| v.len().saturating_add(g.frames.len()) <= cadcraft_alignment::frames::MAX_FRAMES)
            .ok_or_else(|| bad(CMD, "FRAME_LIMIT: add_frames requires a bounded handle array"))?;
        let mut seen = g.frames.iter().map(|f| store::handle(&f.handle)).collect::<Result<BTreeSet<_>>>()?;
        let mut index = g.frames.iter().map(|f| f.index).max().map_or(Some(0), |i| i.checked_add(1)).ok_or_else(|| bad(CMD, "FRAME_INDEX_LIMIT"))?;
        for value in handles {
            let h = crate::cmd::curves::handle_value(value).ok_or_else(|| bad(CMD, "INVALID_FRAME: invalid added frame handle"))?;
            if !seen.insert(h) || alignment_store::record_handle_for(d, h, "CADCRAFT_SHEET_OWNER")?.is_some() {
                return Err(bad(CMD, "FRAME_OWNED: frame is duplicate or already owned"));
            }
            let geometry = if let Some(a) = a {
                let e = d.model.get(h).ok_or_else(|| bad(CMD, "INVALID_FRAME: added rectangle must be in model space"))?;
                let EntityKind::LwPolyline(poly) = &e.kind else {
                    return Err(bad(CMD, "INVALID_FRAME: road frame must be a rectangle"));
                };
                if e.common.extrusion != Vec3::Z {
                    return Err(bad(CMD, "INVALID_FRAME: nonplanar rectangle"));
                }
                let (center, size, angle) = sheet_frames::rectangle(&sheet_frames::points(poly)?)?;
                // A newly placed rectangle has no earlier chord: its first edge defines the initial chord.
                let half_chord = size[0] / 2.;
                let axis = [half_chord * angle.cos(), half_chord * angle.sin()];
                let view = a.definition.validated().map_err(|e| bad(CMD, e.to_string()))?;
                let start = view.project([center[0] - axis[0], center[1] - axis[1]]).map_err(|e| bad(CMD, e.to_string()))?;
                let end = view.project([center[0] + axis[0], center[1] + axis[1]]).map_err(|e| bad(CMD, e.to_string()))?;
                if start.beyond.is_some() || end.beyond.is_some() {
                    return Err(bad(CMD, "FRAME_RANGE: added frame exceeds alignment"));
                }
                let f = cadcraft_alignment::frames::Frame { range: [start.station, end.station], center, size, angle, half_chord, offset: 0. };
                cadcraft_alignment::frames::contains(&a.definition, &f).map_err(|e| bad(CMD, e.to_string()))?;
                f
            } else {
                sheet_frames::site_geometry(d, h)?
            };
            if geometry.size.iter().zip(g.paper_size).any(|(s, p)| (s - p * g.scale).abs() > 1e-6) {
                return Err(bad(CMD, "FRAME_SIZE_MISMATCH: added rectangle differs from group viewport"));
            }
            store::own(d, h, &g.own, index, "frame")?;
            g.frames.push(store::Member { index, handle: h.hex(), geometry, matchlines: Vec::new(), profile: None });
            index = index.checked_add(1).ok_or_else(|| bad(CMD, "FRAME_INDEX_LIMIT"))?;
        }
        Ok(())
    }
    fn remove_sheets(d: &mut Drawing, g: &Group, run: &SheetRun, names: &BTreeSet<String>) -> Result<()> {
        for sheet in &run.layouts {
            let l = d.layout(&sheet.name).ok_or_else(|| bad(CMD, "OWNERSHIP: layout missing"))?;
            let unowned = l.entities.handles().iter().any(|h| !sheet.members.contains(&h.hex()));
            if unowned && !names.contains(&sheet.name.to_ascii_uppercase()) {
                return Err(bad(CMD, "SHEET_HAS_UNOWNED_OBJECTS: cannot remove a layout containing user objects"));
            }
            store::erase(d, &sheet.members, &g.own, *sheet.frames.first().ok_or_else(|| bad(CMD, "OWNERSHIP: no sheet frame"))?, "sheet")?;
            if !names.contains(&sheet.name.to_ascii_uppercase()) {
                d.layouts.retain(|l| !l.name.eq_ignore_ascii_case(&sheet.name));
            }
        }
        Ok(())
    }
    fn gaps_overlaps(g: &Group) -> Vec<Value> {
        let mut out = Vec::new();
        if g.alignment.is_some() {
            for pair in g.frames.windows(2) {
                if let [left, right] = pair {
                    let gap = right.geometry.range[0] - left.geometry.range[1];
                    if gap.abs() > 1e-6 {
                        out.push(json!({"before":right.index,"distance":gap,"kind":if gap>0.{"gap"}else{"overlap"}}));
                    }
                }
            }
        }
        out
    }
    pub(super) fn run(s: &mut Session, p: &Value, update: bool) -> Result<Value> {
        if p.get("storage").is_some_and(|v| v.as_str() != Some("current")) {
            return Err(bad(CMD, "STORAGE_UNAVAILABLE: this stage supports current drawing storage"));
        }
        let h = selected(s, p, "group", CMD)?;
        let mut g = store::load(s.doc()?, h)?;
        let old = g.sheets.clone();
        let c = if update {
            old.as_ref().ok_or_else(|| bad(CMD, "NO_SHEETS: create sheets first"))?.config.clone()
        } else {
            if old.is_some() {
                return Err(bad(CMD, "SHEETS_EXIST: use sheetupdate"));
            }
            config(p)?
        };
        if c.number_width > 16 || c.number_increment == 0 {
            return Err(bad(CMD, "INVALID_FRAME_GROUP: invalid stored numbering"));
        }
        let a = g
            .alignment
            .as_deref()
            .map(|h| alignment_store::load(s.doc()?, store::handle(h)?))
            .transpose()
            .map_err(|e| bad(CMD, format!("NO_ALIGNMENT: {e}")))?;
        let mut d = s.doc()?.clone();
        prune(&mut d, &mut g)?;
        refresh(&d, &mut g, a.as_ref())?;
        if c.arrangement.contains("profile") {
            if g.profiles.is_none() {
                return Err(bad(CMD, "MISSING_PROFILE_VIEW: create frame profile views first"));
            }
            for f in &g.frames {
                let view = f.profile.as_ref().ok_or_else(|| bad(CMD, "MISSING_PROFILE_VIEW: frame has no owned profile view"))?;
                for h in &view.handles {
                    let h = store::handle(h)?;
                    if !d.model.contains(h) {
                        return Err(bad(CMD, "MISSING_PROFILE_VIEW: an owned profile component was erased"));
                    }
                    store::check(&d, h, &g.own, f.index, "profile")?;
                }
            }
        }
        add_frames(&mut d, &mut g, p, a.as_ref())?;
        let (source, layout, template_hash) = template(&c, &g)?;
        let input = digest(&(&c, &template_hash, &g.frames, &a))?;
        let unchanged = old.as_ref().map(|r| owned(&d, &g, r)).transpose()?.unwrap_or(false);
        if update && unchanged && old.as_ref().is_some_and(|r| r.source_hash == input) {
            return Ok(
                json!({"group":g.own,"changed":false,"layouts":old.as_ref().map(|r|r.layouts.iter().map(|s|&s.name).collect::<Vec<_>>()),"warnings":[],"gaps_overlaps":gaps_overlaps(&g)}),
            );
        }
        let mut profile_report = Value::Null;
        if let Some(a) = &a {
            sheet_frames::redraw_matchlines(&mut d, &mut g, a)?;
            if let Some(settings) = g.profiles.clone() {
                profile_report = sheet_frames::redraw_profiles(&mut d, &mut g, a, &settings)?;
            }
        }
        let per = if two_up(&c) { 2 } else { 1 };
        let count = g.frames.len().div_ceil(per);
        if layout.entities.len().checked_mul(count).is_none_or(|n| n > store::MAX_MEMBERS) {
            return Err(bad(CMD, "SHEET_LIMIT: too many sheet entities"));
        }
        let mut warnings = Vec::new();
        let mut planned = Vec::new();
        let mut names = BTreeSet::new();
        for i in 0..count {
            let n = c
                .number_increment
                .checked_mul(i as u32)
                .and_then(|n| c.number_start.checked_add(n))
                .ok_or_else(|| bad(CMD, "SHEET_NUMBER_OVERFLOW"))?;
            let number = if c.number_width == 0 { n.to_string() } else { format!("{n:0width$}", width = c.number_width) };
            let indices = (i * per..((i + 1) * per).min(g.frames.len())).collect::<Vec<_>>();
            let values = values(&g, a.as_ref(), &indices, &number, count);
            let name = name(&c, &number, &values, &mut warnings)?;
            if !names.insert(name.to_ascii_uppercase())
                || d.layout(&name).is_some() && !old.as_ref().is_some_and(|r| r.layouts.iter().any(|s| s.name.eq_ignore_ascii_case(&name)))
            {
                return Err(bad(CMD, format!("LAYOUT_COLLISION: {name}")));
            }
            planned.push((name, indices, values));
        }
        if let Some(old) = &old {
            remove_sheets(&mut d, &g, old, &names)?;
        }
        import(&mut d, &source, &layout, &mut warnings)?;
        let mut next_viewport = d
            .layouts
            .iter()
            .flat_map(|l| l.entities.iter())
            .filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { Some(v.id) } else { None })
            .max()
            .unwrap_or(1)
            .checked_add(1)
            .ok_or_else(|| bad(CMD, "VIEWPORT_ID_LIMIT"))?;
        let mut sheets = Vec::new();
        let mut unused = Vec::new();
        for (name, indices, values) in planned {
            let mut copied = layout.clone();
            copied.name = name.clone();
            copied.entities = Default::default();
            copied.tab_order = d.layouts.iter().map(|l| l.tab_order).max().unwrap_or(0).saturating_add(1);
            if let Some(existing) = d.layout(&name) {
                copied.tab_order = existing.tab_order;
                copied.entities = existing.entities.clone();
            }
            d.layouts.retain(|l| !l.name.eq_ignore_ascii_case(&name));
            d.layouts.push(copied);
            let first_index = *indices.first().ok_or_else(|| bad(CMD, "NO_FRAMES"))?;
            let first = g.frames.get(first_index).ok_or_else(|| bad(CMD, "NO_FRAMES"))?.index;
            let mut members = Vec::new();
            for original in layout.entities.iter() {
                let mut e = (**original).clone();
                let role = e.common.layer.clone();
                let slot = if role.ends_with("-2") { 1 } else { 0 };
                if two_up(&c) && slot == 1 && indices.len() == 1 && (matches!(e.kind, EntityKind::Viewport(_)) || role == "SHEET-NORTH-2") {
                    continue;
                }
                let frame = indices.get(slot).and_then(|i| g.frames.get(*i));
                match &mut e.kind {
                    EntityKind::Viewport(v) if v.id != 1 => {
                        let f = frame.ok_or_else(|| bad(CMD, "TEMPLATE_ROLES: missing frame for slot"))?;
                        v.id = next_viewport;
                        next_viewport = next_viewport.checked_add(1).ok_or_else(|| bad(CMD, "VIEWPORT_ID_LIMIT"))?;
                        v.locked = true;
                        v.view_height = v.height * g.scale;
                        if role.contains("PLAN") {
                            v.view_center = Vec2::new(f.geometry.center[0], f.geometry.center[1]);
                            v.twist = -f.geometry.angle;
                        } else {
                            let p = f.profile.as_ref().ok_or_else(|| bad(CMD, "MISSING_PROFILE_VIEW: profile slot has no owned figure"))?;
                            for h in &p.handles {
                                store::check(&d, store::handle(h)?, &g.own, f.index, "profile")?;
                            }
                            let width = p.window.range[1] - p.window.range[0];
                            let height = (p.window.elevation[1] - p.window.elevation[0]) * p.exaggeration;
                            if width > v.width * g.scale + 1e-6 || height > v.height * g.scale + 1e-6 {
                                return Err(bad(CMD, "PROFILE_VIEW_OUTSIDE: profile figure does not fit at the horizontal scale"));
                            }
                            v.view_center = Vec2::new(p.at[0] + width / 2., p.at[1] + height / 2.);
                            v.twist = 0.;
                        }
                        if !v.view_height.is_finite() || v.view_height <= 0. || !v.view_center.is_finite() {
                            return Err(bad(CMD, "VIEWPORT_OVERFLOW: scale or view centre cannot be represented"));
                        }
                    }
                    EntityKind::Text(t) => {
                        t.value = figureexport::native::substitute(&t.value, &values, &mut warnings, &name, false).map_err(|e| bad(CMD, e))?
                    }
                    EntityKind::MText(t) => {
                        t.contents = figureexport::native::substitute(&t.contents, &values, &mut warnings, &name, true).map_err(|e| bad(CMD, e))?
                    }
                    EntityKind::Insert(i) => {
                        if role.starts_with("SHEET-NORTH-") && c.north_arrow.is_some() {
                            i.rotation = -frame.ok_or_else(|| bad(CMD, "TEMPLATE_NORTH: missing frame"))?.geometry.angle;
                        }
                        for a in &mut i.attribs {
                            a.text.value =
                                figureexport::native::substitute(&a.text.value, &values, &mut warnings, &name, false).map_err(|e| bad(CMD, e))?;
                        }
                    }
                    _ => {}
                }
                let h = d.add(&Space::Paper(name.clone()), e.common, e.kind)?;
                store::own(&mut d, h, &g.own, first, "sheet")?;
                members.push(h.hex());
            }
            let marker = d.add(
                &Space::Paper(name.clone()),
                Common { visible: false, ..Default::default() },
                EntityKind::Point(Point { p: Vec3::ZERO, angle: 0. }),
            )?;
            store::own(&mut d, marker, &g.own, first, "sheet")?;
            members.push(marker.hex());
            let mut sheet = Sheet {
                name: name.clone(),
                marker: marker.hex(),
                frames: indices.iter().filter_map(|i| g.frames.get(*i)).map(|f| f.index).collect(),
                members,
                hash: String::new(),
            };
            sheet.hash = member_hash(&d, &sheet)?;
            if two_up(&c) && indices.len() == 1 {
                unused.push(json!({"layout":name,"slot":2}));
            }
            sheets.push(sheet);
        }
        let gaps = gaps_overlaps(&g);
        let layouts = sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        let input = digest(&(&c, &template_hash, &g.frames, &a))?;
        if d.layouts.is_empty() {
            return Err(bad(CMD, "LAST_LAYOUT: keep one paper layout before removing every sheet"));
        }
        g.sheets = Some(SheetRun { config: c, source_hash: input, layouts: sheets });
        store::save(&mut d, &g)?;
        let remaining = d.layouts.iter().map(|l| l.name.to_ascii_uppercase()).collect::<BTreeSet<_>>();
        *s.doc_mut()? = d;
        let state = s.state_mut()?;
        state.views.retain(|(space, _)| space.layout().is_none_or(|name| remaining.contains(&name.to_ascii_uppercase())));
        if state.space.layout().is_some_and(|name| !remaining.contains(&name.to_ascii_uppercase())) {
            state.space = Space::Model;
            state.mspace = None;
            state.selection.clear();
        }
        Ok(
            json!({"group":g.own,"changed":true,"layouts":layouts,"warnings":warnings,"unused_slots":unused,"gaps_overlaps":gaps,"profiles":profile_report}),
        )
    }
}
