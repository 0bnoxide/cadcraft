//! Explicit settings diffs; no entity edits or unnamed setting changes.
use super::{CommandSpec, bad};
use crate::{Result, Session};
use serde_json::Value;
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("configuredrawing", "Configure Drawing", run)
            .menu(&["File", "Configure Drawing"])
            .params("{action:plan|create,proposal_root,manifest_path,target:relative_drawing_path,approved_hash?}")
            .noundo()
            .interactive(|_| Ok(super::proposal::follow_prompt("configuredrawing"))),
    ]
}
fn run(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("configuredrawing", "Project filesystem editing is not available on web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        run_with_hooks(p, super::file::io().ok_or_else(|| bad("configuredrawing", "Drawing formats are not available"))?)
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_with_hooks(p: &Value, h: &super::file::IoHooks) -> Result<Value> {
    Ok(native::run(p, h).unwrap_or_else(super::proposal::files::issue))
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::super::proposal::files::{self, Checked, Project};
    use super::Value;
    use cadcraft_color::Color;
    use cadcraft_doc::{AnnoScale, DimStyle, Drawing, Layer, Lineweight, TextStyle};
    use cadcraft_proposal::settings::DrawingSettings;
    use serde_json::json;
    use std::{fs, path::Path};

    fn changed(actions: &mut Vec<Value>, kind: &str, name: &str, field: &str, current: Value, planned: Value) {
        if !files::same(&current, &planned) {
            actions.push(json!({"kind":kind,"name":name,"field":field,"current":current,"planned":planned}));
        }
    }
    pub(super) fn configure(d: &Drawing, s: &DrawingSettings) -> Checked<(Drawing, Vec<Value>)> {
        let mut d = d.clone();
        let mut actions = Vec::new();
        for (name, value) in &s.headers {
            changed(&mut actions, "set_header", name, "value", json!(d.header.get(name)), json!(value));
            d.header.set_i64(name, *value);
        }
        configure_layers(&mut d, s, &mut actions)?;
        configure_text_styles(&mut d, s, &mut actions)?;
        configure_dimension_styles(&mut d, s, &mut actions)?;
        configure_annotation_scales(&mut d, s, &mut actions)?;
        Ok((d, actions))
    }
    fn configure_layers(d: &mut Drawing, s: &DrawingSettings, actions: &mut Vec<Value>) -> Checked<()> {
        for patch in &s.layers {
            if let Some(linetype) = &patch.linetype
                && !d.linetypes.iter().any(|l| l.name.eq_ignore_ascii_case(linetype))
            {
                return Err("missing_dependency: layer linetype does not exist".into());
            }
            let existing = d.layers.iter().position(|l| l.name.eq_ignore_ascii_case(&patch.name));
            let mut layer = existing.and_then(|i| d.layers.get(i)).cloned().unwrap_or_else(|| Layer::new(&patch.name));
            let before = layer.clone();
            if let Some(v) = patch.color {
                layer.color = Color::Index(v);
            }
            if let Some(v) = &patch.linetype {
                layer.linetype = d
                    .linetypes
                    .iter()
                    .find(|l| l.name.eq_ignore_ascii_case(v))
                    .map(|l| l.name.clone())
                    .ok_or("missing_dependency: linetype missing")?;
            }
            if let Some(v) = patch.lineweight {
                layer.lineweight = Lineweight::from_dxf(v);
            }
            if let Some(i) = existing {
                if patch.color.is_some() {
                    changed(actions, "set_layer", &layer.name, "color", json!(before.color), json!(layer.color));
                }
                if patch.linetype.is_some() {
                    changed(actions, "set_layer", &layer.name, "linetype", json!(before.linetype), json!(layer.linetype));
                }
                if patch.lineweight.is_some() {
                    changed(actions, "set_layer", &layer.name, "lineweight", json!(before.lineweight), json!(layer.lineweight));
                }
                *d.layers.get_mut(i).ok_or("invalid_settings: layer disappeared")? = layer;
            } else {
                actions.push(json!({"kind":"create_layer","name":layer.name,"current":null,"planned":layer}));
                d.layers.push(layer);
            }
        }
        Ok(())
    }
    fn configure_text_styles(d: &mut Drawing, s: &DrawingSettings, actions: &mut Vec<Value>) -> Checked<()> {
        for patch in &s.text_styles {
            let existing = d.text_styles.iter().position(|v| v.name.eq_ignore_ascii_case(&patch.name));
            let mut style =
                existing.and_then(|i| d.text_styles.get(i)).cloned().unwrap_or_else(|| TextStyle { name: patch.name.clone(), ..Default::default() });
            macro_rules! set {
                ($field:ident) => {
                    if let Some(v) = &patch.$field {
                        if existing.is_some() {
                            changed(actions, "set_text_style", &style.name, stringify!($field), json!(style.$field), json!(v));
                        }
                        style.$field = v.clone();
                    }
                };
            }
            set!(font);
            set!(height);
            set!(width_factor);
            set!(oblique);
            set!(annotative);
            if let Some(i) = existing {
                *d.text_styles.get_mut(i).ok_or("invalid_settings: style disappeared")? = style;
            } else {
                actions.push(json!({"kind":"create_text_style","name":style.name,"current":null,"planned":style}));
                d.text_styles.push(style);
            }
        }
        Ok(())
    }
    fn configure_dimension_styles(d: &mut Drawing, s: &DrawingSettings, actions: &mut Vec<Value>) -> Checked<()> {
        for patch in &s.dimension_styles {
            let text_style = patch
                .text_style
                .as_ref()
                .map(|name| d.text_style(name).map(|s| s.name.clone()).ok_or("missing_dependency: dimension text style does not exist"))
                .transpose()?;
            let existing = d.dim_styles.iter().position(|v| v.name.eq_ignore_ascii_case(&patch.name));
            let mut style =
                existing.and_then(|i| d.dim_styles.get(i)).cloned().unwrap_or_else(|| DimStyle { name: patch.name.clone(), ..Default::default() });
            macro_rules! set {
                ($field:ident) => {
                    if let Some(v) = &patch.$field {
                        if existing.is_some() {
                            changed(actions, "set_dimension_style", &style.name, stringify!($field), json!(style.$field), json!(v));
                        }
                        style.$field = v.clone();
                    }
                };
            }
            if let Some(name) = text_style {
                if existing.is_some() {
                    changed(actions, "set_dimension_style", &style.name, "text_style", json!(style.text_style), json!(name));
                }
                style.text_style = name;
            }
            set!(scale);
            set!(arrow_size);
            set!(text_height);
            set!(decimals);
            set!(angular_decimals);
            set!(annotative);
            if let Some(i) = existing {
                *d.dim_styles.get_mut(i).ok_or("invalid_settings: style disappeared")? = style;
            } else {
                actions.push(json!({"kind":"create_dimension_style","name":style.name,"current":null,"planned":style}));
                d.dim_styles.push(style);
            }
        }
        Ok(())
    }
    fn configure_annotation_scales(d: &mut Drawing, s: &DrawingSettings, actions: &mut Vec<Value>) -> Checked<()> {
        for patch in &s.annotation_scales {
            let scale = AnnoScale::new(&patch.name, patch.paper_units, patch.drawing_units);
            if let Some(existing) = d.scales.iter_mut().find(|v| v.name.eq_ignore_ascii_case(&patch.name)) {
                let scale = AnnoScale { name: existing.name.clone(), ..scale };
                changed(actions, "set_annotation_scale", &scale.name, "scale", json!(existing), json!(scale));
                *existing = scale;
            } else {
                if d.scales.len() >= cadcraft_doc::MAX_SCALES {
                    return Err("invalid_settings: annotation scale capacity exceeded".into());
                }
                actions.push(json!({"kind":"set_annotation_scale","name":scale.name,"current":null,"planned":scale}));
                d.scales.push(scale);
            }
        }
        Ok(())
    }
    pub fn run(p: &Value, h: &super::super::file::IoHooks) -> Checked<Value> {
        let action = p.get("action").and_then(Value::as_str).unwrap_or("plan");
        if !matches!(action, "plan" | "create") {
            return Err("invalid_input: action must be plan or create".into());
        }
        let project = Project::open(p)?;
        let relative = p.get("target").and_then(Value::as_str).ok_or("invalid_input: target required")?;
        let portable = cadcraft_proposal::paths::relative(relative).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let key = cadcraft_proposal::paths::case_fold(&portable);
        let sources = cadcraft_proposal::paths::case_fold(project.manifest.source_location());
        if portable.split('/').any(|part| part.eq_ignore_ascii_case("_superseded")) || key == sources || key.starts_with(&format!("{sources}/")) {
            return Err("protected_target: intake copies and retained versions are immutable".into());
        }
        let target = files::child(&project.root, relative)?;
        if !Path::new(relative).extension().is_some_and(|e| e.eq_ignore_ascii_case("dxf") || e.eq_ignore_ascii_case("dwg")) {
            return Err("invalid_input: target must be DXF or DWG".into());
        }
        let (before, prior_hash) = files::drawing(h, &target)?;
        let settings = project.manifest.drawing_settings().ok_or("manifest_v2_required: drawing settings missing")?;
        let (wanted, actions) = configure(&before, settings)?;
        files::roundtrip(h, &wanted, &target)?;
        let plan = json!({"version":1,"command":"configuredrawing","proposal_root":project.root,"target":relative,"target_sha256":prior_hash,"fingerprints":project.fingerprints,"settings":settings,"actions":actions});
        let hash = files::hash(&plan)?;
        if action == "plan" {
            return Ok(json!({"plan":plan,"hash":hash,"issues":[]}));
        }
        if actions.is_empty() {
            return Ok(json!({"receipt":{"outcome":"NoChanges","plan_sha256":hash}}));
        }
        if let Some(failure) = files::approval_failure(&hash, p.get("approved_hash").and_then(Value::as_str)) {
            return Ok(failure);
        }
        let _lock = files::lock(&project.root)?;
        let id = files::run_id();
        let stage = files::stage_path(&target, "configuredrawing", &id)?;
        let receipt_path = files::receipt_path(&project, "configuredrawing", &id)?;
        let mut swapped = None;
        let mut owned_stage = false;
        let result = (|| -> Checked<Value> {
            let bytes = (h.write)(&wanted, &target.to_string_lossy())?;
            files::write_new(&stage, &bytes)?;
            owned_stage = true;
            let actual = (h.read)(&files::read(&stage, 32 << 20)?, &target.to_string_lossy())?;
            if !configure(&actual, settings)?.1.is_empty() || !files::same_drawing(&actual, &wanted) {
                return Err("verification_failed: settings or entity contents differ after reopening".into());
            }
            project.check(p)?;
            swapped = Some(files::swap(&target, &stage, &prior_hash, &files::rename_no_replace)?);
            let receipt = json!({"version":1,"command":"configuredrawing","outcome":"Succeeded","plan_sha256":hash,"drawing":swapped,"actions":actions,"receipt_path":receipt_path});
            files::write_new(&receipt_path, &files::encoded(&receipt)?)?;
            Ok(receipt)
        })();
        let mut cleanup = Vec::new();
        if result.is_err()
            && let Some(record) = &swapped
            && let Err(e) = files::restore(&target, record, &files::rename_no_replace)
        {
            cleanup.push(e);
        }
        if owned_stage
            && let Err(e) = fs::remove_file(&stage)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            cleanup.push(e.to_string());
        }
        Ok(match result {
            Ok(receipt) => json!({"receipt":receipt}),
            Err(error) => files::failed_receipt(&receipt_path, "configuredrawing", &hash, error, cleanup),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::native::configure;
    use cadcraft_doc::{Drawing, Layer, TextStyle};
    use cadcraft_proposal::settings::DrawingSettings;
    use serde_json::json;

    #[test]
    fn patches_preserve_unnamed_fields_styles_scales_and_entities() {
        let mut d = Drawing::new_metric();
        let mut layer = Layer::new("Survey");
        layer.locked = true;
        layer.description = "keep".into();
        d.layers.push(layer);
        let mut style = TextStyle { name: "Notes".into(), ..Default::default() };
        style.backwards = true;
        d.text_styles.push(style);
        let original = d.clone();
        let settings:DrawingSettings=serde_json::from_value(json!({"layers":[{"name":"Survey","color":3,"lineweight":25}],"text_styles":[{"name":"Notes","height":2.5}],"dimension_styles":[{"name":"Dims","text_style":"notes","decimals":3}],"annotation_scales":[{"name":"1:123","paper_units":1,"drawing_units":123}]})).unwrap();
        let (changed, actions) = configure(&d, &settings).unwrap();
        assert!(!actions.is_empty());
        assert_eq!(changed.header, original.header);
        assert!(changed.layer("Survey").unwrap().locked);
        assert_eq!(changed.layer("Survey").unwrap().description, "keep");
        assert!(changed.text_style("Notes").unwrap().backwards);
        assert_eq!(changed.text_style("Notes").unwrap().height, 2.5);
        assert_eq!(changed.dim_style("Dims").unwrap().text_style, "Notes");
        for scale in original.scales {
            assert!(changed.scales.contains(&scale));
        }
        assert!(configure(&changed, &settings).unwrap().1.is_empty());
        let invalid: DrawingSettings = serde_json::from_value(json!({"layers":[{"name":"Survey","linetype":"absent"}]})).unwrap();
        assert!(configure(&d, &invalid).unwrap_err().starts_with("missing_dependency"));
        let invalid: DrawingSettings = serde_json::from_value(json!({"dimension_styles":[{"name":"Dims","text_style":"absent"}]})).unwrap();
        assert!(configure(&d, &invalid).unwrap_err().starts_with("missing_dependency"));
    }
}
