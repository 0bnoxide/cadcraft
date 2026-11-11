//! Verified, approved proposal deliverables; drawings only ever travel as unchanged bytes.
use super::CommandSpec;
#[cfg(target_arch = "wasm32")]
use super::bad;
use crate::{Result, Session};
use serde_json::Value;
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("packagedeliverables", "Package Deliverables", run)
            .menu(&["File", "Package Deliverables"])
            .params("{action:plan|preview|execute|create (alias for execute),proposal_root,manifest_path,profile_path,approved_hash?}")
            .noundo()
            .interactive(|_| Ok(super::figureexport::prompt("packagedeliverables"))),
    ]
}
fn run(_: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("packagedeliverables", "Filesystem deliverable packaging is not available on web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(run_with_move(p, &super::proposal::files::rename_no_replace))
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_with_move(p: &Value, mover: &super::proposal::files::Mover<'_>) -> Value {
    native::run(p, mover).unwrap_or_else(super::proposal::files::issue)
}
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) fn run_with_hooks(p: &Value, h: &super::file::IoHooks, mover: &super::proposal::files::Mover<'_>) -> Value {
    native::run_with_hooks(p, h, mover).unwrap_or_else(super::proposal::files::issue)
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::super::{
        figureexport::native as outgoing,
        file,
        proposal::files::{self, Checked, Mover, Project, no_named_streams},
    };
    use cadcraft_proposal::{
        approval,
        deliverables::DeliverableProfile,
        manifest::{SHEET_ROLES, Sheet, text},
        paths,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        io::Read,
        path::{Path, PathBuf},
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        action: String,
        proposal_root: String,
        manifest_path: String,
        profile_path: String,
        approved_hash: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Index {
        version: u32,
        sheets: Vec<Sheet>,
    }
    #[derive(Clone, Serialize)]
    struct Entry {
        path: String,
        role: String,
        sheet_role: Option<String>,
        source: PathBuf,
        layout: Option<String>,
        plot_options: Option<Value>,
    }
    struct Built {
        plan: Value,
        project: Project,
        refs: outgoing::References,
        entries: Vec<Entry>,
    }
    fn insert(entries: &mut BTreeMap<String, Entry>, entry: Entry, shared: bool) -> Checked<()> {
        paths::relative(&entry.path).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let key = paths::case_fold(&entry.path);
        if let Some(prior) = entries.get(&key) {
            if shared && prior.source == entry.source && prior.path == entry.path {
                return Ok(());
            }
            return Err("output_collision: two files have the same destination".into());
        }
        entries.insert(key, entry);
        Ok(())
    }
    fn pair(root: &Path) -> Checked<(PathBuf, PathBuf)> {
        let names = outgoing::existing(root)?;
        for n in 1..=10_000 {
            let stem = if n == 1 { "Package".into() } else { format!("Package_v{n}") };
            if !names.contains(&paths::case_fold(&stem)) && !names.contains(&paths::case_fold(&format!("{stem}.zip"))) {
                return Ok((root.join(&stem), root.join(format!("{stem}.zip"))));
            }
        }
        Err("output_limit: no available package version".into())
    }
    fn index(project: &Project) -> Checked<Vec<Sheet>> {
        let path = files::child(&project.root, project.manifest.sheet_index_path())?;
        let index: Index = serde_json::from_slice(&files::read(&path, 2 << 20)?).map_err(|e| format!("invalid_sheet_index: {e}"))?;
        if index.version != 1 || index.sheets.len() > 1000 {
            return Err("invalid_sheet_index: unsupported version or too many sheets".into());
        }
        let mut roles = BTreeSet::new();
        let mut names = BTreeSet::new();
        for sheet in &index.sheets {
            paths::relative(&sheet.path).map_err(|e| format!("{}: {}", e.code, e.message))?;
            if !SHEET_ROLES.contains(&sheet.role.as_str())
                || !roles.insert(&sheet.role)
                || !names.insert(paths::case_fold(&sheet.path))
                || !text(&sheet.number)
                || !text(&sheet.title)
            {
                return Err("invalid_sheet_index: invalid or duplicate sheet".into());
            }
        }
        Ok(index.sheets)
    }
    fn layout(project: &Project) -> Checked<String> {
        let config =
            project.manifest.support_records().iter().find(|r| r.role == "ProjectConfiguration").ok_or("invalid_project: configuration missing")?;
        let config: Value = serde_json::from_slice(&files::read(&files::child(&project.root, &config.path)?, 2 << 20)?).map_err(|e| e.to_string())?;
        let inputs = config.get("inputs").ok_or("invalid_project: creation inputs missing")?;
        let orientation = inputs.get("orientation").and_then(Value::as_str).ok_or("invalid_project: orientation missing")?;
        let size = inputs.get("sheet_size").and_then(Value::as_str).ok_or("invalid_project: sheet size missing")?;
        project.manifest.profile(orientation, size).map(|p| p.layout.clone()).ok_or("invalid_project: selected sheet profile missing".into())
    }
    fn build(p: &Value, r: &Request, h: &file::IoHooks) -> Checked<Built> {
        let project = Project::open(p)?;
        let profile_path = PathBuf::from(&r.profile_path);
        outgoing::safe_source(&profile_path)?;
        let profile_path = profile_path.canonicalize().map_err(|e| e.to_string())?;
        let mut refs = outgoing::References::new();
        refs.add(&profile_path)?;
        refs.add(Path::new(&r.manifest_path))?;
        for record in project
            .manifest
            .support_records()
            .iter()
            .filter(|r| ["ProjectConfiguration", "CreationPlan", "CreationReceipt"].contains(&r.role.as_str()))
        {
            refs.add(&files::child(&project.root, &record.path)?)?;
        }
        refs.add(&files::child(&project.root, project.manifest.sheet_index_path())?)?;
        let profile = DeliverableProfile::parse(&files::read(&profile_path, 1 << 20)?).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let selected = profile.select(&index(&project)?).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let layout = layout(&project)?;
        let mut entries = BTreeMap::new();
        let mut sheets = Vec::new();
        for output in selected {
            let source = files::child(&project.root, &output.sheet.path)?;
            outgoing::safe_source(&source)?;
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            refs.add(&source)?;
            let d = outgoing::read_drawing(&source, h)?;
            let mut closure = outgoing::References::new();
            closure.add(&source)?;
            let mut qa = Vec::new();
            closure.walk(&d, &source, h, 0, &mut BTreeSet::from([source.clone()]), &mut qa)?;
            let session = outgoing::plot_session(d, &source, h, &mut qa)?;
            let d = session.doc().map_err(|e| e.to_string())?;
            let page = &d.layout(&layout).ok_or("layout_missing: selected sheet layout does not exist")?.page;
            if !page.width_mm.is_finite()
                || !page.height_mm.is_finite()
                || page.width_mm <= 0.
                || page.height_mm <= 0.
                || page.width_mm > cadcraft_doc::MAX_PAPER_MM
                || page.height_mm > cadcraft_doc::MAX_PAPER_MM
                || !page.scale.is_finite()
                || page.scale <= 0.
            {
                return Err("invalid_page_setup: selected sheet has invalid dimensions or scale".into());
            }
            if qa.iter().any(|q| q.severity == cadcraft_proposal::figures::Severity::Error) {
                return Err("broken_reference: selected sheet has unresolved references".into());
            }
            let options = super::super::plotters::plot_options(&session, &json!({"layout":layout,"output":"pdf","device":page.device}), "exportpdf")
                .map_err(|e| format!("invalid_page_setup: {e}"))?;
            sheets.push(json!({"sheet":output.sheet,"pdf":output.pdf,"layout":layout,"page_setup":page,"qa":qa,"plot_options":options}));
            insert(
                &mut entries,
                Entry {
                    path: output.pdf,
                    role: "sheet_pdf".into(),
                    sheet_role: Some(output.sheet.role),
                    source: source.clone(),
                    layout: Some(layout.clone()),
                    plot_options: Some(options),
                },
                false,
            )?;
            closure.check()?;
            for (path, expected) in &closure.fingerprints {
                refs.add(path)?;
                if refs.fingerprints.get(path) != Some(expected) {
                    return Err("stale_approval: reference changed during preflight".into());
                }
                if profile.include_drawings {
                    let relative =
                        path.strip_prefix(&project.root).map_err(|_| "nonportable_reference: traveling references must stay inside the proposal")?;
                    let dest = format!("drawings/{}", relative.to_string_lossy().replace('\\', "/"));
                    insert(
                        &mut entries,
                        Entry { path: dest, role: "reference_file".into(), sheet_role: None, source: path.clone(), layout: None, plot_options: None },
                        true,
                    )?;
                }
            }
        }
        for extra in &profile.extra_files {
            let source = profile_path.parent().ok_or("unsafe_path: profile parent missing")?.join(&extra.path);
            outgoing::safe_source(&source)?;
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            no_named_streams(&source)?;
            refs.add(&source)?;
            let name = source.file_name().and_then(|n| n.to_str()).ok_or("unsafe_path: extra filename is not Unicode")?;
            let dest = extra.destination(name).map_err(|e| format!("{}: {}", e.code, e.message))?;
            if dest == "checksums.json" {
                return Err("output_collision: checksum manifest destination is reserved".into());
            }
            let role = serde_json::to_value(&extra.role).map_err(|e| e.to_string())?.as_str().unwrap_or_default().to_string();
            insert(&mut entries, Entry { path: dest, role, sheet_role: None, source, layout: None, plot_options: None }, false)?;
        }
        if entries.len() + 1 > outgoing::MAX_FILES {
            return Err("entry_limit: package exceeds 4096 entries".into());
        }
        let mut collisions: Vec<_> = entries.values().map(|e| (e.path.clone(), true)).collect();
        collisions.push(("checksums.json".into(), true));
        if !paths::collisions(&collisions).is_empty() {
            return Err("output_collision: file and folder destinations collide".into());
        }
        let parent = files::child(&project.root, "Deliverables")?;
        let (package_path, zip_path) = pair(&parent)?;
        let entries: Vec<_> = entries.into_values().collect();
        check_copy_sources(&entries)?;
        refs.check()?;
        let plan = json!({"version":1,"command":"packagedeliverables","proposal_root":project.root,"package_path":package_path,"zip_path":zip_path,
            "sheets":sheets,"files":entries,"generated_files":["checksums.json"],"fingerprints":refs.fingerprints,"project_fingerprints":project.fingerprints});
        Ok(Built { plan, project, refs, entries })
    }
    #[cfg(all(test, windows))]
    mod stream_tests {
        use super::no_named_streams;
        use std::path::Path;

        #[test]
        fn unavailable_stream_inspection_is_an_error() {
            let missing = std::env::temp_dir().join(format!("cadcraft-missing-stream-{}", super::files::run_id()));
            let error = no_named_streams(&missing).unwrap_err();
            assert!(error.starts_with("stream_inspection_failed:"), "{error}");
        }

        #[test]
        fn stream_inspection_rejects_nul_and_overlong_paths() {
            for path in ["file\0hidden".to_string(), "x".repeat(32_768)] {
                let error = no_named_streams(Path::new(&path)).unwrap_err();
                assert!(error.starts_with("stream_inspection_failed:"), "{error}");
            }
        }
    }
    fn check_copy_sources(entries: &[Entry]) -> Checked<()> {
        for entry in entries.iter().filter(|entry| entry.layout.is_none()) {
            no_named_streams(&entry.source)?;
        }
        Ok(())
    }
    fn copy(source: &Path, target: &Path) -> Checked<()> {
        outgoing::safe_source(source)?;
        no_named_streams(source)?;
        let input = fs::File::open(source).map_err(|e| e.to_string())?;
        let size = input.metadata().map_err(|e| e.to_string())?.len();
        if size > outgoing::MAX_FILE_BYTES as u64 {
            return Err("source_limit: entry exceeds 512 MiB".into());
        }
        let mut output = fs::OpenOptions::new().create_new(true).write(true).open(target).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut input.take(size + 1), &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        no_named_streams(source)?;
        if copied != size {
            return Err("stale_approval: source size changed".into());
        }
        Ok(())
    }
    fn tree(root: &Path, path: &Path, found: &mut BTreeSet<String>, depth: usize) -> Checked<()> {
        if depth > 32 {
            return Err("entry_limit: package folders are too deep".into());
        }
        files::reject_link(path)?;
        no_named_streams(path)?;
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            files::reject_link(&path)?;
            if path.is_dir() {
                tree(root, &path, found, depth + 1)?;
            } else {
                let name = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
                found.insert(name);
                if found.len() > outgoing::MAX_FILES {
                    return Err("entry_limit: package file count exceeded".into());
                }
            }
        }
        Ok(())
    }
    fn verify_folder(root: &Path, checks: &BTreeMap<String, String>) -> Checked<()> {
        let mut found = BTreeSet::new();
        tree(root, root, &mut found, 0)?;
        if found != checks.keys().cloned().collect() {
            return Err("verification_failed: package file set differs from plan".into());
        }
        for (name, hash) in checks {
            let path = files::child(root, name)?;
            no_named_streams(&path)?;
            if files::fingerprint(&path)? != *hash {
                return Err("verification_failed: package hash differs".into());
            }
        }
        Ok(())
    }
    fn zip(root: &Path, path: &Path, checks: &BTreeMap<String, String>) -> Checked<()> {
        let file = fs::OpenOptions::new().create_new(true).write(true).open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut total = 0u64;
        for name in checks.keys() {
            let source = files::child(root, name)?;
            let file = fs::File::open(source).map_err(|e| e.to_string())?;
            archive.start_file(name, options).map_err(|e| e.to_string())?;
            let size = std::io::copy(&mut file.take(outgoing::MAX_FILE_BYTES as u64 + 1), &mut archive).map_err(|e| e.to_string())?;
            total = total.checked_add(size).ok_or("entry_limit: ZIP total overflow")?;
            if size > outgoing::MAX_FILE_BYTES as u64 || total > outgoing::MAX_TOTAL_BYTES {
                return Err("entry_limit: ZIP size budget exceeded".into());
            }
        }
        archive.finish().map_err(|e| e.to_string())?.sync_all().map_err(|e| e.to_string())
    }
    fn verify_zip(path: &Path, checks: &BTreeMap<String, String>) -> Checked<Vec<String>> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        if archive.len() != checks.len() || archive.len() > outgoing::MAX_FILES {
            return Err("verification_failed: ZIP entry count differs".into());
        }
        let mut found = BTreeSet::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().map_err(|e| e.to_string())?.into_owned();
            let hash = checks.get(&name).ok_or("verification_failed: ZIP has unplanned entry")?;
            if !found.insert(name) || entry.size() > outgoing::MAX_FILE_BYTES as u64 {
                return Err("verification_failed: ZIP entry duplicate or oversized".into());
            }
            let actual = approval::reader_hash(&mut entry.take(outgoing::MAX_FILE_BYTES as u64 + 1)).map_err(|e| e.to_string())?;
            if actual != *hash {
                return Err("verification_failed: ZIP entry hash differs from manifest".into());
            }
        }
        if found != checks.keys().cloned().collect() {
            return Err("verification_failed: ZIP entry set differs from plan".into());
        }
        Ok(found.into_iter().collect())
    }
    fn execute(p: &Value, b: &Built, h: &file::IoHooks, mover: &Mover<'_>) -> Checked<Value> {
        let _lock = files::lock(&b.project.root)?;
        let id = files::run_id();
        let hash = files::hash(&b.plan)?;
        let stage = files::child(&b.project.root, &format!(".packagedeliverables-{id}.stage"))?;
        let contents = stage.join("contents");
        let zip_stage = stage.join("package.zip");
        let final_dir = PathBuf::from(b.plan["package_path"].as_str().ok_or("invalid_plan: package path missing")?);
        let final_zip = PathBuf::from(b.plan["zip_path"].as_str().ok_or("invalid_plan: ZIP path missing")?);
        let parent = files::child(&b.project.root, "Deliverables")?;
        let parent_created = !parent.exists();
        let receipt_path = files::receipt_path(&b.project, "packagedeliverables", &id)?;
        let mut checks = BTreeMap::new();
        let mut published_folder = false;
        let mut published_zip = false;
        let mut zip_hash = None;
        let mut owned_stage = false;
        let mut owned_parent = false;
        let result = (|| -> Checked<Value> {
            fs::create_dir(&stage).map_err(|e| e.to_string())?;
            owned_stage = true;
            fs::create_dir(&contents).map_err(|e| e.to_string())?;
            let mut rows = Vec::new();
            let mut total = 0u64;
            let mut pdfs = 0usize;
            for entry in &b.entries {
                let target = files::child(&contents, &entry.path)?;
                files::ensure_parent(&contents, &target, &mut Vec::new())?;
                if let Some(layout) = &entry.layout {
                    let mut qa = Vec::new();
                    let d = outgoing::read_drawing(&entry.source, h)?;
                    let mut session = outgoing::plot_session(d, &entry.source, h, &mut qa)?;
                    if qa.iter().any(|q| q.severity == cadcraft_proposal::figures::Severity::Error) {
                        return Err("broken_reference: sheet references changed".into());
                    }
                    if session.doc().map_err(|e| e.to_string())?.layout(layout).is_none() {
                        return Err("layout_missing: sheet layout disappeared".into());
                    }
                    outgoing::plot(&mut session, entry.plot_options.as_ref().ok_or("invalid_plan: plot options missing")?, &target, h)?;
                    pdfs += 1;
                } else {
                    copy(&entry.source, &target)?;
                }
                let digest = files::fingerprint(&target)?;
                let size = fs::metadata(&target).map_err(|e| e.to_string())?.len();
                if entry.layout.is_none() && b.refs.fingerprints.get(&entry.source) != Some(&digest) {
                    return Err("stale_approval: copied source differs".into());
                }
                total = total.checked_add(size).ok_or("entry_limit: package total overflow")?;
                if size > outgoing::MAX_FILE_BYTES as u64 || total > outgoing::MAX_TOTAL_BYTES {
                    return Err("entry_limit: package size budget exceeded".into());
                }
                rows.push(json!({"path":entry.path,"role":entry.role,"sheet_role":entry.sheet_role,"sha256":digest,"size":size}));
                checks.insert(entry.path.clone(), digest);
            }
            if Some(pdfs) != b.plan.get("sheets").and_then(Value::as_array).map(Vec::len) {
                return Err("verification_failed: sheet PDF count differs".into());
            }
            let bytes = files::encoded(&json!({"version":1,"files":rows}))?;
            files::write_new(&contents.join("checksums.json"), &bytes)?;
            let manifest_hash = approval::sha256(&bytes);
            checks.insert("checksums.json".into(), manifest_hash.clone());
            verify_folder(&contents, &checks)?;
            zip(&contents, &zip_stage, &checks)?;
            let verified = verify_zip(&zip_stage, &checks)?;
            let digest = files::fingerprint(&zip_stage)?;
            zip_hash = Some(digest.clone());
            check_copy_sources(&b.entries)?;
            b.refs.check()?;
            b.project.check(p)?;
            if parent_created {
                fs::create_dir(&parent).map_err(|e| e.to_string())?;
                owned_parent = true;
            }
            let dir_name = final_dir.file_name().and_then(|n| n.to_str()).ok_or("unsafe_path: package name missing")?;
            let zip_name = final_zip.file_name().and_then(|n| n.to_str()).ok_or("unsafe_path: ZIP name missing")?;
            files::vacant(&parent, dir_name)?;
            files::vacant(&parent, zip_name)?;
            mover(&contents, &final_dir)?;
            published_folder = true;
            mover(&zip_stage, &final_zip)?;
            published_zip = true;
            verify_folder(&final_dir, &checks)?;
            no_named_streams(&final_zip)?;
            if files::fingerprint(&final_zip)? != digest {
                return Err("verification_failed: published ZIP changed".into());
            }
            check_copy_sources(&b.entries)?;
            b.refs.check()?;
            b.project.check(p)?;
            let receipt = json!({"version":1,"command":"packagedeliverables","outcome":"Succeeded","plan_sha256":hash,"package_path":final_dir,"zip_path":final_zip,"zip_sha256":digest,
                "manifest_sha256":manifest_hash,"files":rows,"verified_zip_entries":verified,"receipt_path":receipt_path});
            files::write_new(&receipt_path, &files::encoded(&receipt)?)?;
            Ok(receipt)
        })();
        let mut cleanup = Vec::new();
        if result.is_err() {
            if published_zip {
                if no_named_streams(&final_zip).is_ok() && files::fingerprint(&final_zip).ok().as_ref() == zip_hash.as_ref() {
                    if let Err(e) = fs::remove_file(&final_zip) {
                        cleanup.push(e.to_string());
                    }
                } else {
                    cleanup.push(format!("Retained changed ZIP {}", final_zip.display()));
                }
            }
            if published_folder {
                if verify_folder(&final_dir, &checks).is_ok() {
                    if let Err(e) = fs::remove_dir_all(&final_dir) {
                        cleanup.push(e.to_string());
                    }
                } else {
                    cleanup.push(format!("Retained changed package {}", final_dir.display()));
                }
            }
        }
        if owned_stage && let Err(e) = fs::remove_dir_all(&stage) {
            cleanup.push(e.to_string());
        }
        if result.is_err()
            && owned_parent
            && parent.exists()
            && let Err(e) = fs::remove_dir(&parent)
        {
            cleanup.push(e.to_string());
        }
        Ok(match result {
            Ok(receipt) => json!({"receipt":receipt,"cleanup_failures":cleanup}),
            Err(error) => files::failed_receipt(&receipt_path, "packagedeliverables", &hash, error, cleanup),
        })
    }
    pub fn run(p: &Value, mover: &Mover<'_>) -> Checked<Value> {
        let r = request(p)?;
        let h = file::io().filter(|h| h.plot.is_some()).ok_or("plot_unavailable: plotting is not available in this build")?;
        run_request(p, &r, h, mover)
    }
    #[cfg(test)]
    pub fn run_with_hooks(p: &Value, h: &file::IoHooks, mover: &Mover<'_>) -> Checked<Value> {
        let r = request(p)?;
        run_request(p, &r, h, mover)
    }
    fn request(p: &Value) -> Checked<Request> {
        if serde_json::to_vec(p).map_err(|e| e.to_string())?.len() > 4 << 20 {
            return Err("invalid_input: inputs exceed 4 MiB".into());
        }
        let r: Request = serde_json::from_value(p.clone()).map_err(|e| format!("invalid_input: {e}"))?;
        if !["plan", "preview", "execute", "create"].contains(&r.action.as_str()) {
            return Err("invalid_input: expected plan, preview, execute or create (alias for execute)".into());
        }
        // Read these fields through the same Project::open contract as the existing follow-ons.
        if r.proposal_root.is_empty() {
            return Err("invalid_input: proposal_root is empty".into());
        }
        Ok(r)
    }
    fn run_request(p: &Value, r: &Request, h: &file::IoHooks, mover: &Mover<'_>) -> Checked<Value> {
        if h.plot.is_none() {
            return Err("plot_unavailable: plotting is not available in this build".into());
        }
        let b = build(p, r, h)?;
        let hash = files::hash(&b.plan)?;
        if r.action == "plan" || r.action == "preview" {
            return Ok(json!({"plan":b.plan,"hash":hash,"issues":[]}));
        }
        if let Some(refused) = files::approval_failure(&hash, r.approved_hash.as_deref()) {
            return Ok(refused);
        }
        execute(p, &b, h, mover)
    }
}
