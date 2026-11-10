//! Preserve supplied files, register their hashes, and attach only drawings and images.
use super::{CommandSpec, bad};
use crate::{Result, Session};
use serde_json::Value;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("intakesource", "Intake Source Files", run)
            .menu(&["File", "Intake Source Files"])
            .params("{action:plan|create,proposal_root,manifest_path,sources:[{path,role}],approved_hash?}")
            .noundo()
            .interactive(|_| Ok(super::proposal::follow_prompt("intakesource"))),
    ]
}
fn run(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("intakesource", "Project filesystem editing is not available on web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        run_with_hooks(p, super::file::io().ok_or_else(|| bad("intakesource", "Drawing formats are not available"))?)
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_with_hooks(p: &Value, h: &super::file::IoHooks) -> Result<Value> {
    run_with_move(p, h, &super::proposal::files::rename_no_replace)
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_with_move(p: &Value, h: &super::file::IoHooks, mover: &super::proposal::files::Mover<'_>) -> Result<Value> {
    Ok(native::run(p, h, mover).unwrap_or_else(super::proposal::files::issue))
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::super::proposal::files::{self, Checked, Mover, Project};
    use super::{Session, Value};
    use cadcraft_doc::{Block, Drawing, EntityKind, Image, Insert, Space};
    use cadcraft_geom::Vec3;
    use cadcraft_proposal::{manifest::SOURCE_ROLES, paths, planner::relative_reference};
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    };

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Row {
        role: String,
        filename: String,
        relative_path: String,
        sha256: String,
        action: String,
        reference_name: Option<String>,
        relative_reference_path: Option<String>,
        image: Option<Image>,
        sizing: Option<String>,
        insertion: [f64; 3],
        scale: f64,
        rotation: f64,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Register {
        version: u32,
        entries: Vec<Row>,
    }
    #[derive(Serialize)]
    struct Source {
        source: PathBuf,
        row: Row,
    }
    #[derive(Serialize)]
    struct Plan {
        version: u32,
        command: &'static str,
        proposal_root: PathBuf,
        target: String,
        fingerprints: std::collections::BTreeMap<String, String>,
        sources: Vec<Source>,
        actions: Vec<Value>,
    }
    fn image(path: &Path, name: &str, relative: &str, h: &super::super::file::IoHooks, host: &Path) -> Checked<(Image, String)> {
        let mut session = Session::new();
        let (image, sizing) = match super::super::image::run_imageattach(&mut session, &json!({"path":path,"at":[0,0],"name":name})) {
            Ok(_) => {
                let mut image = session
                    .doc()
                    .map_err(|e| e.to_string())?
                    .model
                    .iter()
                    .find_map(|e| match &e.kind {
                        EntityKind::Image(i) => Some(i.clone()),
                        _ => None,
                    })
                    .ok_or("verification_failed: image attachment missing")?;
                image.path = relative.into();
                image.name = name.into();
                (image, "imageattach_default".to_string())
            }
            // The existing decoder handles bounds and panics; unknown images remain explicit frames.
            Err(_) => (
                Image { path: relative.into(), name: name.into(), ..Image::default() },
                "unit_square_fallback: imageattach could not decode this source".to_string(),
            ),
        };
        // Let the host format materialize its default image clip before hashing the plan.
        let mut drawing = Drawing::new_metric();
        drawing.add(&Space::Model, Default::default(), EntityKind::Image(image)).map_err(|e| e.to_string())?;
        let reopened = (h.read)(&(h.write)(&drawing, &host.to_string_lossy())?, &host.to_string_lossy())?;
        let image = reopened
            .model
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::Image(i) => Some(i.clone()),
                _ => None,
            })
            .ok_or("lossy_format: image cannot round-trip in host format")?;
        Ok((image, sizing))
    }
    fn apply(d: &mut Drawing, sources: &[Source]) -> Checked<()> {
        for source in sources {
            let row = &source.row;
            match row.action.as_str() {
                "overlay_xref" => {
                    let name = row.reference_name.as_deref().ok_or("invalid_input: reference name missing")?;
                    let mut block = Block::new(name);
                    block.xref_path = row.relative_reference_path.clone();
                    block.xref_overlay = true;
                    d.blocks.insert(name.into(), Arc::new(block));
                    d.add(
                        &Space::Model,
                        Default::default(),
                        EntityKind::Insert(Insert { block: name.into(), insert: Vec3::ZERO, ..Insert::default() }),
                    )
                    .map_err(|e| e.to_string())?;
                }
                "image" => {
                    d.add(&Space::Model, Default::default(), EntityKind::Image(row.image.clone().ok_or("invalid_input: image missing")?))
                        .map_err(|e| e.to_string())?;
                }
                "copy_only" => {}
                _ => return Err("invalid_input: unsupported source action".into()),
            }
        }
        Ok(())
    }
    fn verify(d: &Drawing, sources: &[Source], prior: &Drawing) -> Checked<()> {
        let mut wanted = prior.clone();
        apply(&mut wanted, sources)?;
        if !files::same(&files::entities(&wanted)?, &files::entities(d)?) {
            return Err("verification_failed: drawing content differs from intake plan".into());
        }
        for source in sources {
            let row = &source.row;
            if row.action == "overlay_xref" {
                let name = row.reference_name.as_deref().ok_or("verification_failed: missing reference name")?;
                let block = d.block(name).ok_or("verification_failed: missing xref")?;
                if block.xref_path != row.relative_reference_path || !block.xref_overlay {
                    return Err("verification_failed: xref path or mode differs".into());
                }
            }
        }
        Ok(())
    }
    pub fn run(p: &Value, h: &super::super::file::IoHooks, mover: &Mover<'_>) -> Checked<Value> {
        let action = p.get("action").and_then(Value::as_str).unwrap_or("plan");
        if !matches!(action, "plan" | "create") {
            return Err("invalid_input: action must be plan or create".into());
        }
        let project = Project::open(p)?;
        let model = project.manifest.models().iter().find(|m| m.role == "BaseModel").ok_or("invalid_project: BaseModel missing")?;
        let target = files::child(&project.root, &model.path)?;
        let mut fingerprints = project.fingerprints.clone();
        let (prior, target_hash) = files::drawing(h, &target)?;
        fingerprints.insert("target".into(), target_hash.clone());
        let support =
            project.manifest.support_records().iter().find(|r| r.role == "SourceRegister").ok_or("invalid_project: SourceRegister missing")?;
        let register_path = files::child(&project.root, &support.path)?;
        let register_bytes = files::read(&register_path, 2 << 20)?;
        fingerprints.insert("register".into(), cadcraft_proposal::approval::sha256(&register_bytes));
        let mut register: Register = serde_json::from_slice(&register_bytes).map_err(|e| format!("invalid_register: {e}"))?;
        if register.version != 1 || register.entries.len() > 1000 {
            return Err("invalid_register: unsupported or oversized source register".into());
        }
        let mut destinations: BTreeSet<_> = register.entries.iter().map(|r| paths::case_fold(&r.relative_path)).collect();
        if destinations.len() != register.entries.len() {
            return Err("invalid_register: duplicate source destinations".into());
        }
        for row in &register.entries {
            if !SOURCE_ROLES.contains(&row.role.as_str())
                || row.sha256.len() != 64
                || !row.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || files::fingerprint(&files::child(&project.root, &row.relative_path)?)? != row.sha256
            {
                return Err("invalid_register: existing source differs from its record".into());
            }
        }
        let mut names: BTreeSet<_> = prior.blocks.keys().map(|s| paths::case_fold(s)).collect();
        for store in std::iter::once(&prior.model).chain(prior.layouts.iter().map(|l| &l.entities)).chain(prior.blocks.values().map(|b| &b.entities))
        {
            for entity in store.iter() {
                if let EntityKind::Image(i) = &entity.kind {
                    names.insert(paths::case_fold(&i.name));
                }
            }
        }
        let host_name = target.file_stem().and_then(|s| s.to_str()).ok_or("unsafe_path: host name missing")?;
        let inputs = p.get("sources").and_then(Value::as_array).ok_or("invalid_input: sources array required")?;
        if inputs.is_empty() || inputs.len() > 128 || register.entries.len().saturating_add(inputs.len()) > 1000 {
            return Err("invalid_input: expected 1..128 sources and at most 1000 registered files".into());
        }
        let mut sources = Vec::new();
        let mut total = 0u64;
        let mut originals = BTreeSet::new();
        for input in inputs {
            if input.as_object().is_none_or(|m| m.len() != 2 || !m.contains_key("path") || !m.contains_key("role")) {
                return Err("invalid_input: each source has only path and role".into());
            }
            let source = PathBuf::from(input.get("path").and_then(Value::as_str).ok_or("invalid_input: source path required")?);
            for ancestor in source.ancestors() {
                files::reject_link(ancestor)?;
            }
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            if !originals.insert(source.clone()) {
                return Err("output_collision: duplicate source".into());
            }
            let role = input.get("role").and_then(Value::as_str).filter(|r| SOURCE_ROLES.contains(r)).ok_or("invalid_input: unknown source role")?;
            let filename = source.file_name().and_then(|s| s.to_str()).ok_or("unsafe_path: source filename is not Unicode")?.to_string();
            paths::component(&filename).map_err(|e| format!("{}: {}", e.code, e.message))?;
            let relative_path = format!("{}/{role}/{filename}", project.manifest.source_location());
            files::vacant(&project.root, &relative_path)?;
            if !destinations.insert(paths::case_fold(&relative_path)) {
                return Err("output_collision: duplicate destination".into());
            }
            let ext = source.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            let kind = match ext.as_str() {
                "dwg" | "dxf" => "overlay_xref",
                "png" | "jpg" | "jpeg" | "tif" | "tiff" => "image",
                _ => "copy_only",
            };
            let reference = source.file_stem().and_then(|s| s.to_str()).ok_or("unsafe_path: source stem missing")?.to_string();
            paths::component(&reference).map_err(|e| format!("{}: {}", e.code, e.message))?;
            if kind != "copy_only" && (reference.eq_ignore_ascii_case(host_name) || !names.insert(paths::case_fold(&reference))) {
                return Err("reference_name_collision: reference name conflicts with host or existing reference".into());
            }
            total = total.saturating_add(fs::metadata(&source).map_err(|e| e.to_string())?.len());
            if total > 2 << 30 {
                return Err("source_limit: combined sources exceed 2 GiB".into());
            }
            let sha256 = files::fingerprint(&source)?;
            let relative = relative_reference(&model.path, &relative_path);
            let (image, sizing) = if kind == "image" {
                let (i, s) = image(&source, &reference, &relative, h, &target)?;
                (Some(i), Some(s))
            } else {
                (None, None)
            };
            if files::fingerprint(&source)? != sha256 {
                return Err("source_changed: source changed during planning".into());
            }
            sources.push(Source {
                source,
                row: Row {
                    role: role.into(),
                    filename,
                    relative_path,
                    sha256,
                    action: kind.into(),
                    reference_name: (kind != "copy_only").then_some(reference),
                    relative_reference_path: (kind != "copy_only").then_some(relative),
                    image,
                    sizing,
                    insertion: [0.0; 3],
                    scale: 1.0,
                    rotation: 0.0,
                },
            });
        }
        sources.sort_by(|a, b| a.row.relative_path.cmp(&b.row.relative_path));
        let mut actions = Vec::new();
        for source in &sources {
            actions.push(json!({"kind":"copy_source","path":source.row.relative_path,"sha256":source.row.sha256}));
            if source.row.action != "copy_only" {
                actions.push(json!({"kind":source.row.action,"host":model.path,"source":source.row}));
            }
            actions.push(json!({"kind":"register_source","path":support.path,"row":source.row}));
        }
        let plan = Plan {
            version: 1,
            command: "intakesource",
            proposal_root: project.root.clone(),
            target: model.path.clone(),
            fingerprints,
            sources,
            actions,
        };
        let hash = files::hash(&plan)?;
        if action == "plan" {
            return Ok(json!({"plan":plan,"hash":hash,"issues":[]}));
        }
        if let Some(failure) = files::approval_failure(&hash, p.get("approved_hash").and_then(Value::as_str)) {
            return Ok(failure);
        }
        let _lock = files::lock(&project.root)?;
        let id = files::run_id();
        let stage = files::stage_path(&target, "intakesource", &id)?;
        let register_stage = files::stage_path(&register_path, "intakesource", &id)?;
        let copies_stage = project.root.join(format!(".intakesource-{id}.stage"));
        let receipt_path = files::receipt_path(&project, "intakesource", &id)?;
        let mut copied = Vec::<PathBuf>::new();
        let mut created = Vec::<PathBuf>::new();
        let mut register_swap = None;
        let mut host_swap = None;
        let mut owned_stage = false;
        let mut owned_register_stage = false;
        let mut owned_copies_stage = false;
        let result = (|| -> Checked<Value> {
            files::reject_link(&copies_stage)?;
            fs::create_dir(&copies_stage).map_err(|e| e.to_string())?;
            owned_copies_stage = true;
            for (index, source) in plan.sources.iter().enumerate() {
                files::reject_link(&source.source)?;
                let staged = copies_stage.join(index.to_string());
                let mut dest = fs::OpenOptions::new().create_new(true).write(true).open(&staged).map_err(|e| e.to_string())?;
                let mut input = fs::File::open(&source.source).map_err(|e| e.to_string())?;
                let size = input.metadata().map_err(|e| e.to_string())?.len();
                if size > 2 << 30 {
                    return Err("source_limit: file exceeds 2 GiB".into());
                }
                use std::io::Read;
                let n = std::io::copy(&mut input.by_ref().take(size.saturating_add(1)), &mut dest).map_err(|e| e.to_string())?;
                dest.sync_all().map_err(|e| e.to_string())?;
                drop(dest);
                if n != size || files::fingerprint(&staged)? != source.row.sha256 || files::fingerprint(&source.source)? != source.row.sha256 {
                    return Err("source_changed: copied bytes differ from approved source".into());
                }
            }
            let mut wanted = prior.clone();
            apply(&mut wanted, &plan.sources)?;
            files::write_new(&stage, &(h.write)(&wanted, &target.to_string_lossy())?)?;
            owned_stage = true;
            let actual = (h.read)(&files::read(&stage, 32 << 20)?, &target.to_string_lossy())?;
            verify(&actual, &plan.sources, &prior)?;
            register.entries.extend(plan.sources.iter().map(|s| s.row.clone()));
            files::write_new(&register_stage, &files::encoded(&register)?)?;
            owned_register_stage = true;
            let check: Register = serde_json::from_slice(&files::read(&register_stage, 2 << 20)?).map_err(|e| e.to_string())?;
            if files::hash(&check)? != files::hash(&register)? {
                return Err("verification_failed: source register differs".into());
            }
            // Copies and register publish first; the host is the final drawing mutation.
            project.check(p)?;
            for (index, source) in plan.sources.iter().enumerate() {
                let destination = files::vacant(&project.root, &source.row.relative_path)?;
                files::ensure_parent(&project.root, &destination, &mut created)?;
                mover(&copies_stage.join(index.to_string()), &destination)?;
                copied.push(destination);
            }
            if files::fingerprint(&target)? != target_hash {
                return Err("stale_approval: host changed before publication".into());
            }
            register_swap = Some(files::swap(&register_path, &register_stage, &cadcraft_proposal::approval::sha256(&register_bytes), mover)?);
            host_swap = Some(files::swap(&target, &stage, &target_hash, mover)?);
            verify(&(h.read)(&files::read(&target, 32 << 20)?, &target.to_string_lossy())?, &plan.sources, &prior)?;
            let receipt = json!({"version":1,"command":"intakesource","outcome":"Succeeded","plan_sha256":hash,"drawing":host_swap,"register":register_swap,"sources":plan.sources.iter().map(|s|&s.row).collect::<Vec<_>>(),"receipt_path":receipt_path});
            files::write_new(&receipt_path, &files::encoded(&receipt)?)?;
            Ok(receipt)
        })();
        let mut cleanup = Vec::new();
        if result.is_err() {
            if let Some(record) = &host_swap
                && let Err(e) = files::restore(&target, record, mover)
            {
                cleanup.push(e);
            }
            if let Some(record) = &register_swap
                && let Err(e) = files::restore(&register_path, record, mover)
            {
                cleanup.push(e);
            }
            for path in copied.iter().rev() {
                if let Err(e) = fs::remove_file(path) {
                    cleanup.push(e.to_string());
                }
            }
        }
        for (path, owned) in [(&stage, owned_stage), (&register_stage, owned_register_stage)] {
            if owned
                && let Err(e) = fs::remove_file(path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                cleanup.push(e.to_string());
            }
        }
        if owned_copies_stage && let Err(e) = fs::remove_dir_all(&copies_stage) {
            cleanup.push(e.to_string());
        }
        if result.is_err() {
            for dir in created.iter().rev() {
                if let Err(e) = fs::remove_dir(dir) {
                    cleanup.push(e.to_string());
                }
            }
        }
        Ok(match result {
            Ok(receipt) => json!({"receipt":receipt,"cleanup_failures":cleanup}),
            Err(error) => {
                json!({"receipt":{"outcome":"Failed","plan_sha256":hash,"failed_checks":[{"code":"execution_failed","message":error}],"cleanup_failures":cleanup}})
            }
        })
    }
}
