//! NEWPROPOSAL: plan, preview, approve and create a proposal project from a standards manifest
//! and DXF/DWG templates. Owner: Codex (see `docs/agents/slice-7-proposal.md`).

#[cfg(not(target_arch = "wasm32"))]
use super::str_param;
use super::{CommandSpec, bad};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("newproposal", "New Proposal", run)
        .menu(&["File", "New Proposal"])
        .params("{action: plan|create, manifest_path, inputs: {client_name,site_name,proposal_year,orientation,sheet_size,client_number?,project_number?,proposal_number?,site_address?,project_manager?}, approved_hash?}")
        .noundo().interactive(|_|Ok(Box::new(ProposalPrompt::default())))]
}

fn run(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("newproposal", "Project filesystem creation is not available on web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::run(p, super::file::io().ok_or_else(|| bad("newproposal", "Drawing formats are not available"))?)
    }
}

#[derive(Default)]
struct ProposalPrompt {
    index: usize,
    params: Value,
    hash: Option<String>,
}
const FIELDS: [&str; 11] = [
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
    "approval",
];
impl Interactive for ProposalPrompt {
    fn name(&self) -> &'static str {
        "NEWPROPOSAL"
    }
    fn prompt(&self, _: &Session) -> Prompt {
        let label = if self.index == 0 { "Standards manifest path" } else { FIELDS.get(self.index - 1).copied().unwrap_or("approval") };
        Prompt::new(if label == "approval" { "Approve this exact plan? Type Yes to create" } else { label }, Accept::TEXT)
    }
    fn input(&mut self, s: &mut Session, input: Input) -> Result<Step> {
        if input == Input::Cancel {
            return Ok(Step::Cancel);
        }
        let value = match input {
            Input::Text(v) | Input::Keyword(v) => v,
            Input::Enter => String::new(),
            _ => return Ok(Step::Continue),
        };
        if self.index == 0 {
            self.params = json!({"manifest_path":value,"action":"plan","inputs":{}});
        } else if let Some(field) = FIELDS.get(self.index - 1) {
            if *field == "approval" {
                if !value.eq_ignore_ascii_case("yes") {
                    s.echo("New Proposal cancelled");
                    return Ok(Step::Done);
                }
                self.params["action"] = json!("create");
                self.params["approved_hash"] = json!(self.hash);
                let result = run(s, &self.params)?;
                s.echo(serde_json::to_string(&result).map_err(|e| bad("newproposal", e.to_string()))?);
                return Ok(Step::Done);
            }
            self.params["inputs"][field] = if *field == "proposal_year" {
                json!(value.parse::<i32>().map_err(|_| bad("newproposal", "Year must be an integer"))?)
            } else if self.index > 5 && value.is_empty() {
                Value::Null
            } else {
                json!(value)
            };
        }
        self.index += 1;
        if self.index == 11 {
            let preview = run(s, &self.params)?;
            self.hash = preview.get("hash").and_then(Value::as_str).map(str::to_string);
            s.echo(serde_json::to_string_pretty(&preview).map_err(|e| bad("newproposal", e.to_string()))?);
            if self.hash.is_none() {
                return Ok(Step::Done);
            }
        }
        Ok(Step::Continue)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Result, Value, bad, json, str_param};
    use cadcraft_doc::{Block, Drawing, EntityKind, Insert, Layout, Space, Viewport};
    use cadcraft_geom::{Vec2, Vec3};
    use cadcraft_proposal::{
        Inputs, Issue,
        approval::{self, Outcome, Receipt},
        manifest::{Manifest, Profile},
        planner::{self, Operation, Plan},
    };
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        io::Write,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    type Checked<T> = std::result::Result<T, String>;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const MAX_TEMPLATE: usize = 32 << 20;

    fn reject_link(path: &Path) -> Checked<()> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err("Reparse paths are not allowed".into());
                    }
                }
                if meta.file_type().is_symlink() {
                    return Err("Symlink paths are not allowed".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        Ok(())
    }
    pub(super) fn child(stage: &Path, relative: &str) -> Checked<PathBuf> {
        let relative = cadcraft_proposal::paths::relative(relative).map_err(|e| e.message)?;
        let path = stage.join(relative);
        // Admin redirects above the reserved stage are allowed; links inside it are not.
        for ancestor in path.ancestors().take_while(|p| p.starts_with(stage)) {
            reject_link(ancestor)?;
        }
        Ok(path)
    }
    fn read(path: &Path, limit: usize) -> Checked<Vec<u8>> {
        crate::capped_read::read_capped(path, limit).map_err(|e| e.to_string())
    }
    fn write_new(path: &Path, bytes: &[u8]) -> Checked<()> {
        reject_link(path)?;
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
        f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
    }
    fn encoded(value: &impl serde::Serialize) -> Checked<Vec<u8>> {
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())
    }
    fn draw_read(h: &super::super::file::IoHooks, path: &Path) -> Checked<Drawing> {
        (h.read)(&read(path, MAX_TEMPLATE)?, &path.to_string_lossy())
    }
    fn save(h: &super::super::file::IoHooks, path: &Path, d: &Drawing, fresh: bool) -> Checked<()> {
        let bytes = (h.write)(d, &path.to_string_lossy())?;
        if fresh {
            write_new(path, &bytes)
        } else {
            reject_link(path)?;
            let mut f = fs::OpenOptions::new().write(true).truncate(true).open(path).map_err(|e| e.to_string())?;
            f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
        }
    }
    fn receipt(plan: &Plan, hash: String) -> Receipt {
        let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
        Receipt {
            version: 1,
            run_id: format!("{}-{}-{}", std::process::id(), now.timestamp_micros(), NEXT.fetch_add(1, Ordering::Relaxed)),
            started_at_utc: now.to_rfc3339(),
            plan_sha256: hash,
            outcome: Outcome::Refused,
            final_root: plan.final_root.clone(),
            receipt_path: None,
            verified_artifacts: Vec::new(),
            manual_steps: vec!["Existing ground data is pending; populate the shortcut folder when available".into()],
            failed_action_id: None,
            failed_checks: Vec::new(),
            failure_detail: None,
            cleanup_failures: Vec::new(),
            reporting_failure: None,
            retained_staging_root: None,
        }
    }
    pub(super) fn run(p: &Value, h: &super::super::file::IoHooks) -> Result<Value> {
        let command = str_param(p, "action").unwrap_or("plan");
        if !matches!(command, "plan" | "create") {
            return Err(bad("newproposal", "action must be plan or create"));
        }
        let manifest_path = PathBuf::from(str_param(p, "manifest_path").ok_or_else(|| bad("newproposal", "manifest_path is required"))?);
        let manifest_path = if manifest_path.is_absolute() {
            manifest_path
        } else {
            std::env::current_dir().map_err(|e| bad("newproposal", e.to_string()))?.join(manifest_path)
        };
        let bytes = read(&manifest_path, 1 << 20).map_err(|e| bad("newproposal", e))?;
        let m = match Manifest::parse(&bytes) {
            Ok(m) => m,
            Err(issues) => return Ok(json!({"plan":null,"hash":null,"issues":issues})),
        };
        let raw_inputs = p.get("inputs").cloned().unwrap_or(Value::Null);
        if raw_inputs.as_object().is_some_and(|o| o.keys().any(|key| !cadcraft_proposal::manifest::INPUT_FIELDS.contains(&key.as_str()))) {
            return Err(bad("newproposal", "Unknown proposal input field"));
        }
        let inputs: Inputs = serde_json::from_value(raw_inputs).map_err(|e| bad("newproposal", e.to_string()))?;
        let mut fingerprints = BTreeMap::from([("manifest".into(), approval::sha256(&bytes))]);
        let mut templates = BTreeMap::new();
        let mut template_bytes = 0usize;
        let dir = manifest_path.parent().ok_or_else(|| bad("newproposal", "Manifest has no directory"))?;
        for name in m.templates() {
            if cadcraft_proposal::paths::absolute(name).is_ok() && !Path::new(name).is_absolute() {
                return Err(bad("newproposal", "Template uses an absolute path for another operating system"));
            }
            let path = if Path::new(name).is_absolute() { PathBuf::from(name) } else { dir.join(name) };
            let bytes = read(&path, MAX_TEMPLATE).map_err(|e| bad("newproposal", e))?;
            template_bytes = template_bytes.saturating_add(bytes.len());
            if template_bytes > 64 << 20 {
                return Err(bad("newproposal", "Combined templates exceed 64 MiB"));
            }
            fingerprints.insert(name.to_string(), approval::sha256(&bytes));
            templates.insert(name.to_string(), bytes);
        }
        let plan = match planner::build(&inputs, &m, &fingerprints) {
            Ok(plan) => plan,
            Err(issues) => return Ok(json!({"plan":null,"hash":null,"issues":issues})),
        };
        let hash = approval::plan_hash(&plan).map_err(|e| bad("newproposal", e.message))?;
        if command == "plan" {
            return Ok(json!({"plan":plan,"hash":hash,"issues":[]}));
        }
        let mut receipt = receipt(&plan, hash);
        if let Err(issue) = approval::check_approval(&plan, str_param(p, "approved_hash")) {
            if issue.code == "approval_required" {
                receipt.outcome = Outcome::Cancelled;
            }
            receipt.failed_checks.push(issue);
            return Ok(json!({"receipt":receipt}));
        }
        let target = Path::new(&plan.final_root);
        if !target.is_absolute() {
            receipt.failed_checks.push(Issue::new("unsafe_path", "Base root is not absolute on this operating system"));
            return Ok(json!({"receipt":receipt}));
        }
        if fs::symlink_metadata(target).is_ok() {
            receipt.failed_checks.push(Issue::new("target_exists", "Target already exists"));
            return Ok(json!({"receipt":receipt}));
        }
        let parent = target.parent().ok_or_else(|| bad("newproposal", "Target has no parent"))?;
        if let Err(e) = reject_link(target).and_then(|_| if parent.is_dir() { Ok(()) } else { Err("Base root must exist and be a directory".into()) })
        {
            receipt.failed_checks.push(Issue::new("preflight_failed", e));
            return Ok(json!({"receipt":receipt}));
        }
        let stage = parent.join(format!(".newproposal-{}.stage", receipt.run_id));
        if let Err(e) = fs::create_dir(&stage) {
            receipt.failed_checks.push(Issue::new("preflight_failed", e.to_string()));
            return Ok(json!({"receipt":receipt}));
        }
        let receipt_relative = m
            .support_records()
            .iter()
            .find(|r| r.role == "CreationReceipt")
            .map(|r| r.path.clone())
            .ok_or_else(|| bad("newproposal", "Missing receipt"))?;
        let result = execute(&plan, &templates, h, &stage, &mut receipt).and_then(|_| {
            receipt.failed_action_id = Some("PrepareReceipt".into());
            let mut success = receipt.clone();
            success.outcome = Outcome::Succeeded;
            success.failed_action_id = None;
            success.receipt_path = Some(target.join(&receipt_relative).to_string_lossy().into_owned());
            write_new(&child(&stage, &receipt_relative)?, &encoded(&success)?)?;
            receipt.failed_action_id = Some("Publish".into());
            reject_link(target)?;
            if fs::symlink_metadata(target).is_ok() {
                return Err("Target appeared before publication".into());
            }
            // shortcut: Unix rename may replace a concurrently created empty directory; use a no-replace API if that narrow race matters.
            fs::rename(&stage, target).map_err(|e| e.to_string())?;
            receipt = success;
            Ok(())
        });
        if let Err(e) = result {
            receipt.outcome = Outcome::Failed;
            receipt.failure_detail = Some(e.clone());
            receipt.receipt_path = None;
            let code = if receipt.failed_action_id.as_deref() == Some("Verify") { "verification_failed" } else { "execution_failed" };
            receipt.failed_checks.push(Issue::new(code, e));
            let success_receipt = stage.join(&receipt_relative);
            if success_receipt.exists()
                && let Err(e) = fs::remove_file(&success_receipt)
            {
                receipt.cleanup_failures.push(e.to_string());
            }
            if let Err(e) = fs::remove_dir_all(&stage) {
                receipt.cleanup_failures.push(e.to_string());
                receipt.retained_staging_root = Some(stage.to_string_lossy().into_owned());
            }
            let failed = parent.join(format!(".newproposal-{}.failed.json", receipt.run_id));
            receipt.receipt_path = Some(failed.to_string_lossy().into_owned());
            if let Err(e) = encoded(&receipt).and_then(|b| write_new(&failed, &b)) {
                receipt.receipt_path = None;
                receipt.reporting_failure = Some(e);
            }
        }
        Ok(json!({"receipt":receipt}))
    }

    fn support(plan: &Plan, role: &str) -> Value {
        match role {
            "CreationPlan" => json!(plan),
            "ProjectConfiguration" => {
                json!({"version":1,"final_root":plan.final_root,"inputs":plan.inputs,"standards":plan.manifest,"existing_ground":"Pending"})
            }
            "SheetIndex" => json!({"version":1,"sheets":plan.manifest.sheets()}),
            _ => json!({"version":1,"entries":[]}),
        }
    }
    fn execute(
        plan: &Plan,
        templates: &BTreeMap<String, Vec<u8>>,
        h: &super::super::file::IoHooks,
        stage: &Path,
        receipt: &mut Receipt,
    ) -> Checked<()> {
        let mut completed = BTreeSet::new();
        let mut expected = BTreeMap::new();
        for action in &plan.actions {
            receipt.failed_action_id = Some(action.id.clone());
            if !action.dependencies.iter().all(|d| completed.contains(d)) {
                return Err("Actions are not topological".into());
            }
            reject_link(stage)?;
            if fs::symlink_metadata(&plan.final_root).is_ok() {
                return Err("Target appeared during execution".into());
            }
            let path = child(stage, &action.relative_path)?;
            match &action.operation {
                Operation::Folder { .. } => fs::create_dir(&path).map_err(|e| e.to_string())?,
                Operation::Model { template, .. } => {
                    let bytes = templates.get(template).ok_or("Template is missing")?;
                    let d = (h.read)(bytes, template)?;
                    reject_external_templates(&d)?;
                    save(h, &path, &d, true)?;
                    expected.insert(action.relative_path.clone(), d);
                }
                Operation::Sheet { profile, sheet } => {
                    let bytes = templates.get(&profile.template).ok_or("Template is missing")?;
                    let mut d = (h.read)(bytes, &profile.template)?;
                    reject_external_templates(&d)?;
                    make_sheet(&mut d, profile, plan, &sheet.number, &sheet.title)?;
                    save(h, &path, &d, true)?;
                    expected.insert(action.relative_path.clone(), d);
                }
                Operation::Xref { reference, relative_reference_path } => {
                    let d = expected.get_mut(&action.relative_path).ok_or("Host drawing missing")?;
                    if d.block(&reference.reference_role).is_some() {
                        return Err("Template block collides with reference role".into());
                    }
                    let mut block = Block::new(&reference.reference_role);
                    block.xref_path = Some(relative_reference_path.clone());
                    block.xref_overlay = true;
                    d.blocks.insert(reference.reference_role.clone(), Arc::new(block));
                    d.add(
                        &Space::Model,
                        Default::default(),
                        EntityKind::Insert(Insert {
                            block: reference.reference_role.clone(),
                            insert: Vec3::new(reference.insertion.x, reference.insertion.y, reference.insertion.z),
                            scale: Vec3::new(reference.scale, reference.scale, reference.scale),
                            rotation: reference.rotation.to_radians(),
                            ..Default::default()
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                    save(h, &path, d, false)?;
                }
                Operation::SheetIndex => write_new(&path, &encoded(&support(plan, "SheetIndex"))?)?,
                Operation::Support { role } if role == "CreationReceipt" => continue,
                Operation::Support { role } => write_new(&path, &encoded(&support(plan, role))?)?,
            }
            completed.insert(action.id.clone());
        }
        receipt.failed_action_id = Some("Verify".into());
        for (relative, wanted) in &expected {
            let path = child(stage, relative)?;
            let actual = draw_read(h, &path)?;
            verify_drawing(&actual, wanted, plan, relative, stage)?;
        }
        let mut inventory = BTreeSet::new();
        for action in &plan.actions {
            match &action.operation {
                Operation::Support { role } if role == "CreationReceipt" => continue,
                Operation::Support { role } => {
                    let actual: Value = serde_json::from_slice(&read(&child(stage, &action.relative_path)?, 4 << 20)?).map_err(|e| e.to_string())?;
                    if actual != support(plan, role) {
                        return Err("Support record verification failed".into());
                    }
                }
                Operation::SheetIndex => {
                    let actual: Value = serde_json::from_slice(&read(&child(stage, &action.relative_path)?, 4 << 20)?).map_err(|e| e.to_string())?;
                    if actual != support(plan, "SheetIndex") {
                        return Err("Sheet index verification failed".into());
                    }
                }
                _ => {}
            }
            inventory.insert(action.relative_path.clone());
        }
        let mut observed = BTreeSet::new();
        collect_tree(stage, stage, &mut observed)?;
        if inventory != observed {
            return Err("Unexpected or missing staged artifacts".into());
        }
        receipt.verified_artifacts = inventory.into_iter().map(|p| Path::new(&plan.final_root).join(p).to_string_lossy().into_owned()).collect();
        Ok(())
    }
    fn collect_tree(root: &Path, path: &Path, found: &mut BTreeSet<String>) -> Checked<()> {
        let mut pending = vec![path.to_path_buf()];
        while let Some(dir) = pending.pop() {
            reject_link(&dir)?;
            for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                reject_link(&path)?;
                found.insert(path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/"));
                if found.len() > 10000 {
                    return Err("Staged inventory exceeds limit".into());
                }
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    pending.push(path);
                }
            }
        }
        Ok(())
    }
    fn reject_external_templates(d: &Drawing) -> Checked<()> {
        if d.blocks.values().any(|b| b.xref_path.is_some()) {
            return Err("Templates must not contain external drawing references".into());
        }
        Ok(())
    }
    fn make_sheet(d: &mut Drawing, p: &Profile, plan: &Plan, number: &str, title: &str) -> Checked<()> {
        if d.layout(&p.layout).is_none() {
            d.layouts.push(Layout::new(&p.layout, 1));
        }
        let paper_unit = if p.plot.units == "Millimeters" { 1.0 } else { cadcraft_render::paper::paper_unit_mm(d) };
        let profile_scale = if p.plot.units == "Inches" { 25.4 } else { 1.0 } / paper_unit;
        let layout = d.layouts.iter_mut().find(|l| l.name.eq_ignore_ascii_case(&p.layout)).ok_or("Layout missing")?;
        let layout_name = layout.name.clone();
        d.active_layout = Some(layout_name.clone());
        let (width, height) = if let Some(paper) = cadcraft_render::paper::paper_size(&p.plot.media) {
            (paper.width_mm, paper.height_mm)
        } else if layout.page.paper.replace('_', " ").eq_ignore_ascii_case(&p.plot.media.replace('_', " ")) {
            (layout.page.width_mm, layout.page.height_mm)
        } else {
            return Err("Custom media requires a matching page setup in the sheet template".into());
        };
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err("Invalid template paper dimensions".into());
        }
        layout.page.device = p.plot.device.clone();
        layout.page.paper = p.plot.media.clone();
        layout.page.plot_style_table = p.plot.style_sheet.clone();
        layout.page.scale = p.plot.scale;
        layout.page.scale_to_fit = false;
        layout.page.paper_mm = p.plot.units == "Millimeters";
        layout.page.landscape = p.orientation == "Landscape";
        layout.page.width_mm = width.min(height);
        layout.page.height_mm = width.max(height);
        layout.page.margins_mm = [0.0; 4];
        let rotation = i64::from(p.plot.rotation / 90);
        let wide = (rotation % 2 != 0) != layout.page.landscape;
        let (raw_width, raw_height) =
            if wide { (layout.page.height_mm, layout.page.width_mm) } else { (layout.page.width_mm, layout.page.height_mm) };
        layout.page.file_paper = Some((raw_width, raw_height, rotation));
        let space = Space::Paper(layout_name);
        let block = d.block(&p.title_block).cloned().ok_or("Title block definition is missing")?;
        let mut insert = Insert { block: p.title_block.clone(), ..Default::default() };
        let values = planner::input_values(&plan.inputs);
        for entity in block.entities.iter() {
            if let EntityKind::AttDef(def) = &entity.kind {
                if def.constant {
                    continue;
                }
                let mut a = def.clone();
                a.prompt.clear();
                a.modes = def.modes.for_reference();
                if let Some(mapping) = plan.manifest.mappings().iter().find(|m| m.title_block_attribute.eq_ignore_ascii_case(&a.tag)) {
                    a.text.value = values.get(&mapping.input).and_then(|v| v.clone()).unwrap_or_default();
                }
                if a.tag.eq_ignore_ascii_case("SHEET_NUMBER") {
                    a.text.value = number.into();
                }
                if a.tag.eq_ignore_ascii_case("SHEET_TITLE") {
                    a.text.value = title.into();
                }
                a.constant = false;
                insert.attribs.push(a);
            }
        }
        for mapping in plan.manifest.mappings() {
            if !insert.attribs.iter().any(|a| a.tag.eq_ignore_ascii_case(&mapping.title_block_attribute)) {
                return Err(format!("Missing title block attribute {}", mapping.title_block_attribute));
            }
        }
        let existing: Vec<_> = d
            .space(&space)
            .into_iter()
            .flat_map(|s| s.iter())
            .filter_map(|e| match &e.kind {
                EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(&p.title_block) => Some(e.handle),
                _ => None,
            })
            .collect();
        if existing.is_empty() {
            let matrix = insert.transform(block.base.xy());
            for a in &mut insert.attribs {
                let mut text = EntityKind::Text(a.text.clone());
                text.transform(&matrix);
                if let EntityKind::Text(t) = text {
                    a.text = t;
                }
                a.transform_multiline(&matrix);
            }
            d.add(&space, Default::default(), EntityKind::Insert(insert)).map_err(|e| e.to_string())?;
        } else {
            for handle in existing {
                d.modify_entity(handle, |e| {
                    if let EntityKind::Insert(i) = &mut e.kind {
                        let matrix = i.transform(block.base.xy());
                        for wanted in &insert.attribs {
                            if let Some(a) = i.attribs.iter_mut().find(|a| a.tag.eq_ignore_ascii_case(&wanted.tag)) {
                                a.text.value = wanted.text.value.clone();
                            } else {
                                let mut a = wanted.clone();
                                let mut text = EntityKind::Text(a.text.clone());
                                text.transform(&matrix);
                                if let EntityKind::Text(t) = text {
                                    a.text = t;
                                }
                                a.transform_multiline(&matrix);
                                i.attribs.push(a);
                            }
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            }
        }
        for placeholder in &p.placeholders {
            let mut r = placeholder.rectangle.clone();
            r.x *= profile_scale;
            r.y *= profile_scale;
            r.width *= profile_scale;
            r.height *= profile_scale;
            if placeholder.kind == "DisabledViewport" {
                let id = d
                    .layouts
                    .iter()
                    .flat_map(|l| l.entities.iter())
                    .filter_map(|e| match &e.kind {
                        EntityKind::Viewport(v) => Some(v.id),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(1)
                    .max(1)
                    .checked_add(1)
                    .ok_or("Viewport ID space exhausted")?;
                d.add(
                    &space,
                    Default::default(),
                    EntityKind::Viewport(Viewport {
                        center: Vec3::new(r.x + r.width / 2.0, r.y + r.height / 2.0, 0.0),
                        width: r.width,
                        height: r.height,
                        on: false,
                        id,
                        ..Default::default()
                    }),
                )
                .map_err(|e| e.to_string())?;
            } else {
                for (a, b) in [
                    (Vec2::new(r.x, r.y), Vec2::new(r.x + r.width, r.y)),
                    (Vec2::new(r.x + r.width, r.y), Vec2::new(r.x + r.width, r.y + r.height)),
                    (Vec2::new(r.x + r.width, r.y + r.height), Vec2::new(r.x, r.y + r.height)),
                    (Vec2::new(r.x, r.y + r.height), Vec2::new(r.x, r.y)),
                ] {
                    d.add(
                        &space,
                        Default::default(),
                        EntityKind::Line(cadcraft_doc::Line { a: Vec3::new(a.x, a.y, 0.0), b: Vec3::new(b.x, b.y, 0.0) }),
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            d.add(
                &space,
                Default::default(),
                EntityKind::Text(cadcraft_doc::Text {
                    insert: Vec3::new(r.x, r.y, 0.0),
                    height: 2.5 * profile_scale,
                    value: placeholder.label.clone(),
                    align_pt: None,
                    rotation: 0.0,
                    width_factor: 1.0,
                    oblique: 0.0,
                    style: "Standard".into(),
                    halign: Default::default(),
                    valign: Default::default(),
                }),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn verify_drawing(actual: &Drawing, wanted: &Drawing, plan: &Plan, relative: &str, stage: &Path) -> Checked<()> {
        if actual.model.len() != wanted.model.len() {
            return Err("Model geometry count changed during save".into());
        }
        for (a, b) in actual.model.iter().zip(wanted.model.iter()) {
            if !equivalent(&serde_json::to_value(&a.kind).map_err(|e| e.to_string())?, &serde_json::to_value(&b.kind).map_err(|e| e.to_string())?) {
                return Err("Saved model geometry differs".into());
            }
        }
        for action in &plan.actions {
            if action.relative_path != relative {
                continue;
            }
            match &action.operation {
                Operation::Sheet { profile, .. } => {
                    let l = actual.layout(&profile.layout).ok_or("Saved layout is missing")?;
                    let expected = wanted.layout(&profile.layout).ok_or("Expected layout missing")?;
                    let content = |layout: &Layout| {
                        layout
                            .entities
                            .iter()
                            .filter(|e| !matches!(&e.kind,EntityKind::Viewport(v) if v.id==1))
                            .map(|e| serde_json::to_value(&e.kind))
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .map_err(|e| e.to_string())
                    };
                    if !equivalent(&json!(content(l)?), &json!(content(expected)?)) {
                        return Err("Saved sheet geometry differs".into());
                    }
                    if l.page.device != profile.plot.device
                        || l.page.paper != profile.plot.media
                        || (l.page.scale - profile.plot.scale).abs() > 1e-8
                        || l.page.landscape != (profile.orientation == "Landscape")
                        || (l.page.width_mm - expected.page.width_mm).abs() > 1e-8
                        || (l.page.height_mm - expected.page.height_mm).abs() > 1e-8
                        || l.page.file_paper.map(|(_, _, r)| r) != Some(i64::from(profile.plot.rotation / 90))
                        || l.page.plot_style_table != profile.plot.style_sheet
                        || cadcraft_render::paper::page_unit_mm(actual, &l.page) != cadcraft_render::paper::page_unit_mm(wanted, &expected.page)
                    {
                        return Err("Saved plot settings differ".into());
                    }
                    let inserts: Vec<_> = l
                        .entities
                        .iter()
                        .filter_map(|e| match &e.kind {
                            EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(&profile.title_block) => Some(i),
                            _ => None,
                        })
                        .collect();
                    if inserts.is_empty() {
                        return Err("Saved title block is missing".into());
                    }
                    let values = planner::input_values(&plan.inputs);
                    for i in inserts {
                        for m in plan.manifest.mappings() {
                            let v = values.get(&m.input).and_then(|v| v.as_deref()).unwrap_or("");
                            if !i.attribs.iter().any(|a| a.tag.eq_ignore_ascii_case(&m.title_block_attribute) && a.text.value == v) {
                                return Err("Saved title attributes differ".into());
                            }
                        }
                    }
                    for p in &profile.placeholders {
                        if !l.entities.iter().any(|e| matches!(&e.kind,EntityKind::Text(t) if t.value==p.label)) {
                            return Err("Saved placeholder missing".into());
                        }
                    }
                }
                Operation::Xref { reference, relative_reference_path } => {
                    let b = actual.block(&reference.reference_role).ok_or("Saved xref block missing")?;
                    if b.xref_path.as_deref() != Some(relative_reference_path) || !b.xref_overlay {
                        return Err("Saved xref path or mode differs".into());
                    }
                    let parent = Path::new(relative).parent().ok_or("Xref host directory missing")?;
                    let resolved = stage.join(parent).join(relative_reference_path).canonicalize().map_err(|e| e.to_string())?;
                    if !resolved.starts_with(stage.canonicalize().map_err(|e| e.to_string())?) {
                        return Err("Xref escapes project".into());
                    }
                    if !actual.model.iter().any(|e|matches!(&e.kind,EntityKind::Insert(i) if i.block==reference.reference_role && i.insert==Vec3::new(reference.insertion.x,reference.insertion.y,reference.insertion.z) && (i.scale.x-reference.scale).abs()<1e-8 && (i.rotation-reference.rotation.to_radians()).abs()<1e-8)) {return Err("Saved xref transform differs".into());}
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn equivalent(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() <= 1e-8 * a.abs().max(b.abs()).max(1.0),
                _ => false,
            },
            (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b)),
            (Value::Object(a), Value::Object(b)) => a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b))),
            _ => a == b,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    mod sample {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../proposal/tests/fixture.rs"));
    }

    fn project() -> (std::path::PathBuf, serde_json::Value, super::super::file::IoHooks) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("cadcraft-proposal-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&root).unwrap();
        let mut manifest = sample::fixture();
        manifest["base_roots"][0]["path"] = json!(root.to_string_lossy());
        let path = root.join("standards.json");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut d = cadcraft_doc::Drawing::new_metric();
        let mut block = cadcraft_doc::Block::new("Title");
        block.entities.push(cadcraft_doc::Entity {
            handle: cadcraft_doc::Handle(900),
            common: Default::default(),
            kind: cadcraft_doc::EntityKind::AttDef(cadcraft_doc::Attrib {
                tag: "CLIENT".into(),
                text: cadcraft_doc::Text {
                    insert: cadcraft_geom::Vec3::ZERO,
                    height: 2.5,
                    value: "".into(),
                    rotation: 0.0,
                    width_factor: 1.0,
                    oblique: 0.0,
                    style: "Standard".into(),
                    halign: Default::default(),
                    valign: Default::default(),
                    align_pt: None,
                },
                invisible: false,
                constant: false,
                prompt: "Enter client number".into(),
                props: Default::default(),
                multiline: None,
                modes: Default::default(),
            }),
        });
        d.blocks.insert("Title".into(), std::sync::Arc::new(block));
        for name in ["model.dxf", "sheet.dxf"] {
            std::fs::write(root.join(name), cadcraft_io::write(&d, name).unwrap()).unwrap();
        }
        let hooks = super::super::file::IoHooks {
            read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
            write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
            plot: None,
        };
        let params = json!({"action":"plan","manifest_path":path,"inputs":{"client_name":"Client","site_name":"Site","proposal_year":2026,"orientation":"Landscape","sheet_size":"A3","client_number":"C123"}});
        (root, params, hooks)
    }

    #[test]
    fn plan_create_reopen_refuse_and_stale() {
        let (root, mut params, hooks) = project();
        let preview = native::run(&params, &hooks).unwrap();
        assert!(!root.join("Client - Site").exists());
        params["action"] = json!("create");
        params["approved_hash"] = preview["hash"].clone();
        let result = native::run(&params, &hooks).unwrap();
        assert_eq!(result["receipt"]["outcome"], "Succeeded", "{result}");
        for item in result["receipt"]["verified_artifacts"].as_array().unwrap() {
            let file = item.as_str().unwrap();
            if file.ends_with(".dxf") {
                cadcraft_io::read(&std::fs::read(file).unwrap(), file).unwrap();
            }
        }
        assert_eq!(native::run(&params, &hooks).unwrap()["receipt"]["outcome"], "Refused");
        params["inputs"]["site_name"] = json!("Other");
        let stale = native::run(&params, &hooks).unwrap();
        assert_eq!(stale["receipt"]["failed_checks"][0]["code"], "stale_approval");
        assert!(!root.join("Client - Other").exists());
        params["action"] = json!("plan");
        let preview = native::run(&params, &hooks).unwrap();
        params["action"] = json!("create");
        params["approved_hash"] = preview["hash"].clone();
        let before = std::fs::read_dir(&root).unwrap().count();
        let mut cancelled = params.clone();
        cancelled.as_object_mut().unwrap().remove("approved_hash");
        assert_eq!(native::run(&cancelled, &hooks).unwrap()["receipt"]["outcome"], "Cancelled");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), before);
        let failing = super::super::file::IoHooks { read: hooks.read, write: |_, _| Err("injected write failure".into()), plot: None };
        let failed = native::run(&params, &failing).unwrap();
        assert_eq!(failed["receipt"]["outcome"], "Failed");
        assert!(!root.join("Client - Other").exists());
        assert!(!std::fs::read_dir(&root).unwrap().any(|e| e.unwrap().path().extension().is_some_and(|e| e == "stage")));
        let failed_path = failed["receipt"]["receipt_path"].as_str().unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(failed_path).unwrap()).unwrap();
        assert_eq!(saved["outcome"], "Failed");
        let corrupt = super::super::file::IoHooks {
            read: hooks.read,
            write: |_, n| cadcraft_io::write(&cadcraft_doc::Drawing::new_metric(), n).map_err(|e| e.to_string()),
            plot: None,
        };
        let failed = native::run(&params, &corrupt).unwrap();
        assert_eq!(failed["receipt"]["outcome"], "Failed");
        assert_eq!(failed["receipt"]["failed_checks"][0]["code"], "verification_failed");
        assert!(!root.join("Client - Other").exists());
        std::fs::write(root.join("model.dxf"), b"changed").unwrap();
        let stale = native::run(&params, &hooks).unwrap();
        assert_eq!(stale["receipt"]["failed_checks"][0]["code"], "stale_approval");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staged_paths_refuse_escape() {
        let stage = std::env::temp_dir().join("cadcraft-proposal-child-test");
        assert!(native::child(&stage, "../escape").is_err());
        assert!(native::child(&stage, "/escape").is_err());
        assert!(native::child(&stage, "nested/artifact.json").is_ok());
    }

    #[test]
    fn existing_layout_name_matching_is_consistent() {
        let (root, mut params, hooks) = project();
        let path = root.join("standards.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["profiles"][0]["layout"] = json!("PROPOSAL");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut template = cadcraft_io::read(&std::fs::read(root.join("sheet.dxf")).unwrap(), "sheet.dxf").unwrap();
        template.layouts.push(cadcraft_doc::Layout::new("Proposal", 1));
        std::fs::write(root.join("sheet.dxf"), cadcraft_io::write(&template, "sheet.dxf").unwrap()).unwrap();
        let preview = native::run(&params, &hooks).unwrap();
        params["action"] = json!("create");
        params["approved_hash"] = preview["hash"].clone();
        let result = native::run(&params, &hooks).unwrap();
        assert_eq!(result["receipt"]["outcome"], "Succeeded", "{result}");
        let sheet = root.join("Client - Site/Sheets/0.dxf");
        let d = cadcraft_io::read(&std::fs::read(&sheet).unwrap(), &sheet).unwrap();
        assert_eq!(d.layouts.iter().filter(|l| l.name.eq_ignore_ascii_case("proposal")).count(), 1);
        assert!(
            d.space(&cadcraft_doc::Space::Paper("Proposal".into()))
                .unwrap()
                .iter()
                .any(|e| matches!(&e.kind,cadcraft_doc::EntityKind::Insert(i) if i.block=="Title"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plot_orientation_dimensions_rotation_and_units_survive_reopen() {
        for (orientation, units, rotation) in
            [("Landscape", "Millimeters", 0), ("Portrait", "Millimeters", 90), ("Landscape", "Inches", 180), ("Portrait", "Inches", 270)]
        {
            let (root, mut params, hooks) = project();
            let path = root.join("standards.json");
            let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let profile = &mut manifest["profiles"][0];
            profile["orientation"] = json!(orientation);
            profile["plot"]["units"] = json!(units);
            profile["plot"]["rotation"] = json!(rotation);
            profile["plot"]["scale"] = json!(2.5);
            profile["plot"]["style_sheet"] = json!("monochrome.ctb");
            let (width, height) = if units == "Inches" {
                (1, 1)
            } else if orientation == "Landscape" {
                (420, 297)
            } else {
                (297, 420)
            };
            profile["printable_area"]["width"] = json!(width);
            profile["printable_area"]["height"] = json!(height);
            if units == "Inches" {
                for p in profile["placeholders"].as_array_mut().unwrap() {
                    p["rectangle"]["width"] = json!(1);
                    p["rectangle"]["height"] = json!(1);
                }
            }
            std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            params["inputs"]["orientation"] = json!(orientation);
            let preview = native::run(&params, &hooks).unwrap();
            params["action"] = json!("create");
            params["approved_hash"] = preview["hash"].clone();
            let result = native::run(&params, &hooks).unwrap();
            assert_eq!(result["receipt"]["outcome"], "Succeeded", "{result}");
            let path = root.join("Client - Site/Sheets/0.dxf");
            let d = cadcraft_io::read(&std::fs::read(&path).unwrap(), &path).unwrap();
            assert_eq!(d.header.i64("MEASUREMENT", 0), 1, "keep metric template measurement");
            let p = &d.layout("Proposal").unwrap().page;
            assert_eq!(p.landscape, orientation == "Landscape");
            assert!((p.width_mm - 297.0).abs() < 1e-8);
            assert!((p.height_mm - 420.0).abs() < 1e-8);
            assert_eq!(p.file_paper.unwrap().2, rotation / 90);
            assert_eq!(p.plot_style_table, "monochrome.ctb");
            assert_eq!(cadcraft_render::paper::page_unit_mm(&d, p), 1.0);
            if units == "Inches" {
                assert!(d.layout("Proposal").unwrap().entities.iter().any(|e| {
                    matches!(&e.kind, cadcraft_doc::EntityKind::Line(line) if ((line.b.x - line.a.x).hypot(line.b.y - line.a.y) - 25.4).abs() < 1e-8)
                }));
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn multiple_placeholders_preserve_unique_existing_viewport_ids() {
        let (root, mut params, hooks) = project();
        let manifest_path = root.join("standards.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        for p in manifest["profiles"][0]["placeholders"].as_array_mut().unwrap() {
            p["kind"] = json!("DisabledViewport");
        }
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut d = cadcraft_io::read(&std::fs::read(root.join("sheet.dxf")).unwrap(), "sheet.dxf").unwrap();
        d.layouts.push(cadcraft_doc::Layout::new("Proposal", 1));
        d.add(
            &cadcraft_doc::Space::Paper("Proposal".into()),
            Default::default(),
            cadcraft_doc::EntityKind::Viewport(cadcraft_doc::Viewport { id: 2, ..Default::default() }),
        )
        .unwrap();
        std::fs::write(root.join("sheet.dxf"), cadcraft_io::write(&d, "sheet.dxf").unwrap()).unwrap();
        let preview = native::run(&params, &hooks).unwrap();
        params["action"] = json!("create");
        params["approved_hash"] = preview["hash"].clone();
        let result = native::run(&params, &hooks).unwrap();
        assert_eq!(result["receipt"]["outcome"], "Succeeded", "{result}");
        let sheet = root.join("Client - Site/Sheets/0.dxf");
        let d = cadcraft_io::read(&std::fs::read(&sheet).unwrap(), &sheet).unwrap();
        let ids: Vec<_> = d
            .layout("Proposal")
            .unwrap()
            .entities
            .iter()
            .filter_map(|e| match &e.kind {
                cadcraft_doc::EntityKind::Viewport(v) if v.id > 1 => Some(v.id),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.iter().copied().collect::<std::collections::BTreeSet<_>>().len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn permits_admin_redirects_above_the_project_and_source_links() {
        let (root, mut params, hooks) = project();
        let alias = root.join("redirected");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        std::os::unix::fs::symlink(root.join("model.dxf"), root.join("model-link.dxf")).unwrap();
        let path = root.join("standards.json");
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["base_roots"][0]["path"] = json!(alias);
        manifest["model_template"] = json!("model-link.dxf");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let preview = native::run(&params, &hooks).unwrap();
        params["action"] = json!("create");
        params["approved_hash"] = preview["hash"].clone();
        assert_eq!(native::run(&params, &hooks).unwrap()["receipt"]["outcome"], "Succeeded");
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_inside_the_reserved_stage() {
        let (root, _, _) = project();
        let stage = root.join("stage");
        std::fs::create_dir(&stage).unwrap();
        std::os::unix::fs::symlink(&root, stage.join("linked")).unwrap();
        assert!(native::child(&stage, "linked/escape.json").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
