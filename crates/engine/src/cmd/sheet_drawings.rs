//! Prepare the entire DWG set before exclusive publication; commit the source manifest last.
use super::{
    alignment_cmd::selected,
    alignment_store, bad, file,
    proposal::files,
    sheet_layouts::native as layouts,
    sheet_store::{self as store, Drawings, Group, Published, SheetConfig, SheetRun},
};
use crate::{Result, Session};
use cadcraft_doc::{Drawing, EntityKind};
use cadcraft_proposal::{approval::sha256, paths};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
const CMD: &str = "sheets";
fn checked<T>(value: files::Checked<T>) -> Result<T> {
    value.map_err(|e| bad(CMD, e))
}
fn settings(p: &Value) -> Result<Drawings> {
    let folder = p
        .get("output_folder")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(|| bad(CMD, "OUTPUT_FOLDER: existing output directory required"))?;
    checked(files::reject_link(Path::new(folder)))?;
    let folder = fs::canonicalize(folder).map_err(|e| bad(CMD, format!("OUTPUT_FOLDER: {e}")))?;
    if !folder.is_dir() {
        return Err(bad(CMD, "OUTPUT_FOLDER: expected a directory"));
    }
    let name = match p.get("drawing_name") {
        None => "Sheet-{n}",
        Some(Value::String(s)) if !s.is_empty() && s.len() <= 4096 => s,
        _ => return Err(bad(CMD, "DRAWING_NAME: bounded name template required")),
    };
    let per = p
        .get("layouts_per_drawing")
        .map_or(Some(1), Value::as_u64)
        .filter(|n| *n > 0 && *n <= 4096)
        .ok_or_else(|| bad(CMD, "SHEET_LIMIT: layouts_per_drawing must be 1..4096"))?;
    Ok(Drawings {
        folder: folder.to_string_lossy().into_owned(),
        name: name.into(),
        per: per as usize,
        version: 1,
        input_hash: String::new(),
        files: Vec::new(),
    })
}
fn signature(d: &Drawing, g: &Group, c: &SheetConfig, settings: &Drawings) -> Result<String> {
    let mut g = g.clone();
    g.sheets = None;
    for f in &g.frames {
        let h = store::handle(&f.handle)?;
        if d.model.contains(h) {
            store::check(d, h, &g.own, f.index, "frame")?;
        }
    }
    let a = g.alignment.as_deref().map(|h| alignment_store::load(d, store::handle(h)?)).transpose()?;
    let template = checked(files::read(Path::new(&c.template), file::MAX_DRAWING_BYTES))?;
    let model = d.model.iter().map(|e| (e.handle, (&e.common, &e.kind))).collect::<BTreeMap<_, _>>();
    layouts::digest(&(
        &g,
        c,
        &settings.folder,
        &settings.name,
        settings.per,
        model,
        &d.blocks,
        // Use the renderer's effective dash settings, including its defaults.
        (d.header.f64("LTSCALE", 1.), d.header.i64("PSLTSCALE", 1) != 0),
        (
            &d.layers,
            &d.linetypes,
            &d.text_styles,
            &d.dim_styles,
            &d.mleader_styles,
            &d.table_styles,
            &d.mline_styles,
            &d.plot_style_tables,
            &d.scales,
        ),
        a,
        sha256(&template),
        layouts::model_hash(d, &g)?,
    ))
}
fn intact(f: &Published) -> Result<()> {
    let path = Path::new(&f.path);
    checked(files::reject_link(path))?;
    checked(files::no_named_streams(path))?;
    if checked(files::fingerprint(path))? != f.sha256 {
        return Err(bad(CMD, format!("OWNED_FILE_CHANGED: {}", path.display())));
    }
    Ok(())
}
struct Prepared {
    stage: PathBuf,
    published: Published,
}
fn validate_manifest(run: &SheetRun, output: &Drawings) -> Result<()> {
    if run.layouts.len() > 4096 || output.input_hash.len() != 64 || !output.input_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad(CMD, "OWNERSHIP: invalid output manifest"));
    }
    let mut expected = BTreeMap::<&str, BTreeSet<&str>>::new();
    for sheet in &run.layouts {
        let path = sheet.file.as_deref().ok_or_else(|| bad(CMD, "OWNERSHIP: sheet file missing"))?;
        if !expected.entry(path).or_default().insert(&sheet.name) {
            return Err(bad(CMD, "OWNERSHIP: duplicate sheet identity"));
        }
    }
    let actual =
        output.files.iter().map(|f| (f.path.as_str(), f.layouts.iter().map(String::as_str).collect::<BTreeSet<_>>())).collect::<BTreeMap<_, _>>();
    if actual.len() != output.files.len() || actual != expected {
        return Err(bad(CMD, "OWNERSHIP: file and layout manifests disagree"));
    }
    Ok(())
}
fn publish(batch: &[Prepared], mover: &files::Mover<'_>) -> Result<()> {
    let mut written = Vec::new();
    let result: Result<()> = (|| {
        for f in batch {
            checked(mover(&f.stage, Path::new(&f.published.path)))?;
            written.push(&f.published);
            intact(&f.published)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let mut failures = Vec::new();
        for f in written.into_iter().rev() {
            if let Err(e) = intact(f).and_then(|()| fs::remove_file(&f.path).map_err(|e| bad(CMD, e.to_string()))) {
                failures.push(format!("{}: {e}", f.path));
            }
        }
        return Err(bad(CMD, format!("PUBLISH_FAILED: {e}; rollback failures: {failures:?}")));
    }
    Ok(())
}
pub(super) fn run(s: &mut Session, p: &Value, update: bool) -> Result<Value> {
    run_with_move(s, p, update, &files::publish_file_no_replace)
}
fn run_with_move(s: &mut Session, p: &Value, update: bool, mover: &files::Mover<'_>) -> Result<Value> {
    // Do this before resolving paths or staging anything: cloning civil objects needs explicit consent.
    if p.get("duplicate_objects").and_then(Value::as_bool) != Some(true) {
        return Err(bad(CMD, "DUPLICATION_REQUIRED: drawings storage requires duplicate_objects:true"));
    }
    let h = selected(s, p, "group", CMD)?;
    let original = s.doc()?.clone();
    let mut g = store::load(&original, h)?;
    let old = g.sheets.clone();
    let (c, mut output) = if update {
        let r = old.as_ref().ok_or_else(|| bad(CMD, "NO_SHEETS: create sheets first"))?;
        (r.config.clone(), r.drawings.clone().ok_or_else(|| bad(CMD, "STORAGE: existing sheets are not drawing files"))?)
    } else {
        if old.is_some() {
            return Err(bad(CMD, "SHEETS_EXIST: use sheetupdate"));
        }
        (layouts::config(p)?, settings(p)?)
    };
    if output.per == 0 || output.per > 4096 || output.files.len() > 4096 || output.version == 0 {
        return Err(bad(CMD, "INVALID_FRAME_GROUP: invalid drawing batch"));
    }
    if let Some(old) = old.as_ref() {
        validate_manifest(old, &output)?;
    }
    let root = PathBuf::from(&output.folder);
    checked(files::reject_link(&root))?;
    if !root.is_absolute() || !root.is_dir() || fs::canonicalize(&root).map_err(|e| bad(CMD, e.to_string()))? != root {
        return Err(bad(CMD, "OUTPUT_FOLDER: stored output directory changed"));
    }
    let mut old_bytes = 0u64;
    let mut old_paths = BTreeSet::new();
    for f in &output.files {
        let path = Path::new(&f.path);
        if path.parent() != Some(root.as_path())
            || f.sha256.len() != 64
            || f.layouts.is_empty()
            || f.layouts.len() > output.per
            || !old_paths.insert(paths::case_fold(&f.path))
        {
            return Err(bad(CMD, "OWNERSHIP: invalid output file record"));
        }
        old_bytes = old_bytes.saturating_add(fs::symlink_metadata(path).map_err(|e| bad(CMD, e.to_string()))?.len());
        if old_bytes > file::MAX_DRAWING_BYTES as u64 {
            return Err(bad(CMD, "SHEET_LIMIT: owned batch exceeds drawing byte limit"));
        }
        intact(f)?;
        let hooks = file::io().ok_or_else(|| bad(CMD, "drawing I/O unavailable"))?;
        let drawing = (hooks.read)(&checked(files::read(path, file::MAX_DRAWING_BYTES))?, &f.path).map_err(|e| bad(CMD, e))?;
        let owner = store::load(&drawing, h)?;
        let run = owner.sheets.as_ref().ok_or_else(|| bad(CMD, "OWNERSHIP: published manifest missing"))?;
        let names = run.layouts.iter().map(|s| s.name.clone()).collect::<BTreeSet<_>>();
        if names != f.layouts.iter().cloned().collect()
            || run.layouts.iter().any(|s| s.file.as_deref() != Some(f.path.as_str()))
            || !layouts::owned(&drawing, &owner, run)?
        {
            return Err(bad(CMD, "OWNERSHIP: published layout identity changed"));
        }
        let expected =
            old.as_ref().map(|r| r.layouts.iter().filter(|s| s.file.as_deref() == Some(f.path.as_str())).collect::<Vec<_>>()).unwrap_or_default();
        if layouts::digest(&expected)? != layouts::digest(&run.layouts)? {
            return Err(bad(CMD, "OWNERSHIP: source and published sheet records disagree"));
        }
    }
    let input = signature(&original, &g, &c, &output)?;
    if update && output.input_hash == input && p.get("add_frames").is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)) {
        return Ok(
            json!({"group":g.own,"changed":false,"layouts":old.as_ref().map(|r|r.layouts.iter().map(|s|&s.name).collect::<Vec<_>>()),"warnings":[],"gaps_overlaps":layouts::gaps_overlaps(&g),"written":[],"superseded":[],"undo_removes_files":false}),
        );
    }
    let superseded = output.files.clone();
    if update {
        output.version = output.version.checked_add(1).ok_or_else(|| bad(CMD, "VERSION_LIMIT"))?;
    }
    let mut stage_session = Session::new();
    *stage_session.doc_mut()? = original.clone();
    g.sheets = None;
    store::save(stage_session.doc_mut()?, &g)?;
    let mut args = serde_json::to_value(&c).map_err(|e| bad(CMD, e.to_string()))?;
    args.as_object_mut().ok_or_else(|| bad(CMD, "INVALID_FRAME_GROUP: configuration object required"))?.retain(|_, v| !v.is_null());
    args["group"] = json!(g.own);
    args["storage"] = json!("current");
    if let Some(add) = p.get("add_frames") {
        args["add_frames"] = add.clone();
    }
    let mut report = layouts::run(&mut stage_session, &args, false)?;
    let mut prepared_source = stage_session.doc()?.clone();
    let mut generated = store::load(&prepared_source, h)?;
    let mut run = generated.sheets.clone().ok_or_else(|| bad(CMD, "NO_SHEETS"))?;
    let a = generated.alignment.as_deref().map(|h| alignment_store::load(&prepared_source, store::handle(h)?)).transpose()?;
    let mut names = BTreeSet::new();
    let mut plans = Vec::new();
    for (i, chunk) in run.layouts.chunks(output.per).enumerate() {
        let n = u32::try_from(i)
            .ok()
            .and_then(|i| i.checked_mul(c.number_increment))
            .and_then(|i| c.number_start.checked_add(i))
            .ok_or_else(|| bad(CMD, "NUMBER_LIMIT"))?;
        let number = if c.number_width == 0 { n.to_string() } else { format!("{n:0width$}", width = c.number_width) };
        let indices =
            chunk.iter().flat_map(|s| &s.frames).filter_map(|index| generated.frames.iter().position(|f| f.index == *index)).collect::<Vec<_>>();
        let values = layouts::values(&generated, a.as_ref(), &indices, &number, run.layouts.len());
        let mut naming = c.clone();
        naming.layout_name = output.name.clone();
        let mut warnings = Vec::new();
        let mut stem = layouts::name(&naming, &number, &values, &mut warnings)?;
        if let Some(list) = report.get_mut("warnings").and_then(Value::as_array_mut) {
            list.extend(warnings.into_iter().map(|w| json!(w)));
        }
        if stem.to_ascii_lowercase().ends_with(".dwg") {
            stem.truncate(stem.len() - 4);
        }
        if output.version > 1 {
            stem.push_str(&format!("_v{}", output.version));
        }
        let filename = format!("{stem}.dwg");
        paths::component(&filename).map_err(|e| bad(CMD, format!("{}: {}", e.code, e.message)))?;
        if !names.insert(paths::case_fold(&filename)) {
            return Err(bad(CMD, "DRAWING_COLLISION: duplicate drawing name"));
        }
        let target = checked(files::vacant(&root, &filename)).map_err(|e| bad(CMD, format!("DRAWING_COLLISION: {e}")))?;
        plans.push((target, chunk.to_vec()));
    }
    // No file or stage exists until every name and existing owned file has passed preflight.
    let stage = root.join(format!(".cadcraft-sheets-{}", files::run_id()));
    fs::create_dir(&stage).map_err(|e| bad(CMD, e.to_string()))?;
    let mut batch = Vec::new();
    let mut staged_paths = Vec::new();
    let result = (|| {
        let hooks = file::io().ok_or_else(|| bad(CMD, "drawing I/O unavailable"))?;
        let mut total = 0usize;
        for (i, (path, mut sheets)) in plans.into_iter().enumerate() {
            let mut drawing = prepared_source.clone();
            let wanted = sheets.iter().map(|s| s.name.clone()).collect::<BTreeSet<_>>();
            drawing.layouts.retain(|l| wanted.contains(&l.name));
            for sheet in &mut sheets {
                sheet.file = Some(path.to_string_lossy().into_owned());
            }
            let mut copy_group = generated.clone();
            let mut copy_run = run.clone();
            copy_run.layouts = sheets.clone();
            copy_run.drawings = None;
            copy_group.sheets = Some(copy_run);
            store::save(&mut drawing, &copy_group)?;
            if !drawing.read_notes.is_empty()
                || !drawing.skipped_records.is_empty()
                || !file::not_saved(&drawing).is_empty()
                || drawing.blocks.values().any(|b| b.xref_path.is_some())
                || drawing.model.iter().any(|e| matches!(e.kind, EntityKind::Image(_)))
            {
                return Err(bad(CMD, "DRAWING_DATA_UNSUPPORTED: sheet copies cannot preserve these source references"));
            }
            let name = path.to_str().ok_or_else(|| bad(CMD, "OUTPUT_FOLDER: non-UTF8 path"))?;
            let bytes = (hooks.write)(&drawing, name).map_err(|e| bad(CMD, e))?;
            total = total.saturating_add(bytes.len());
            if total > file::MAX_DRAWING_BYTES {
                return Err(bad(CMD, "SHEET_LIMIT: staged batch exceeds drawing byte limit"));
            }
            let stage_path = stage.join(format!("{i}.dwg"));
            checked(files::write_new(&stage_path, &bytes))?;
            staged_paths.push(stage_path.clone());
            let verify = checked(files::read(&stage_path, file::MAX_DRAWING_BYTES))?;
            let reopened = (hooks.read)(&verify, name).map_err(|e| bad(CMD, e))?;
            let reopened_group = store::load(&reopened, h)?;
            let expected = drawing.model.iter().map(|e| (e.handle, (&e.common, &e.kind))).collect::<BTreeMap<_, _>>();
            let actual = reopened.model.iter().map(|e| (e.handle, (&e.common, &e.kind))).collect::<BTreeMap<_, _>>();
            let checks = [
                ("reader warnings", reopened.read_notes.is_empty()),
                ("layout count", reopened.layouts.len() == sheets.len()),
                ("model entities", expected == actual),
                ("block definitions", drawing.blocks == reopened.blocks),
                (
                    "layer and style definitions",
                    drawing.layers == reopened.layers
                        && drawing.linetypes == reopened.linetypes
                        && drawing.text_styles == reopened.text_styles
                        && drawing.dim_styles == reopened.dim_styles
                        && drawing.mleader_styles == reopened.mleader_styles
                        && drawing.table_styles == reopened.table_styles
                        && drawing.mline_styles == reopened.mline_styles
                        && drawing.plot_style_tables == reopened.plot_style_tables,
                ),
                ("group manifest", layouts::digest(&copy_group)? == layouts::digest(&reopened_group)?),
                (
                    "owned layouts",
                    layouts::owned(
                        &reopened,
                        &reopened_group,
                        reopened_group.sheets.as_ref().ok_or_else(|| bad(CMD, "VERIFY: missing sheet manifest"))?,
                    )?,
                ),
            ];
            if let Some((reason, _)) = checks.iter().find(|(_, pass)| !pass) {
                return Err(bad(CMD, format!("VERIFY: reopened DWG differs in {reason}")));
            }
            for sheet in &mut run.layouts {
                if wanted.contains(&sheet.name) {
                    sheet.file = Some(path.to_string_lossy().into_owned());
                }
            }
            batch.push(Prepared {
                stage: stage_path,
                published: Published { path: path.to_string_lossy().into_owned(), sha256: sha256(&verify), layouts: wanted.into_iter().collect() },
            });
        }
        // Finish and validate the source manifest before publication, so a storage error cannot strand a batch.
        for sheet in &run.layouts {
            store::erase(&mut prepared_source, &sheet.members, &generated.own, *sheet.frames.first().ok_or_else(|| bad(CMD, "NO_FRAMES"))?, "sheet")?;
        }
        prepared_source.layouts = original.layouts.clone();
        output.files = batch.iter().map(|f| f.published.clone()).collect();
        output.input_hash = signature(&prepared_source, &generated, &c, &output)?;
        run.drawings = Some(output.clone());
        generated.sheets = Some(run);
        store::save(&mut prepared_source, &generated)?;
        for f in &superseded {
            intact(f)?;
        }
        publish(&batch, mover)?;
        *s.doc_mut()? = prepared_source;
        report["written"] = json!(output.files);
        report["superseded"] = json!(superseded);
        report["undo_removes_files"] = json!(false);
        Ok(report)
    })();
    let mut cleanup = Vec::new();
    for path in staged_paths {
        if let Err(e) = checked(files::reject_link(&stage)).and_then(|()| fs::remove_file(&path).map_err(|e| bad(CMD, e.to_string()))) {
            cleanup.push(e.to_string());
        }
    }
    if let Err(e) = fs::remove_dir(&stage) {
        cleanup.push(e.to_string());
    }
    if !cleanup.is_empty() {
        return match result {
            Ok(mut report) => {
                report["cleanup_failures"] = json!(cleanup);
                Ok(report)
            }
            Err(e) => Err(bad(CMD, format!("{e}; stage cleanup: {cleanup:?}"))),
        };
    }
    result
}
#[cfg(test)]
mod tests {
    use super::{Prepared, Published, SheetRun, files, publish, sha256, validate_manifest};
    use std::{cell::Cell, fs};
    #[test]
    fn corrupt_file_manifest_cannot_drop_or_reassign_a_sheet() {
        let mut run:SheetRun=serde_json::from_value(serde_json::json!({
            "config":{"template":"template.dxf","template_layout":"T","arrangement":"plan_only","layout_name":"S-{n}","number_start":1,"number_increment":1,"number_width":0,"north_arrow":null},
            "source_hash":"unused","layouts":[{"name":"S-1","marker":"10","frames":[0],"members":["10"],"hash":"unused","file":"one.dwg"}],
            "drawings":{"folder":".","name":"S-{n}","per":1,"version":1,"input_hash":sha256(b"input"),"files":[{"path":"one.dwg","sha256":sha256(b"file"),"layouts":["S-1"]}]}
        })).unwrap();
        let output = run.drawings.clone().unwrap();
        validate_manifest(&run, &output).unwrap();
        let mut missing = output.clone();
        missing.files.clear();
        assert!(validate_manifest(&run, &missing).is_err());
        run.layouts[0].file = Some("unowned.dwg".into());
        assert!(validate_manifest(&run, &output).is_err());
        run.layouts[0].file = Some("one.dwg".into());
        let mut corrupt = output;
        corrupt.input_hash.clear();
        assert!(validate_manifest(&run, &corrupt).is_err());
    }
    #[test]
    fn failed_second_publish_rolls_back_only_unchanged_owned_outputs() {
        for changed in [false, true] {
            let root = std::env::temp_dir().join(format!("cadcraft-sheet-publish-{}", files::run_id()));
            fs::create_dir(&root).unwrap();
            let batch = (0..2)
                .map(|i| {
                    let stage = root.join(format!("{i}.stage"));
                    fs::write(&stage, b"original").unwrap();
                    Prepared {
                        stage,
                        published: Published {
                            path: root.join(format!("{i}.dwg")).to_string_lossy().into_owned(),
                            sha256: sha256(b"original"),
                            layouts: vec!["Sheet".into()],
                        },
                    }
                })
                .collect::<Vec<_>>();
            let calls = Cell::new(0);
            let mover = |from: &std::path::Path, to: &std::path::Path| {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    if changed {
                        fs::write(&batch[0].published.path, b"user edit").unwrap();
                    }
                    fs::write(to, b"unowned racer").unwrap();
                    return Err("forced collision".into());
                }
                files::publish_file_no_replace(from, to)
            };
            let error = publish(&batch, &mover).unwrap_err().to_string();
            assert!(error.contains("PUBLISH_FAILED"));
            assert_eq!(fs::read(&batch[1].published.path).unwrap(), b"unowned racer");
            if changed {
                assert_eq!(fs::read(&batch[0].published.path).unwrap(), b"user edit");
                assert!(error.contains("OWNED_FILE_CHANGED"));
            } else {
                assert!(!std::path::Path::new(&batch[0].published.path).exists());
            }
            fs::remove_dir_all(&root).unwrap();
        }
    }
    #[test]
    fn exclusive_file_publish_never_overwrites_a_destination() {
        let root = std::env::temp_dir().join(format!("cadcraft-sheet-exclusive-{}", files::run_id()));
        fs::create_dir(&root).unwrap();
        let from = root.join("stage");
        let to = root.join("output");
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"existing").unwrap();
        assert!(files::publish_file_no_replace(&from, &to).is_err());
        assert_eq!(fs::read(&to).unwrap(), b"existing");
        assert!(files::publish_file_no_replace(&root, &root.join("directory-copy")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
