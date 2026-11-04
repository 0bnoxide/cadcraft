//! `handoff.json`: structural rules of the v1 JSON Schema (`contract/v1/handoff-manifest.schema.json`
//! in AutoGIS-Civil3D), then the semantic rules the schema cannot express.

use serde_json::{Map, Value};

use crate::{Issue, LinearUnit, Severity, codes};

/// The manifest facts later stages use.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub package_id: String,
    pub surface_sha256: String,
    pub surface_name: String,
    pub point_count: u64,
    pub face_count: u64,
    pub epsg_code: i32,
    pub horizontal_unit: LinearUnit,
    pub vertical_unit: LinearUnit,
}

/// Parses and checks the manifest. `Ok` carries any warnings; `Err` carries the errors.
pub(crate) fn parse(bytes: &[u8]) -> Result<(Manifest, Vec<Issue>), Vec<Issue>> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let Ok(root) = serde_json::from_slice::<Value>(bytes) else {
        return Err(vec![Issue::error(codes::MANIFEST_INVALID_JSON, "handoff.json is not valid UTF-8 JSON.")]);
    };
    if !schema::root(&root) {
        return Err(vec![Issue::error(codes::MANIFEST_SCHEMA_VIOLATION, "handoff.json does not satisfy contract version 1.0.")]);
    }
    semantic(&root)
}

fn get<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, key| v.get(key))
}

fn text<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    get(value, path).and_then(Value::as_str).unwrap_or_default()
}

fn semantic(root: &Value) -> Result<(Manifest, Vec<Issue>), Vec<Issue>> {
    let mut errors = Vec::new();
    let semantic_issue = |message: &str, location: &str| Issue::error_at(codes::MANIFEST_SEMANTIC_VIOLATION, message, location);

    let package_id = text(root, &["package_id"]).to_ascii_lowercase();
    if !utc_timestamp(text(root, &["created_utc"])) {
        errors.push(semantic_issue("created_utc must be normalized to UTC with a trailing Z.", "created_utc"));
    }

    let name = text(root, &["producer", "name"]);
    let version = text(root, &["producer", "version"]);
    let commit = get(root, &["producer", "source_commit"]).and_then(Value::as_str);
    for (value, location) in [(name, "producer.name"), (version, "producer.version")] {
        if value.trim().is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            errors.push(semantic_issue("Producer fields must not be blank, padded with whitespace, or contain control characters.", location));
        }
    }
    if path_shaped(name) || path_shaped(version) || commit.is_some_and(path_shaped) {
        errors.push(semantic_issue("producer fields must not contain filesystem paths.", "producer"));
    }

    let surface_name = text(root, &["surface", "name"]).trim().to_owned();
    if surface_name.is_empty() {
        errors.push(semantic_issue("Surface name must not be blank after trimming.", "surface.name"));
    }
    let mut count = |key: &str| {
        let value = get(root, &["surface", key]).and_then(Value::as_i64).and_then(|n| u64::try_from(n).ok());
        if value.is_none() {
            errors.push(semantic_issue(&format!("{key} must fit within a 64-bit signed integer."), &format!("surface.{key}")));
        }
        value.unwrap_or_default()
    };
    let point_count = count("point_count");
    let face_count = count("face_count");

    let vertical = ["coordinate_reference", "vertical"];
    let datum_known = text(root, &["coordinate_reference", "vertical", "datum", "status"]) == "known";
    if datum_known && int32(get(root, &["coordinate_reference", "vertical", "datum", "code"])).is_none() {
        errors.push(semantic_issue("code must fit within a 32-bit signed integer.", "coordinate_reference.vertical.datum.code"));
    }
    let epsg_code = int32(get(root, &["coordinate_reference", "horizontal", "code"]));
    if epsg_code.is_none() {
        errors.push(semantic_issue("code must fit within a 32-bit signed integer.", "coordinate_reference.horizontal.code"));
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let manifest = Manifest {
        package_id,
        surface_sha256: text(root, &["surface", "sha256"]).to_owned(),
        surface_name,
        point_count,
        face_count,
        epsg_code: epsg_code.unwrap_or_default(),
        horizontal_unit: unit(text(root, &["coordinate_reference", "horizontal", "unit"])),
        vertical_unit: unit(text(root, &[vertical[0], vertical[1], "unit"])),
    };
    let mut warnings = Vec::new();
    if !datum_known {
        warnings.push(Issue {
            code: codes::UNKNOWN_VERTICAL_DATUM,
            severity: Severity::Warning,
            message: "Vertical datum is unknown and requires review; confirm elevation alignment before use.".into(),
            location: Some("coordinate_reference.vertical.datum".into()),
        });
    }
    Ok((manifest, warnings))
}

