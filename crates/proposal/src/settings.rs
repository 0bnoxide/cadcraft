//! The deliberately small, closed set of settings a follow-on may name.
use crate::{Issue, manifest::text, paths};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DrawingSettings {
    pub headers: BTreeMap<String, i64>,
    pub layers: Vec<LayerSettings>,
    pub text_styles: Vec<TextSettings>,
    pub dimension_styles: Vec<DimensionSettings>,
    pub annotation_scales: Vec<ScaleSettings>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerSettings {
    pub name: String,
    pub color: Option<u8>,
    pub linetype: Option<String>,
    pub lineweight: Option<i16>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSettings {
    pub name: String,
    pub font: Option<String>,
    pub height: Option<f64>,
    pub width_factor: Option<f64>,
    pub oblique: Option<f64>,
    pub annotative: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionSettings {
    pub name: String,
    pub text_style: Option<String>,
    pub scale: Option<f64>,
    pub arrow_size: Option<f64>,
    pub text_height: Option<f64>,
    pub decimals: Option<u8>,
    pub angular_decimals: Option<u8>,
    pub annotative: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleSettings {
    pub name: String,
    pub paper_units: f64,
    pub drawing_units: f64,
}

impl DrawingSettings {
    pub fn validate(&self) -> Result<(), Issue> {
        let invalid = || Issue::new("invalid_settings", "Invalid or unsupported drawing setting");
        for (name, value) in &self.headers {
            let range = match name.as_str() {
                "INSUNITS" => 0..=24,
                "MEASUREMENT" => 0..=1,
                "LUNITS" => 1..=5,
                "AUNITS" => 0..=4,
                "LUPREC" | "AUPREC" => 0..=8,
                _ => return Err(invalid()),
            };
            if !range.contains(value) {
                return Err(invalid());
            }
        }
        let groups: Vec<Vec<&str>> = vec![
            self.layers.iter().map(|s| s.name.as_str()).collect(),
            self.text_styles.iter().map(|s| s.name.as_str()).collect(),
            self.dimension_styles.iter().map(|s| s.name.as_str()).collect(),
            self.annotation_scales.iter().map(|s| s.name.as_str()).collect(),
        ];
        for names in groups {
            let mut seen = std::collections::BTreeSet::new();
            if names.len() > 1000 || names.iter().any(|n| !text(n) || n.len() > 255 || !seen.insert(paths::case_fold(n))) {
                return Err(invalid());
            }
        }
        for s in &self.layers {
            paths::component(&s.name)?;
            if s.color == Some(0)
                || s.linetype.as_ref().is_some_and(|v| paths::component(v).is_err())
                || s.lineweight.is_some_and(|v| {
                    v != -3 && ![0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211].contains(&v)
                })
            {
                return Err(invalid());
            }
        }
        for s in &self.text_styles {
            paths::component(&s.name)?;
            if s.font.as_ref().is_some_and(|v| !text(v))
                || s.height.is_some_and(|v| !v.is_finite() || v < 0.0)
                || s.width_factor.is_some_and(|v| !v.is_finite() || v <= 0.0)
                || s.oblique.is_some_and(|v| !v.is_finite() || v.abs() >= std::f64::consts::FRAC_PI_2)
            {
                return Err(invalid());
            }
        }
        for s in &self.dimension_styles {
            paths::component(&s.name)?;
            if s.text_style.as_ref().is_some_and(|v| paths::component(v).is_err())
                || [s.scale, s.text_height].into_iter().flatten().any(|v| !v.is_finite() || v <= 0.0)
                || s.arrow_size.is_some_and(|v| !v.is_finite() || v < 0.0)
                || s.decimals.is_some_and(|v| v > 8)
                || s.angular_decimals.is_some_and(|v| v > 8)
            {
                return Err(invalid());
            }
        }
        for s in &self.annotation_scales {
            if ![s.paper_units, s.drawing_units, s.drawing_units / s.paper_units].iter().all(|v| v.is_finite() && *v > 0.0) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
