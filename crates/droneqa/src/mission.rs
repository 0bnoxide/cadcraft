//! Mission record and accuracy ruleset (JSON, not YAML). Reference: AIGIS3D `models.py` (`Mission`,
//! `normalize_mission`) and `config.py` (`Ruleset`). Owner: Claude (see `docs/agents/slice-3-droneqa.md`).
//!
//! Both are read strictly: unknown keys, wrong types, negative or out-of-range numbers, blank list
//! entries and over-long text are errors. Blank top-level text means "not documented" (`None`).

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Longest text value a mission or ruleset may carry, in characters.
pub const MAX_TEXT: usize = 2000;
/// Most entries a list in a mission or ruleset may carry (hostile-input cap).
pub const MAX_LIST: usize = 10_000;
/// Longest attachment pattern, in bytes; patterns compile under a size limit too.
pub const MAX_PATTERN: usize = 1000;

/// One file attached to a mission record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub id: u64,
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default = "octet_stream")]
    pub content_type: String,
}

fn octet_stream() -> String {
    "application/octet-stream".into()
}

/// The canonical mission record that the QA rules read. `None` means "not documented".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    pub global_id: String,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub mission_id: Option<String>,
    #[serde(default)]
    pub mission_date: Option<String>,
    #[serde(default)]
    pub pilot: Option<String>,
    #[serde(default)]
    pub aircraft: Option<String>,
    #[serde(default)]
    pub sensor: Option<String>,
    #[serde(default)]
    pub site_area_acres: Option<f64>,
    #[serde(default)]
    pub vertical_relief_feet: Option<f64>,
    #[serde(default)]
    pub target_horizontal_accuracy_feet: Option<f64>,
    #[serde(default)]
    pub target_vertical_accuracy_feet: Option<f64>,
    #[serde(default)]
    pub planned_altitude_agl_feet: Option<f64>,
    #[serde(default)]
    pub actual_altitude_agl_feet: Option<f64>,
    #[serde(default)]
    pub planned_speed_mph: Option<f64>,
    #[serde(default)]
    pub actual_speed_mph: Option<f64>,
    #[serde(default)]
    pub front_overlap_percent: Option<f64>,
    #[serde(default)]
    pub side_overlap_percent: Option<f64>,
    #[serde(default)]
    pub planned_front_overlap_percent: Option<f64>,
    #[serde(default)]
    pub planned_side_overlap_percent: Option<f64>,
    #[serde(default)]
    pub flight_pattern: Option<String>,
    #[serde(default)]
    pub planned_flight_pattern: Option<String>,
    #[serde(default)]
    pub course_angle_degrees: Option<f64>,
    #[serde(default)]
    pub terrain_follow: Option<bool>,
    #[serde(default)]
    pub rtk_ppk_status: Option<String>,
    #[serde(default)]
    pub correction_source: Option<String>,
    #[serde(default)]
    pub horizontal_crs: Option<String>,
    #[serde(default)]
    pub horizontal_datum: Option<String>,
    #[serde(default)]
    pub horizontal_units: Option<String>,
    #[serde(default)]
    pub vertical_crs: Option<String>,
    #[serde(default)]
    pub vertical_datum: Option<String>,
    #[serde(default)]
    pub vertical_units: Option<String>,
    #[serde(default)]
    pub geoid_model: Option<String>,
    #[serde(default)]
    pub gcp_count: Option<u64>,
    #[serde(default)]
    pub checkpoint_count: Option<u64>,
    #[serde(default)]
    pub control_accuracy_feet: Option<f64>,
    #[serde(default)]
    pub checkpoint_accuracy_feet: Option<f64>,
    #[serde(default)]
    pub checkpoints_independent: Option<bool>,
    #[serde(default)]
    pub checkpoints_bare_ground: Option<bool>,
    #[serde(default)]
    pub rmse_x_feet: Option<f64>,
    #[serde(default)]
    pub rmse_y_feet: Option<f64>,
    #[serde(default)]
    pub rmse_z_feet: Option<f64>,
    #[serde(default)]
    pub processing_software: Option<String>,
    #[serde(default)]
    pub processing_version: Option<String>,
    #[serde(default)]
    pub planned_image_count: Option<u64>,
    #[serde(default)]
    pub actual_image_count: Option<u64>,
    #[serde(default)]
    pub present_deliverables: Vec<String>,
    #[serde(default)]
    pub intended_deliverables: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// Plain-text fields free of the 2000-character `Text` bound (as in the reference).
