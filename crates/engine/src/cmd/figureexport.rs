//! Prepare figures in separate sessions and publish verified plots without changing drawings.
use super::{CommandSpec, bad};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("figureexport", "Export Figures", run)
        .menu(&["File", "Export Figures"])
        .params("{action:plan|preview|execute,spec_paths:[path],sites:[label],events:[label],mode:per_site|per_map_type|historical|combined_appendix,format:pdf|png,output_folder,approved_hash?}")
        .noundo().interactive(|_| Ok(prompt("figureexport")))]
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{super::file, json, native};
    use cadcraft_doc::{Drawing, EntityKind, Layer, Layout, Space, Viewport};
    use cadcraft_geom::Vec3;
    use cadcraft_proposal::figures::FigureSpec;

    #[test]
    fn preparation_replaces_text_attributes_and_only_largest_viewport() {
        let mut d = Drawing::new_metric();
        d.layers.push(Layer::new("Boundary"));
        d.layouts.push(Layout::new("Figure", 1));
        let space = Space::Paper("Figure".into());
        let text =
            serde_json::from_value(json!({"insert":[0,0,0],"height":3,"value":"{{Site}} {{Event}} {{FigureId}} {{Missing}} tail {{unfinished"}))
                .unwrap();
        let t = d.add(&space, Default::default(), EntityKind::Text(text)).unwrap();
        let mtext = serde_json::from_value(json!({"insert":[0,0,0],"height":3,"contents":"{{Site}} {{Custom}}"})).unwrap();
        let m = d.add(&space, Default::default(), EntityKind::MText(mtext)).unwrap();
        let insert = serde_json::from_value(
            json!({"block":"Title","insert":[0,0,0],"attribs":[{"tag":"A","text":{"insert":[0,0,0],"height":3,"value":"{{Event}}"}}]}),
        )
        .unwrap();
        let a = d.add(&space, Default::default(), EntityKind::Insert(insert)).unwrap();
        let small = d
            .add(
                &space,
                Default::default(),
                EntityKind::Viewport(Viewport { width: 10., height: 10., id: 2, view_height: 777., ..Default::default() }),
            )
            .unwrap();
        let big = d
            .add(
                &space,
                Default::default(),
                EntityKind::Viewport(Viewport { width: 100., height: 50., id: 3, view_height: 999., ..Default::default() }),
            )
            .unwrap();
        d.add(
            &Space::Model,
            cadcraft_doc::Common { layer: "Boundary".into(), ..Default::default() },
            EntityKind::Line(cadcraft_doc::Line { a: Vec3::ZERO, b: Vec3::new(100., 100., 0.) }),
        )
        .unwrap();
        let spec=FigureSpec::parse(br#"{"figure_id":"F","source_drawing":"a.dxf","layout":"Figure","extent_boundary_layer":"Boundary","extent_buffer_pct":10,"text":{"Custom":"value"}}"#).unwrap();
        let original = d.clone();
        let mut qa = Vec::new();
        assert_eq!(native::prepare(&mut d, &spec, "Site A", "Event B", &mut qa).unwrap(), Some(big.hex()));
        assert_eq!(original.layout("Figure").unwrap().entities.get(small), d.layout("Figure").unwrap().entities.get(small));
        let store = &d.layout("Figure").unwrap().entities;
        assert!(matches!(&store.get(t).unwrap().kind,EntityKind::Text(t) if t.value=="Site A Event B F {{Missing}} tail {{unfinished"));
        assert!(matches!(&store.get(m).unwrap().kind,EntityKind::MText(t) if t.contents=="Site A value"));
        assert!(matches!(&store.get(a).unwrap().kind,EntityKind::Insert(i) if i.attribs[0].text.value=="Event B"));
        assert!(
            matches!(&store.get(big).unwrap().kind,EntityKind::Viewport(v) if v.view_center==cadcraft_geom::Vec2::new(50.,50.) && (v.view_height-120.).abs()<1e-8)
        );
        assert_eq!(qa.iter().filter(|q| q.code == "unresolved_placeholder").count(), 1);
    }

    #[test]
    fn failed_second_publish_removes_first_output_and_success_receipt() {
        file::set_io(file::IoHooks {
            read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
            write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
            plot: Some(|d, s, p| cadcraft_io::plotter::plot(d, s, p).map_err(|e| e.to_string())),
        });
        let root = std::env::temp_dir().join(format!("cadcraft-figure-publish-{}", super::super::proposal::files::run_id()));
        std::fs::create_dir(&root).unwrap();
        let mut d = Drawing::new_metric();
        d.layouts.push(Layout::new("Figure", 1));
        std::fs::write(root.join("source.dxf"), cadcraft_io::write(&d, "source.dxf").unwrap()).unwrap();
        std::fs::write(root.join("spec.json"), br#"{"figure_id":"F","source_drawing":"source.dxf","layout":"Figure"}"#).unwrap();
        let mut p = json!({"action":"plan","spec_paths":[root.join("spec.json")],"sites":["A","B"],"events":["E"],"mode":"per_site","format":"pdf","output_folder":root.join("out")});
        let planned = native::run(&p, &super::super::proposal::files::rename_no_replace).unwrap();
        p["action"] = json!("execute");
        p["approved_hash"] = planned["hash"].clone();
        let count = std::cell::Cell::new(0);
        let failure = |from: &std::path::Path, to: &std::path::Path| {
            count.set(count.get() + 1);
            if count.get() == 2 { Err("forced second publish failure".into()) } else { super::super::proposal::files::rename_no_replace(from, to) }
        };
        let result = native::run(&p, &failure).unwrap();
        assert_eq!(result["receipt"]["outcome"], "Failed", "{result}");
        assert_eq!(count.get(), 2);
        let files: Vec<_> = std::fs::read_dir(root.join("out")).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&files[0]).unwrap()).unwrap()["outcome"], "Failed");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn failed_second_publish_retains_added_named_stream() {
        check_retained_stream(b"caller-added data").unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn failed_second_publish_retains_added_empty_named_stream() {
        check_retained_stream(b"").unwrap();
    }

    #[cfg(windows)]
    fn check_retained_stream(contents: &[u8]) -> std::result::Result<(), Box<dyn std::error::Error>> {
        file::set_io(file::IoHooks {
            read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
            write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
            plot: Some(|d, s, p| cadcraft_io::plotter::plot(d, s, p).map_err(|e| e.to_string())),
        });
        let root = std::env::temp_dir().join(format!("cadcraft-figure-stream-{}", super::super::proposal::files::run_id()));
        std::fs::create_dir(&root)?;
        let mut d = Drawing::new_metric();
        d.layouts.push(Layout::new("Figure", 1));
        std::fs::write(root.join("source.dxf"), cadcraft_io::write(&d, "source.dxf")?)?;
        std::fs::write(root.join("spec.json"), br#"{"figure_id":"F","source_drawing":"source.dxf","layout":"Figure"}"#)?;
        let mut p = json!({"action":"plan","spec_paths":[root.join("spec.json")],"sites":["A","B"],"events":["E"],"mode":"per_site","format":"pdf","output_folder":root.join("out")});
        let planned = native::run(&p, &super::super::proposal::files::rename_no_replace)?;
        p["action"] = json!("execute");
        p["approved_hash"] = planned["hash"].clone();
        let count = std::cell::Cell::new(0);
        let original = std::cell::RefCell::new(Vec::new());
        let failure = |from: &std::path::Path, to: &std::path::Path| {
            count.set(count.get() + 1);
            if count.get() == 2 {
                return Err("forced second publish failure".into());
            }
            super::super::proposal::files::rename_no_replace(from, to)?;
            *original.borrow_mut() = std::fs::read(to).map_err(|e| e.to_string())?;
            let mut stream = to.as_os_str().to_os_string();
            stream.push(":additional");
            std::fs::write(stream, contents).map_err(|e| e.to_string())?;
            Ok(())
        };
        let result = native::run(&p, &failure)?;
        let first = root.join("out/A_E_F.pdf");
        let actual = std::fs::read(&first);
        let mut stream = first.as_os_str().to_os_string();
        stream.push(":additional");
        let added = std::fs::read(stream);
        let files: Vec<_> = std::fs::read_dir(root.join("out"))?.map(|e| e.map(|e| e.path())).collect::<std::io::Result<_>>()?;
        let receipt = std::fs::read(result["receipt"]["receipt_path"].as_str().ok_or("receipt path missing")?)?;
        std::fs::remove_dir_all(root)?;
        assert_eq!(result["receipt"]["outcome"], "Failed", "{result}");
        assert_eq!(count.get(), 2);
        assert!(actual.is_ok(), "rollback removed an artifact with a named stream: {result}");
        assert_eq!(actual?, original.into_inner());
        assert_eq!(added?, contents);
        assert_eq!(files.len(), 2, "only the retained PDF and Failed receipt remain");
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&receipt)?, result["receipt"]);
        assert!(
            result["receipt"]["cleanup_failures"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|v| v.as_str().is_some_and(|s| s.contains("A_E_F.pdf"))))
        );
        Ok(())
    }
}
fn run(_: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("figureexport", "Filesystem figure export is not available on web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(native::run(p, &super::proposal::files::rename_no_replace).unwrap_or_else(super::proposal::files::issue))
    }
}