fn int32(value: Option<&Value>) -> Option<i32> {
    value.and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok())
}

fn unit(text: &str) -> LinearUnit {
    match text {
        "international_foot" => LinearUnit::InternationalFoot,
        "us_survey_foot" => LinearUnit::UsSurveyFoot,
        _ => LinearUnit::Metre,
    }
}

fn path_shaped(value: &str) -> bool {
    let b = value.as_bytes();
    value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || (b.len() >= 2 && b.first().is_some_and(u8::is_ascii_alphabetic) && b.get(1) == Some(&b':'))
}

/// `YYYY-MM-DDTHH:MM:SS[.fff]Z` naming a real instant, upper-case `Z`.
fn utc_timestamp(text: &str) -> bool {
    text.ends_with('Z')
        && schema::date_time(text)
        && text.get(..19).is_some_and(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok())
}

/// The v1 JSON Schema, written out by hand (draft 2020-12 semantics for the keywords it uses).
mod schema {
    use super::{Map, Value};

    fn object<'a>(value: &'a Value, allowed: &[&str], required: &[&str]) -> Option<&'a Map<String, Value>> {
        let map = value.as_object()?;
        let ok = map.keys().all(|k| allowed.contains(&k.as_str())) && required.iter().all(|k| map.contains_key(*k));
        ok.then_some(map)
    }

    fn is_const(value: Option<&Value>, expected: &str) -> bool {
        value.and_then(Value::as_str) == Some(expected)
    }

    /// JSON Schema "integer" (any number with no fractional part) with `minimum: 1`.
    fn positive_integer(value: Option<&Value>) -> bool {
        let Some(n) = value.and_then(Value::as_number) else { return false };
        if let Some(i) = n.as_i64() {
            return i >= 1;
        }
        if n.as_u64().is_some() {
            return true;
        }
        n.as_f64().is_some_and(|f| f.is_finite() && f.fract() == 0.0 && f >= 1.0)
    }

    fn name(value: Option<&Value>, max: usize) -> bool {
        let Some(s) = value.and_then(Value::as_str) else { return false };
        let len = s.chars().count();
        (1..=max).contains(&len) && !s.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
    }

    fn unit(value: Option<&Value>) -> bool {
        matches!(value.and_then(Value::as_str), Some("metre" | "international_foot" | "us_survey_foot"))
    }

    fn hex(s: &str, lower_only: bool) -> bool {
        s.chars().all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f') || (!lower_only && matches!(c, 'A'..='F')))
    }

    fn uuid(value: Option<&Value>) -> bool {
        let Some(s) = value.and_then(Value::as_str) else { return false };
        let parts: Vec<&str> = s.split('-').collect();
        let [a, b, c, d, e] = parts.as_slice() else { return false };
        let lens = [a.len(), b.len(), c.len(), d.len(), e.len()] == [8, 4, 4, 4, 12];
        lens && parts.iter().all(|p| hex(p, false))
            && c.starts_with(|ch: char| ('1'..='5').contains(&ch))
            && d.starts_with(['8', '9', 'a', 'b', 'A', 'B'])
    }

    /// RFC 3339 `date-time`.
    pub(super) fn date_time(s: &str) -> bool {
        let b = s.as_bytes();
        let digits = |range: std::ops::Range<usize>| b.get(range).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
        let at = |i: usize| b.get(i).copied();
        if !(digits(0..4) && at(4) == Some(b'-') && digits(5..7) && at(7) == Some(b'-') && digits(8..10)) {
            return false;
        }
        if !matches!(at(10), Some(b'T' | b't'))
            || !(digits(11..13) && at(13) == Some(b':') && digits(14..16) && at(16) == Some(b':') && digits(17..19))
        {
            return false;
        }
        let mut i = 19;
        if at(i) == Some(b'.') {
            i += 1;
            let start = i;
            while at(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            if i == start {
                return false;
            }
        }
        let zone_ok = match at(i) {
            Some(b'Z' | b'z') => i + 1 == b.len(),
            Some(b'+' | b'-') => digits(i + 1..i + 3) && at(i + 3) == Some(b':') && digits(i + 4..i + 6) && i + 6 == b.len(),
            _ => false,
        };
        let num = |r: std::ops::Range<usize>| s.get(r).and_then(|t| t.parse::<u32>().ok()).unwrap_or(99);
        let date_ok = chrono::NaiveDate::from_ymd_opt(i32::try_from(num(0..4)).unwrap_or(0), num(5..7), num(8..10)).is_some();
        zone_ok && date_ok && num(11..13) <= 23 && num(14..16) <= 59 && num(17..19) <= 60
    }

    pub(super) fn root(v: &Value) -> bool {
        let Some(m) = object(
            v,
            &["contract_version", "package_id", "created_utc", "producer", "surface", "coordinate_reference"],
            &["contract_version", "package_id", "created_utc", "producer", "surface", "coordinate_reference"],
        ) else {
            return false;
        };
        is_const(m.get("contract_version"), "1.0")
            && uuid(m.get("package_id"))
            && m.get("created_utc").and_then(Value::as_str).is_some_and(date_time)
            && m.get("producer").is_some_and(producer)
            && m.get("surface").is_some_and(surface)
            && m.get("coordinate_reference").is_some_and(coordinate_reference)
    }

    fn producer(v: &Value) -> bool {
        let Some(m) = object(v, &["name", "version", "source_commit"], &["name", "version"]) else { return false };
        let commit_ok = m.get("source_commit").is_none_or(|c| c.as_str().is_some_and(|s| (7..=64).contains(&s.len()) && hex(s, true)));
        name(m.get("name"), 100) && name(m.get("version"), 64) && commit_ok
    }

    fn surface(v: &Value) -> bool {
        let keys = ["filename", "sha256", "landxml_version", "name", "point_count", "face_count"];
        let Some(m) = object(v, &keys, &keys) else { return false };
        is_const(m.get("filename"), "surface.landxml")
            && m.get("sha256").and_then(Value::as_str).is_some_and(|s| s.len() == 64 && hex(s, true))
            && is_const(m.get("landxml_version"), "1.2")
            && name(m.get("name"), 100)
            && positive_integer(m.get("point_count"))
            && positive_integer(m.get("face_count"))
    }

    fn coordinate_reference(v: &Value) -> bool {
        let Some(m) = object(v, &["horizontal", "vertical"], &["horizontal", "vertical"]) else { return false };
        m.get("horizontal").is_some_and(horizontal) && m.get("vertical").is_some_and(vertical)
    }

    fn horizontal(v: &Value) -> bool {
        let keys = ["kind", "authority", "code", "unit"];
        let Some(m) = object(v, &keys, &keys) else { return false };
        is_const(m.get("kind"), "projected") && is_const(m.get("authority"), "EPSG") && positive_integer(m.get("code")) && unit(m.get("unit"))
    }

    fn vertical(v: &Value) -> bool {
        let keys = ["unit", "direction", "datum"];
        let Some(m) = object(v, &keys, &keys) else { return false };
        unit(m.get("unit")) && is_const(m.get("direction"), "positive_up") && m.get("datum").is_some_and(|d| known_datum(d) != unknown_datum(d))
    }

    fn known_datum(v: &Value) -> bool {
        let keys = ["status", "authority", "code", "name"];
        let Some(m) = object(v, &keys, &keys) else { return false };
        is_const(m.get("status"), "known") && name(m.get("authority"), 64) && positive_integer(m.get("code")) && name(m.get("name"), 100)
    }

    fn unknown_datum(v: &Value) -> bool {
        let Some(m) = object(v, &["status", "note"], &["status"]) else { return false };
        is_const(m.get("status"), "unknown") && m.get("note").is_none_or(|n| name(Some(n), 100))
    }
}

#[cfg(test)]
mod tests {
    use super::schema::date_time;

    #[test]
    fn rfc3339() {
        for ok in ["2026-08-02T00:00:00Z", "2026-08-02t00:00:00.5z", "2026-08-02T00:00:00+02:00"] {
            assert!(date_time(ok), "{ok}");
        }
        for bad in ["2026-08-02", "2026-02-30T00:00:00Z", "2026-08-02T24:00:00Z", "2026-08-02T00:00:00", "2026-08-02T00:00:00.Z"] {
            assert!(!date_time(bad), "{bad}");
        }
    }
}