const UNBOUNDED_TEXT: [&str; 2] = ["global_id", "mission_date"];
const PERCENT: [&str; 4] = ["front_overlap_percent", "side_overlap_percent", "planned_front_overlap_percent", "planned_side_overlap_percent"];
/// Coordinate documentation fields, in the order the rules report them.
pub const COORDINATE_FIELDS: [&str; 6] =
    ["horizontal_crs", "horizontal_datum", "horizontal_units", "vertical_crs", "vertical_datum", "vertical_units"];

impl Mission {
    /// Every canonical field name.
    pub fn field_names() -> Vec<String> {
        match serde_json::to_value(Mission::default()) {
            Ok(Value::Object(m)) => m.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// Strict read of a canonical mission (the JSON form of `fixtures/mission.json`).
    pub fn from_value(value: &Value) -> Result<Mission, String> {
        let Value::Object(map) = value else { return Err("mission: expected a JSON object".into()) };
        let mut map = map.clone();
        for v in map.values_mut() {
            trim_strings(v);
        }
        // Blank top-level text is "not documented"; blank list entries stay blank and are rejected.
        for v in map.values_mut() {
            if v.as_str() == Some("") {
                *v = Value::Null;
            }
        }
        let mut m: Mission = serde_json::from_value(Value::Object(map)).map_err(|e| format!("mission: {e}"))?;
        m.global_id = canonical_uuid(&m.global_id).ok_or("mission: global_id is not a UUID")?;
        if m.mission_date.as_deref().is_some_and(|d| !is_iso_date(d)) {
            return Err("mission: mission_date is not an ISO date".into());
        }
        m.check()?;
        Ok(m)
    }

    /// The mission as a JSON object, for field lookup by canonical name.
    pub fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }

    fn check(&self) -> Result<(), String> {
        for (name, v) in self.to_map() {
            match &v {
                Value::String(s) if !UNBOUNDED_TEXT.contains(&name.as_str()) => text(&name, s)?,
                Value::Number(n) => {
                    let f = n.as_f64().unwrap_or(f64::NAN);
                    let ok = if PERCENT.contains(&name.as_str()) {
                        (0.0..=100.0).contains(&f)
                    } else if name == "course_angle_degrees" {
                        (0.0..360.0).contains(&f)
                    } else {
                        f.is_finite() && f >= 0.0
                    };
                    if !ok {
                        return Err(format!("mission: {name} is out of range"));
                    }
                }
                Value::Array(a) if a.len() > MAX_LIST => return Err(format!("mission: {name} has too many entries")),
                _ => {}
            }
        }
        for s in self.present_deliverables.iter().chain(&self.intended_deliverables) {
            text("deliverables", s)?;
        }
        for a in &self.attachments {
            text("attachment name", &a.name)?;
            if a.sha256.as_deref().is_some_and(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))) {
                return Err("mission: attachment sha256 must be 64 lower-case hex digits".into());
            }
        }
        Ok(())
    }
}

/// `Text`: non-empty (after trimming) and at most `MAX_TEXT` characters.
fn text(name: &str, s: &str) -> Result<(), String> {
    if s.is_empty() || s.chars().count() > MAX_TEXT {
        return Err(format!("mission: {name} must be 1 to {MAX_TEXT} characters"));
    }
    Ok(())
}