pub(super) fn prompt(command: &'static str) -> Box<dyn Interactive> {
    Box::new(ExportPrompt { command, params: Value::Null, hash: None })
}
struct ExportPrompt {
    command: &'static str,
    params: Value,
    hash: Option<String>,
}
impl Interactive for ExportPrompt {
    fn name(&self) -> &'static str {
        self.command
    }
    fn prompt(&self, _: &Session) -> Prompt {
        Prompt::new(if self.hash.is_none() { "Export inputs as JSON object" } else { "Approve this exact plan? Type Yes to execute" }, Accept::TEXT)
    }
    fn input(&mut self, s: &mut Session, input: Input) -> Result<Step> {
        if input == Input::Cancel {
            return Ok(Step::Cancel);
        }
        let text = match input {
            Input::Text(t) | Input::Keyword(t) => t,
            Input::Enter => String::new(),
            _ => return Ok(Step::Continue),
        };
        if self.hash.is_none() {
            if text.len() > 4 << 20 {
                return Err(bad(self.command, "Inputs exceed 4 MiB"));
            }
            self.params = serde_json::from_str(&text).map_err(|e| bad(self.command, e.to_string()))?;
            if !self.params.is_object() {
                return Err(bad(self.command, "Expected an inputs object"));
            }
            self.params["action"] = json!("plan");
            let result = s.execute(self.command, &self.params)?;
            self.hash = result.get("hash").and_then(Value::as_str).map(str::to_string);
            s.echo(serde_json::to_string_pretty(&result).map_err(|e| bad(self.command, e.to_string()))?);
            return Ok(if self.hash.is_some() { Step::Continue } else { Step::Done });
        }
        if text.eq_ignore_ascii_case("yes") {
            self.params["action"] = json!("execute");
            self.params["approved_hash"] = json!(self.hash);
            let result = s.execute(self.command, &self.params)?;
            s.echo(serde_json::to_string(&result).map_err(|e| bad(self.command, e.to_string()))?);
        } else {
            s.echo("Export cancelled");
        }
        Ok(Step::Done)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) mod native {
    use super::super::{
        file,
        proposal::files::{self, Checked, Mover},
    };
    use super::{Session, Value, json};
    use cadcraft_doc::{Drawing, EntityKind, XrefStatus};
    use cadcraft_geom::Bounds2;
    use cadcraft_proposal::{
        figures::{self, FigureSpec, Qa, Severity},
        paths,
    };
    use serde::Deserialize;
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::{Path, PathBuf},
    };

    pub const MAX_FILES: usize = 4096;
    pub const MAX_FILE_BYTES: usize = 512 << 20;
    pub const MAX_TOTAL_BYTES: u64 = 2 << 30;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        action: String,
        spec_paths: Vec<String>,
        sites: Vec<String>,
        events: Vec<String>,
        mode: String,
        format: String,
        output_folder: String,
        approved_hash: Option<String>,
    }
    struct Loaded {
        spec: FigureSpec,
        source: PathBuf,
    }
    pub struct References {
        pub fingerprints: BTreeMap<PathBuf, String>,
        bytes: u64,
    }
    impl References {
        pub fn new() -> Self {
            Self { fingerprints: BTreeMap::new(), bytes: 0 }
        }
        pub fn add(&mut self, path: &Path) -> Checked<()> {
            safe_source(path)?;
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if self.fingerprints.contains_key(&path) {
                return Ok(());
            }
            let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
            self.bytes = self.bytes.checked_add(size).ok_or("source_limit: total size overflow")?;
            if self.fingerprints.len() >= MAX_FILES || size > MAX_FILE_BYTES as u64 || self.bytes > MAX_TOTAL_BYTES {
                return Err("source_limit: outgoing file budget exceeded".into());
            }
            self.fingerprints.insert(path.clone(), files::fingerprint(&path)?);
            Ok(())
        }
        pub fn check(&self) -> Checked<()> {
            for (path, hash) in &self.fingerprints {
                safe_source(path)?;
                if &files::fingerprint(path)? != hash {
                    return Err("stale_approval: source changed".into());
                }
            }
            Ok(())
        }
        pub fn walk(
            &mut self,
            d: &Drawing,
            host: &Path,
            h: &file::IoHooks,
            depth: usize,
            seen: &mut BTreeSet<PathBuf>,
            qa: &mut Vec<Qa>,
        ) -> Checked<()> {
            if depth > 16 {
                return Err("reference_limit: reference depth exceeds 16".into());
            }
            let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
            let images =
                stores.flat_map(|st| st.iter()).filter_map(|e| if let EntityKind::Image(i) = &e.kind { Some((i.path.clone(), false)) } else { None });
            let xrefs = d.blocks.values().filter_map(|b| b.xref_path.as_ref().map(|p| (p.clone(), true)));
            let refs: Vec<_> = xrefs.chain(images).take(MAX_FILES + 1).collect();
            if refs.len() > MAX_FILES {
                return Err("reference_limit: too many references".into());
            }
            for (name, drawing) in refs {
                let Some(path) = super::super::blocks::xref::find_file(&name, Some(&host.to_string_lossy())) else {
                    qa.push(Qa::new(Severity::Error, "broken_reference", name, "Referenced file cannot be found"));
                    continue;
                };
                self.add(&path)?;
                let path = path.canonicalize().map_err(|e| e.to_string())?;
                if drawing && seen.insert(path.clone()) {
                    match read_drawing(&path, h) {
                        Ok(next) => self.walk(&next, &path, h, depth + 1, seen, qa)?,
                        Err(error) => {
                            qa.push(Qa::new(Severity::Error, "check_unavailable", name, &format!("Could not inspect referenced drawing: {error}")))
                        }
                    }
                }
            }
            Ok(())
        }
    }
    pub fn safe_source(path: &Path) -> Checked<()> {
        for ancestor in path.ancestors() {
            files::reject_link(ancestor)?;
        }
        let meta = fs::metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("unsafe_path: expected a regular file".into());
        }
        Ok(())
    }
    pub fn read_drawing(path: &Path, h: &file::IoHooks) -> Checked<Drawing> {
        safe_source(path)?;
        let bytes = files::read(path, 32 << 20)?;
        let d = (h.read)(&bytes, &path.to_string_lossy())?;
        if d.entity_count().saturating_add(d.blocks.values().map(|b| b.entities.len()).sum::<usize>()) > 4_000_000 {
            return Err("source_limit: drawing entity limit".into());
        }
        Ok(d)
    }
    pub fn output_root(value: &str) -> Checked<PathBuf> {
        let p = PathBuf::from(value);
        let p = if p.is_absolute() { p } else { std::env::current_dir().map_err(|e| e.to_string())?.join(p) };
        paths::absolute(&p.to_string_lossy()).map_err(|e| format!("{}: {}", e.code, e.message))?;
        for a in p.ancestors() {
            files::reject_link(a)?;
        }
        if p.exists() && !p.is_dir() {
            return Err("unsafe_path: output root must be a folder".into());
        }
        Ok(p)
    }
    pub fn existing(root: &Path) -> Checked<BTreeSet<String>> {
        let mut names = BTreeSet::new();
        if !root.exists() {
            return Ok(names);
        }
        for entry in fs::read_dir(root).map_err(|e| e.to_string())?.take(100_001) {
            let entry = entry.map_err(|e| e.to_string())?;
            names.insert(paths::case_fold(&entry.file_name().to_string_lossy()));
            if names.len() > 100_000 {
                return Err("output_limit: too many output directory entries".into());
            }
        }
        Ok(names)
    }
    pub fn versioned(name: &str, occupied: &BTreeSet<String>) -> Checked<String> {
        let (stem, ext) = name.rsplit_once('.').ok_or("unsafe_path: output extension missing")?;
        for n in 1..=10_000 {
            let name = if n == 1 { name.to_string() } else { format!("{stem}_v{n}.{ext}") };
            paths::component(&name).map_err(|e| format!("{}: {}", e.code, e.message))?;
            if !occupied.contains(&paths::case_fold(&name)) {
                return Ok(name);
            }
        }
        Err("output_limit: no available versioned name".into())
    }
    fn substitute(value: &str, values: &BTreeMap<String, String>, qa: &mut Vec<Qa>, subject: &str, mtext: bool) -> Checked<String> {
        let mut out = String::new();
        let mut rest = value;
        while let Some(at) = rest.find("{{") {
            let (head, tail) = rest.split_at(at);
            out.push_str(head);
            let Some(end) = tail.find("}}") else {
                rest = tail;
                break;
            };
            let key = tail.get(2..end).unwrap_or_default();
            if let Some(v) = values.get(key) {
                let replaced = if mtext { cadcraft_doc::mtext_literal(v) } else { v.clone() };
                if out.len().saturating_add(replaced.len()) > 1 << 20 {
                    return Err("text_limit: expanded text exceeds 1 MiB".into());
                }
                out.push_str(&replaced);
            } else {
                if qa.len() >= 10_000 {
                    return Err("qa_limit: too many QA records".into());
                }
                qa.push(Qa::new(Severity::Warning, "unresolved_placeholder", format!("{subject}: {key}"), "Placeholder has no value"));
                out.push_str(tail.get(..end + 2).unwrap_or_default());
            }
            rest = tail.get(end + 2..).unwrap_or_default();
        }
        if out.len().saturating_add(rest.len()) > 1 << 20 {
            return Err("text_limit: expanded text exceeds 1 MiB".into());
        }
        out.push_str(rest);
        Ok(out)
    }
    pub fn prepare(d: &mut Drawing, spec: &FigureSpec, site: &str, event: &str, qa: &mut Vec<Qa>) -> Checked<Option<String>> {
        let Some(layout) = d.layout(spec.layout.trim()).map(|l| l.name.clone()) else {
            qa.push(Qa::new(Severity::Error, "layout_missing", &spec.layout, "Requested layout does not exist"));
            return Ok(None);
        };
        for name in spec.visible_layers.iter().chain(&spec.required_layers) {
            if d.layer(name).is_none() {
                qa.push(Qa::new(Severity::Error, "layer_missing", name, "Visible or required layer does not exist"));
            }
        }
        for name in &spec.required_layers {
            let count = std::iter::once(&d.model)
                .chain(d.layouts.iter().filter(|l| l.name == layout).map(|l| &l.entities))
                .chain(d.blocks.values().map(|b| &b.entities))
                .any(|st| st.iter().any(|e| e.common.layer.eq_ignore_ascii_case(name)));
            if d.layer(name).is_some() && !count {
                qa.push(Qa::new(Severity::Warning, "required_layer_empty", name, "Required layer contains no entities"));
            }
        }
        for layer in &mut d.layers {
            if spec.visible_layers.iter().any(|n| n.eq_ignore_ascii_case(&layer.name)) {
                layer.on = true;
                layer.frozen = false;
            }
            if spec.hidden_layers.iter().any(|n| n.eq_ignore_ascii_case(&layer.name)) {
                layer.on = false;
            }
        }
        let mut values = spec.text.clone();
        values.insert("Site".into(), site.into());
        values.insert("Event".into(), event.into());
        values.insert("FigureId".into(), spec.figure_id.clone());
        let bounds = spec.extent_boundary_layer.as_ref().map(|name| {
            let mut bounds = Bounds2::EMPTY;
            let mut cache = cadcraft_doc::BlockBoundsCache::default();
            for e in d.model.iter().filter(|e| e.common.layer.eq_ignore_ascii_case(name)) {
                bounds = bounds.union(&cadcraft_doc::entity_bounds_cached(d, e, 0, &mut cache));
            }
            if d.layer(name).is_none() {
                qa.push(Qa::new(Severity::Warning, "boundary_layer_missing", name, "Saved viewport view retained"));
            } else if bounds.is_empty() {
                qa.push(Qa::new(Severity::Warning, "boundary_layer_empty", name, "Saved viewport view retained"));
            }
            bounds
        });
        let selected = d.layout(&layout).and_then(|l| {
            l.entities
                .iter()
                .filter_map(|e| match &e.kind {
                    EntityKind::Viewport(v) if v.id != 1 && v.width.is_finite() && v.height.is_finite() && v.width > 0. && v.height > 0. => {
                        Some((e.handle, v.width * v.height))
                    }
                    _ => None,
                })
                .max_by(|(ha, a), (hb, b)| a.total_cmp(b).then_with(|| hb.cmp(ha)))
                .map(|(h, _)| h)
        });
        let mut changed = None;
        let l = d.layouts.iter_mut().find(|l| l.name == layout).ok_or("layout_missing: layout disappeared")?;
        for handle in l.entities.handles() {
            let Some(original) = l.entities.get(handle) else {
                continue;
            };
            let mut e = (**original).clone();
            let subject = handle.hex();
            match &mut e.kind {
                EntityKind::Text(t) => t.value = substitute(&t.value, &values, qa, &subject, false)?,
                EntityKind::MText(t) => t.contents = substitute(&t.contents, &values, qa, &subject, true)?,
                EntityKind::Insert(i) => {
                    for a in &mut i.attribs {
                        a.text.value = substitute(&a.text.value, &values, qa, &subject, false)?;
                    }
                }
                EntityKind::Viewport(v) if Some(handle) == selected => {
                    if let Some(b) = bounds.filter(|b| !b.is_empty()) {
                        let pad = 1. + 2. * spec.extent_buffer_pct / 100.;
                        let (_, w, h) = crate::View::twisted_box(b, v.view_twist());
                        let height = (h.max(w * v.height / v.width) * pad).max(1e-9);
                        let center = b.center();
                        if height.is_finite() && center.is_finite() {
                            v.view_center = center;
                            v.view_height = height;
                            changed = Some(subject);
                        } else {
                            qa.push(Qa::new(
                                Severity::Warning,
                                "check_unavailable",
                                &spec.layout,
                                "Boundary extent is not finite; saved view retained",
                            ));
                        }
                    }
                }
                _ => {}
            }
            l.entities.replace(e);
        }
        if bounds.is_some_and(|b| !b.is_empty()) && selected.is_none() {
            qa.push(Qa::new(Severity::Warning, "check_unavailable", &spec.layout, "No usable content viewport; saved view retained"));
        }
        Ok(changed)
    }
    pub fn plot_session(mut d: Drawing, path: &Path, qa: &mut Vec<Qa>) -> Checked<Session> {
        for outcome in super::super::blocks::xref::load::load_all(&mut d, Some(&path.to_string_lossy())) {
            if outcome.status != XrefStatus::Loaded {
                qa.push(Qa::new(Severity::Error, "broken_reference", outcome.name, &outcome.detail));
            }
        }
        if !d.read_notes.is_empty() || !d.skipped_records.is_empty() {
            qa.push(Qa::new(
                Severity::Warning,
                "check_unavailable",
                path.to_string_lossy(),
                "Source contains content the reader could not fully inspect",
            ));
        }
        let mut session = Session::new();
        let i = session.open_drawing(d, "Outgoing", Some(path.to_string_lossy().into_owned()));
        super::super::image::after_open(&mut session, i);
        for note in session.log.iter().filter(|n| n.starts_with("Image") || n.starts_with("Images:")) {
            qa.push(Qa::new(Severity::Warning, "check_unavailable", path.to_string_lossy(), note));
        }
        Ok(session)
    }
    pub fn plot_options(s: &Session, layout: &str, format: &str) -> Checked<Value> {
        super::super::plotters::plot_options(
            s,
            &json!({"layout":layout,"output":format,"device":if format=="png"{"CADCraft PNG"}else{"CADCraft PDF"}}),
            "plot",
        )
        .map_err(|e| e.to_string())
    }
    pub fn plot(s: &mut Session, options: &Value, path: &Path) -> Checked<()> {
        let mut p = options.clone();
        p["path"] = json!(path);
        s.execute("plot", &p).map_err(|e| e.to_string())?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        use std::io::Read;
        let mut prefix = [0u8; 8];
        let n = file.take(8).read(&mut prefix).map_err(|e| e.to_string())?;
        let format = options.get("output").and_then(Value::as_str).unwrap_or("pdf");
        if !(format == "pdf" && prefix.get(..5) == Some(b"%PDF-") || format == "png" && n == 8 && prefix == *b"\x89PNG\r\n\x1a\n") {
            return Err("verification_failed: plot output has wrong format".into());
        }
        Ok(())
    }
    fn build(r: &Request, h: &file::IoHooks) -> Checked<(Value, Vec<Loaded>, References)> {
        if r.spec_paths.is_empty() || r.spec_paths.len() > figures::MAX_JOBS {
            return Err("invalid_input: expected 1..1000 spec paths".into());
        }
        let root = output_root(&r.output_folder)?;
        let mut refs = References::new();
        let mut loaded = Vec::new();
        let mut seen = BTreeSet::new();
        for name in &r.spec_paths {
            safe_source(Path::new(name))?;
            let path = Path::new(name).canonicalize().map_err(|e| e.to_string())?;
            if !seen.insert(path.clone()) {
                continue;
            }
            refs.add(&path)?;
            let spec = FigureSpec::parse(&files::read(&path, figures::MAX_SPEC_BYTES)?).map_err(|e| format!("{}: {}", e.code, e.message))?;
            let source = path.parent().ok_or("unsafe_path: spec parent missing")?.join(&spec.source_drawing);
            safe_source(&source)?;
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            refs.add(&source)?;
            loaded.push(Loaded { spec, source });
        }
        let specs: Vec<_> = loaded.iter().map(|s| s.spec.clone()).collect();
        let jobs = figures::series(&specs, &r.sites, &r.events, &r.mode, &r.format).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let disk = existing(&root)?;
        let mut reserved = disk.clone();
        let mut rows = Vec::new();
        for job in jobs {
            let input = loaded.get(job.spec_index).ok_or("invalid_spec: missing spec")?;
            let d = read_drawing(&input.source, h)?;
            let mut qa = Vec::new();
            refs.walk(&d, &input.source, h, 0, &mut BTreeSet::from([input.source.clone()]), &mut qa)?;
            let mut session = plot_session(d, &input.source, &mut qa)?;
            let viewport = prepare(session.doc_mut().map_err(|e| e.to_string())?, &input.spec, &job.site, &job.event, &mut qa)?;
            let opts = plot_options(&session, &input.spec.layout, &r.format)?;
            let filename = versioned(&job.filename, &disk)?;
            if !reserved.insert(paths::case_fold(&filename)) {
                return Err("output_collision: versioned outputs collide within batch".into());
            }
            rows.push(json!({"spec_index":job.spec_index,"site":job.site,"event":job.event,"figure_id":input.spec.figure_id,"source_drawing":input.source,"layout":input.spec.layout,"filename":filename,"qa":qa,"viewport":viewport,"plot_options":opts}));
        }
        refs.check()?;
        let plan = json!({"version":1,"command":"figureexport","output_folder":root,"mode":r.mode,"format":r.format,"jobs":rows,"fingerprints":refs.fingerprints});
        Ok((plan, loaded, refs))
    }
    pub fn run(p: &Value, mover: &Mover<'_>) -> Checked<Value> {
        if serde_json::to_vec(p).map_err(|e| e.to_string())?.len() > 4 << 20 {
            return Err("invalid_input: inputs exceed 4 MiB".into());
        }
        let r: Request = serde_json::from_value(p.clone()).map_err(|e| format!("invalid_input: {e}"))?;
        if !["plan", "preview", "execute", "create"].contains(&r.action.as_str()) {
            return Err("invalid_input: action must be plan, preview or execute".into());
        }
        let h = file::io().ok_or("plot_unavailable: drawing formats unavailable")?;
        let (plan, loaded, refs) = build(&r, h)?;
        let blocked = plan.get("jobs").and_then(Value::as_array).is_some_and(|jobs| {
            jobs.iter().any(|j| {
                j.get("qa").and_then(Value::as_array).is_some_and(|qa| qa.iter().any(|q| q.get("severity").and_then(Value::as_str) == Some("error")))
            })
        });
        if blocked {
            return Ok(json!({"plan":plan,"hash":null,"issues":[{"code":"export_blocked","message":"Pre-export QA refused this batch"}]}));
        }
        let hash = files::hash(&plan)?;
        if r.action == "plan" || r.action == "preview" {
            return Ok(json!({"plan":plan,"hash":hash,"issues":[]}));
        }
        if let Some(refused) = files::approval_failure(&hash, r.approved_hash.as_deref()) {
            return Ok(refused);
        }
        let root = output_root(&r.output_folder)?;
        let created = !root.exists();
        // Only one new output folder is created; a typo cannot create an arbitrary directory tree.
        if created {
            fs::create_dir(&root).map_err(|e| e.to_string())?;
        }
        let result = execute(&plan, &loaded, &refs, &root, h, mover);
        if created && !result.as_ref().is_ok_and(|v| v["receipt"]["outcome"] == "Succeeded") {
            remove_empty_output_root(&root);
        }
        result
    }
    fn remove_empty_output_root(root: &Path) {
        if no_named_streams(root).is_ok() {
            let _ = fs::remove_dir(root);
        }
    }
    fn no_named_streams(path: &Path) -> Checked<()> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            // Bound the dependency's UTF-16 allocation and prevent Win32 NUL truncation.
            if path.as_os_str().encode_wide().take(32_768).count() >= 32_768 || path.as_os_str().encode_wide().any(|c| c == 0) {
                return Err("stream_inspection_failed: path is invalid or too long".into());
            }
            let failed = |e| format!("stream_inspection_failed: cannot inspect {}: {e}", path.display());
            // Rollback must not delete caller-added streams that primary-byte hashes omit.
            // The lazy iterator detects even empty named streams and skips only unnamed $DATA.
            let mut streams = match fsquirrel::list(path) {
                Ok(streams) => streams,
                // An ordinary directory has no unnamed $DATA stream. Only its documented
                // ERROR_HANDLE_EOF result is empty; all other inspection failures retain it.
                Err(e) if e.raw_os_error() == Some(0x8007_0026_u32.cast_signed()) && path.is_dir() => return Ok(()),
                Err(e) => return Err(failed(e)),
            };
            if streams.next().transpose().map_err(failed)?.is_some() {
                return Err(format!("named_stream: {} has a named stream not covered by the primary-byte hash", path.display()));
            }
        }
        #[cfg(not(windows))]
        let _ = path;
        Ok(())
    }
    #[cfg(all(test, windows))]
    mod stream_tests {
        use super::{files, no_named_streams, remove_empty_output_root};
        use std::{fs, path::Path};

        #[test]
        fn empty_output_root_without_streams_is_removed() {
            let root = std::env::temp_dir().join(format!("cadcraft-figure-root-{}", files::run_id()));
            fs::create_dir(&root).unwrap();
            remove_empty_output_root(&root);
            assert!(!root.exists());
        }

        #[test]
        fn empty_output_root_with_named_stream_is_retained() {
            check_retained_root(b"caller-added data").unwrap();
        }

        #[test]
        fn empty_output_root_with_empty_named_stream_is_retained() {
            check_retained_root(b"").unwrap();
        }

        fn check_retained_root(contents: &[u8]) -> std::io::Result<()> {
            let root = std::env::temp_dir().join(format!("cadcraft-figure-root-{}", files::run_id()));
            fs::create_dir(&root)?;
            let mut stream = root.as_os_str().to_os_string();
            stream.push(":additional");
            fs::write(&stream, contents)?;
            remove_empty_output_root(&root);
            let retained = fs::read(&stream);
            if root.exists() {
                fs::remove_dir(&root)?;
            }
            assert_eq!(retained?, contents);
            Ok(())
        }

        #[test]
        fn unavailable_stream_inspection_is_an_error() {
            let missing = std::env::temp_dir().join(format!("cadcraft-figure-missing-{}", files::run_id()));
            assert!(no_named_streams(&missing).unwrap_err().starts_with("stream_inspection_failed:"));
        }

        #[test]
        fn stream_inspection_rejects_nul_and_overlong_paths() {
            for path in ["file\0hidden".to_string(), "x".repeat(32_768)] {
                assert!(no_named_streams(Path::new(&path)).unwrap_err().starts_with("stream_inspection_failed:"));
            }
        }
    }
    fn execute(plan: &Value, loaded: &[Loaded], refs: &References, root: &Path, h: &file::IoHooks, mover: &Mover<'_>) -> Checked<Value> {
        let _lock = files::lock(root)?;
        let id = files::run_id();
        let hash = files::hash(plan)?;
        let stage = files::child(root, &format!(".figureexport-{id}.stage"))?;
        fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let mut published: Vec<(PathBuf, String)> = Vec::new();
        let mut artifacts = Vec::new();
        let receipt_path = files::child(root, &format!("figureexport.{id}.receipt.json"))?;
        let result = (|| -> Checked<Value> {
            let jobs = plan.get("jobs").and_then(Value::as_array).ok_or("invalid_plan: jobs missing")?;
            for job in jobs {
                let i =
                    job.get("spec_index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).ok_or("invalid_plan: spec index missing")?;
                let input = loaded.get(i).ok_or("invalid_plan: spec missing")?;
                let d = read_drawing(&input.source, h)?;
                let mut qa = Vec::new();
                let mut session = plot_session(d, &input.source, &mut qa)?;
                prepare(
                    session.doc_mut().map_err(|e| e.to_string())?,
                    &input.spec,
                    job.get("site").and_then(Value::as_str).unwrap_or_default(),
                    job.get("event").and_then(Value::as_str).unwrap_or_default(),
                    &mut qa,
                )?;
                if qa.iter().any(|q| q.severity == Severity::Error) {
                    return Err("export_blocked: reference changed after planning".into());
                }
                let name = job.get("filename").and_then(Value::as_str).ok_or("invalid_plan: filename missing")?;
                let path = files::child(&stage, name)?;
                plot(&mut session, job.get("plot_options").ok_or("invalid_plan: plot options missing")?, &path)?;
                let digest = files::fingerprint(&path)?;
                artifacts.push(json!({"path":root.join(name),"sha256":digest,"qa":job.get("qa"),"site":job.get("site"),"event":job.get("event"),"figure_id":job.get("figure_id")}));
            }
            refs.check()?;
            for artifact in &artifacts {
                let target = PathBuf::from(artifact.get("path").and_then(Value::as_str).ok_or("invalid_plan: target missing")?);
                let name = target.file_name().and_then(|n| n.to_str()).ok_or("unsafe_path: filename missing")?;
                files::vacant(root, name)?;
                mover(&stage.join(name), &target)?;
                published.push((target.clone(), artifact.get("sha256").and_then(Value::as_str).unwrap_or_default().into()));
                if files::fingerprint(&target)? != artifact.get("sha256").and_then(Value::as_str).unwrap_or_default() {
                    return Err("verification_failed: published hash mismatch".into());
                }
            }
            refs.check()?;
            let receipt =
                json!({"version":1,"command":"figureexport","outcome":"Succeeded","plan_sha256":hash,"jobs":artifacts,"receipt_path":receipt_path});
            files::write_new(&receipt_path, &files::encoded(&receipt)?)?;
            Ok(receipt)
        })();
        let mut cleanup = Vec::new();
        if result.is_err() {
            for (path, digest) in published.iter().rev() {
                match files::fingerprint(path) {
                    Ok(actual) if actual == *digest && no_named_streams(path).is_ok() => {
                        if let Err(e) = fs::remove_file(path) {
                            cleanup.push(e.to_string());
                        }
                    }
                    _ => cleanup.push(format!("Retained changed published artifact {}", path.display())),
                }
            }
        }
        if let Err(e) = fs::remove_dir_all(&stage) {
            cleanup.push(e.to_string());
        }
        Ok(match result {
            Ok(receipt) => json!({"receipt":receipt}),
            Err(error) => {
                let mut receipt = json!({"version":1,"command":"figureexport","outcome":"Failed","plan_sha256":hash,"failed_checks":[{"code":"execution_failed","message":error}],"cleanup_failures":cleanup,"receipt_path":receipt_path});
                if let Err(e) = files::encoded(&receipt).and_then(|b| files::write_new(&receipt_path, &b)) {
                    receipt["reporting_failure"] = json!(e);
                    receipt["receipt_path"] = Value::Null;
                }
                json!({"receipt":receipt})
            }
        })
    }
}