fn trim_strings(v: &mut Value) {
    match v {
        Value::String(s) => {
            let t = s.trim();
            if t.len() != s.len() {
                *s = t.to_string();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(trim_strings),
        Value::Object(o) => o.values_mut().for_each(trim_strings),
        _ => {}
    }
}

/// Python `str(UUID(value))`: braces, `urn:uuid:` and hyphens are accepted; lower-case hyphenated out.
fn canonical_uuid(s: &str) -> Option<String> {
    let s = s.trim().strip_prefix("urn:uuid:").unwrap_or(s.trim());
    let s = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(s);
    let hex: String = s.chars().filter(|&c| c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let h = hex.to_ascii_lowercase();
    Some(format!("{}-{}-{}-{}-{}", h.get(0..8)?, h.get(8..12)?, h.get(12..16)?, h.get(16..20)?, h.get(20..32)?))
}

/// Python `datetime.fromisoformat` for the forms a mission date takes.
fn is_iso_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        || DateTime::parse_from_rfc3339(s).is_ok()
        || DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%:z").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").is_ok()
        || NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").is_ok()
}

/// Python's `str()` of a scalar, used as the coded-domain key.
fn domain_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// ArcGIS UTC epoch milliseconds as Python's `datetime.isoformat()` with a `+00:00` offset.
fn epoch_ms_iso(ms: f64) -> Option<String> {
    let micros = (ms * 1000.0).round();
    if !micros.is_finite() || micros.abs() > 1e17 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // bounded and finite just above
    let t = DateTime::from_timestamp_micros(micros as i64)?;
    let fmt = if t.timestamp_subsec_micros() == 0 { "%Y-%m-%dT%H:%M:%S+00:00" } else { "%Y-%m-%dT%H:%M:%S%.6f+00:00" };
    Some(t.format(fmt).to_string())
}

/// Map allowlisted service attributes onto the canonical mission. `mapping` is canonical name →
/// source attribute; `coded_domains` is canonical name → (source code → value). Numeric dates are
/// ArcGIS UTC milliseconds, and deliverables may arrive as a JSON array in a string.
pub fn normalize_mission(
    attributes: &Map<String, Value>,
    mapping: &BTreeMap<String, String>,
    attachments: Vec<Attachment>,
    coded_domains: Option<&BTreeMap<String, Map<String, Value>>>,
    global_id_field: &str,
) -> Result<Mission, String> {
    let mut values = Map::new();
    values.insert("global_id".into(), attributes.get(global_id_field).cloned().unwrap_or(Value::Null));
    values.insert("attachments".into(), serde_json::to_value(attachments).map_err(|e| e.to_string())?);
    for (canonical, source) in mapping {
        let mut value = attributes.get(source).cloned().unwrap_or(Value::Null);
        if let Some(domain) = coded_domains.and_then(|d| d.get(canonical))
            && !value.is_null()
        {
            value = domain.get(&domain_key(&value)).cloned().ok_or_else(|| format!("{canonical}: unknown configured coded-domain value"))?;
        }
        if (canonical == "present_deliverables" || canonical == "intended_deliverables")
            && let Some(s) = value.as_str().filter(|s| !s.trim().is_empty())
        {
            value = serde_json::from_str(s).map_err(|_| format!("{canonical}: malformed JSON list"))?;
        }
        if canonical == "mission_date"
            && let Some(ms) = value.as_f64()
        {
            value = Value::String(epoch_ms_iso(ms).ok_or("mission_date: epoch milliseconds out of range")?);
        }
        values.insert(canonical.clone(), value);
    }
    Mission::from_value(&Value::Object(values))
}

/// Check configured sensitive field names: each must be a canonical mission field.
pub fn check_field_names(what: &str, names: &[String]) -> Result<(), String> {
    let known = Mission::field_names();
    match names.iter().find(|n| !known.contains(n)) {
        Some(n) => Err(format!("{what}: `{n}` is not a mission field")),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Accuracy {
    pub maximum_rmse_horizontal_feet: Option<f64>,
    pub maximum_rmse_x_feet: Option<f64>,
    pub maximum_rmse_y_feet: Option<f64>,
    pub maximum_rmse_z_feet: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Tolerances {
    pub maximum_altitude_difference_feet: Option<f64>,
    pub maximum_speed_difference_mph: Option<f64>,
    pub maximum_overlap_difference_percent: Option<f64>,
    pub maximum_image_count_difference: Option<u64>,
    pub require_matching_flight_pattern: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Control {
    pub minimum_gcp_count: Option<u64>,
    pub minimum_checkpoint_count: Option<u64>,
    pub require_independent_checkpoints: bool,
    pub require_bare_ground_checkpoints: bool,
    pub permit_zero_checkpoints: bool,
    pub maximum_control_accuracy_feet: Option<f64>,
    pub maximum_checkpoint_accuracy_feet: Option<f64>,
}

/// Report metrics a ruleset may require.
pub const PROCESSING_METRICS: [&str; 7] =
    ["rmse_x_feet", "rmse_y_feet", "rmse_z_feet", "calibrated_images", "total_images", "processing_software", "processing_version"];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Processing {
    pub required_metrics: Vec<String>,
    pub minimum_calibrated_percent: Option<f64>,
}

/// A project's accuracy and completeness criteria. A criterion left unset never passes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    pub version: String,
    pub required_properties: Vec<String>,
    #[serde(default)]
    pub accuracy: Accuracy,
    #[serde(default)]
    pub tolerances: Tolerances,
    #[serde(default)]
    pub required_deliverables: Vec<String>,
    #[serde(default)]
    pub required_attachment_patterns: Vec<String>,
    #[serde(default)]
    pub coordinates: BTreeMap<String, String>,
    #[serde(default)]
    pub control: Control,
    #[serde(default)]
    pub allowed_correction_statuses: Vec<String>,
    #[serde(default)]
    pub require_correction_source: bool,
    #[serde(default)]
    pub processing: Processing,
}

const COORDINATE_REQUIREMENTS: [&str; 7] =
    ["horizontal_crs", "horizontal_datum", "horizontal_units", "vertical_crs", "vertical_datum", "vertical_units", "geoid_model"];

/// Compile a user-configured attachment pattern for a whole-name match (Python `re.fullmatch`).
pub fn attachment_pattern(pattern: &str) -> Result<regex::Regex, String> {
    if pattern.len() > MAX_PATTERN {
        return Err(format!("attachment pattern is longer than {MAX_PATTERN} bytes"));
    }
    regex::RegexBuilder::new(&format!("^(?:{pattern})$"))
        .size_limit(1 << 20)
        .build()
        .map_err(|_| "attachment pattern is not a valid regular expression".to_string())
}

impl Ruleset {
    /// Strict read of a ruleset from JSON text.
    pub fn from_json(text: &str) -> Result<Ruleset, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("ruleset: {e}"))?;
        Ruleset::from_value(&v)
    }

    /// Strict read of a ruleset from a JSON value.
    pub fn from_value(value: &Value) -> Result<Ruleset, String> {
        let r: Ruleset = serde_json::from_value(value.clone()).map_err(|e| format!("ruleset: {e}"))?;
        r.check()?;
        Ok(r)
    }

    fn check(&self) -> Result<(), String> {
        let bad = |m: &str| Err(format!("ruleset: {m}"));
        let version = self.version.trim();
        if version.is_empty() || version.chars().count() > MAX_TEXT {
            return bad("version must be 1 to 2000 characters");
        }
        check_field_names("ruleset: required_properties", &self.required_properties)?;
        if let Some(k) = self.coordinates.keys().find(|k| !COORDINATE_REQUIREMENTS.contains(&k.as_str())) {
            return bad(&format!("unknown coordinate requirement `{k}`"));
        }
        if let Some(m) = self.processing.required_metrics.iter().find(|m| !PROCESSING_METRICS.contains(&m.as_str())) {
            return bad(&format!("unknown processing metric `{m}`"));
        }
        let lists = [&self.required_properties, &self.required_deliverables, &self.required_attachment_patterns, &self.allowed_correction_statuses];
        if lists.iter().any(|l| l.len() > MAX_LIST) || self.coordinates.len() > MAX_LIST {
            return bad("a list has too many entries");
        }
        let texts = self.required_deliverables.iter().chain(&self.allowed_correction_statuses).chain(self.coordinates.values());
        if texts.into_iter().any(|t| t.trim().is_empty() || t.chars().count() > MAX_TEXT) {
            return bad("text values must be 1 to 2000 characters");
        }
        for p in &self.required_attachment_patterns {
            attachment_pattern(p).map_err(|e| format!("ruleset: {e}"))?;
        }
        let a = &self.accuracy;
        let t = &self.tolerances;
        let c = &self.control;
        let non_negative = [
            a.maximum_rmse_horizontal_feet,
            a.maximum_rmse_x_feet,
            a.maximum_rmse_y_feet,
            a.maximum_rmse_z_feet,
            t.maximum_altitude_difference_feet,
            t.maximum_speed_difference_mph,
            c.maximum_control_accuracy_feet,
            c.maximum_checkpoint_accuracy_feet,
        ];
        if non_negative.into_iter().flatten().any(|v| !(v.is_finite() && v >= 0.0)) {
            return bad("thresholds must be finite and not negative");
        }
        let percent = [t.maximum_overlap_difference_percent, self.processing.minimum_calibrated_percent];
        if percent.into_iter().flatten().any(|v| !(0.0..=100.0).contains(&v)) {
            return bad("percentages must be between 0 and 100");
        }
        Ok(())
    }
}
