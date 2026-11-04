//! Point export: PNEZD CSV, per-layer grouping by feature code, LandXML CgPoints. Reference: AutoGIS
//! `core/envmon/civil3d_points.py`, `export_survey_cad.py`, `cad_layer_map.py`, `core/common/landxml.py`.
//! Owner: Codex (see `docs/agents/slice-1-survey.md`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

use crate::{Finding, SurveyPoint};

pub const MAX_ROWS: usize = 1_000_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 1024 * 1024;

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Bounded RFC-4180 records, including escaped quotes and embedded newlines. No paths in core.
pub(crate) fn read_csv(reader: impl Read, mut visit: impl FnMut(&[String], &[String]) -> io::Result<()>) -> io::Result<()> {
    let mut text = String::new();
    reader.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(invalid("CSV exceeds 64 MiB"));
    }
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let mut header = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut state = 0;
    let mut rows = 0;
    let mut record_bytes = 0;
    while let Some(c) = chars.next() {
        record_bytes += c.len_utf8();
        if record_bytes > MAX_RECORD_BYTES {
            return Err(invalid("CSV record exceeds 1 MiB"));
        }
        if state == 1 {
            if c == '"' {
                state = 2;
            } else {
                cell.push(c);
            }
            continue;
        }
        if c == '"' && state == 2 {
            cell.push('"');
            state = 1;
            continue;
        }
        if c == '"' && cell.is_empty() && state == 0 {
            state = 1;
            continue;
        }
        if c == ',' || c == '\r' || c == '\n' {
            row.push(std::mem::take(&mut cell));
            state = 0;
            if row.len() > 64 {
                return Err(invalid("CSV exceeds 64 columns"));
            }
            if c == ',' {
                continue;
            }
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            record_bytes = 0;
            accept_record(&mut header, &mut row, &mut rows, &mut visit)?;
        } else if state == 2 || c == '"' {
            return Err(invalid("malformed CSV quoting"));
        } else {
            cell.push(c);
        }
    }
    if state == 1 {
        return Err(invalid("unterminated CSV quoted field"));
    }
    if !cell.is_empty() || !row.is_empty() || state == 2 {
        row.push(cell);
        accept_record(&mut header, &mut row, &mut rows, &mut visit)?;
    }
    if header.is_empty() {
        return Err(invalid("CSV header is required"));
    }
    Ok(())
}

fn accept_record(
    header: &mut Vec<String>,
    row: &mut Vec<String>,
    rows: &mut usize,
    visit: &mut impl FnMut(&[String], &[String]) -> io::Result<()>,
) -> io::Result<()> {
    if row.len() == 1 && row.first().is_some_and(|s| s.is_empty()) {
        row.clear();
        return Ok(());
    }
    if header.is_empty() {
        *header = row.drain(..).map(|s| s.trim().to_ascii_lowercase()).collect();
        if header.len() > 64 || header.iter().any(String::is_empty) || header.iter().collect::<BTreeSet<_>>().len() != header.len() {
            return Err(invalid("CSV headers must be nonblank and unique (maximum 64)"));
        }
        // Validate even a header-only file.
        visit(header, &[])?;
        return Ok(());
    }
    *rows += 1;
    if *rows > MAX_ROWS {
        return Err(invalid("CSV exceeds 1,000,000 rows"));
    }
    if row.len() != header.len() {
        return Err(invalid(format!("CSV row {rows} has a different column count")));
    }
    visit(header, row)?;
    row.clear();
    Ok(())
}

pub(crate) fn column(header: &[String], names: &[&str]) -> io::Result<usize> {
    names.iter().find_map(|name| header.iter().position(|h| h == name)).ok_or_else(|| invalid(format!("missing CSV column {}", names.join(" / "))))
}

pub(crate) fn value(row: &[String], index: usize) -> &str {
    row.get(index).map(String::as_str).unwrap_or_default()
}

pub(crate) fn finite(value: f64) -> io::Result<f64> {
    if value.is_finite() { Ok(value) } else { Err(invalid("numbers and computed results must be finite")) }
}

pub(crate) fn number(text: &str) -> io::Result<f64> {
    finite(text.trim().parse().map_err(|_| invalid(format!("invalid number {text:?}")))?)
}

/// Elevation-record CSV (location_id,x,y,z) or layer CSV (point_id,x,y,z,...).
/// Missing/unusable elevations are skipped, with one finding per record as in AutoGIS.
pub fn read_points_csv(reader: impl Read) -> io::Result<(Vec<SurveyPoint>, Vec<Finding>)> {
    let mut points = Vec::new();
    let mut findings = Vec::new();
    read_csv(reader, |header, row| {
        let id = column(header, &["point_id", "location_id", "id"])?;
        let x = column(header, &["x", "easting"])?;
        let y = column(header, &["y", "northing"])?;
        let z = column(header, &["z", "elevation"])?;
        if row.is_empty() {
            return Ok(());
        }
        let id = value(row, id).to_string();
        let Ok(elevation) = number(value(row, z)) else {
            findings.push(Finding::warning(format!("missing_elevation: point {id:?} has no usable elevation; skipped")));
            return Ok(());
        };
        let description = column(header, &["description"]).ok().map(|i| value(row, i)).unwrap_or_default();
        points.push(SurveyPoint {
            description: if description.is_empty() && !header.iter().any(|h| h == "point_id" || h == "id") { id.clone() } else { description.into() },
            id,
            easting: number(value(row, x))?,
            northing: number(value(row, y))?,
            elevation,
            feature_code: column(header, &["feature_code"]).ok().map(|i| value(row, i)).unwrap_or_default().into(),
        });
        Ok(())
    })?;
    Ok((points, findings))
}

fn check_points(points: &[SurveyPoint]) -> io::Result<()> {
    if points.len() > MAX_ROWS {
        return Err(invalid("point count exceeds 1,000,000"));
    }
    for p in points {
        finite(p.northing)?;
        finite(p.easting)?;
        finite(p.elevation)?;
    }
    Ok(())
}

fn safe_layer(name: &str) -> String {
    let safe: String = name.trim().chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let safe = safe.trim_end_matches(['.', ' ']);
    if safe.is_empty() {
        return "_invalid_layer".into();
    }
    let stem = safe.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
        "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        format!("_{safe}")
    } else {
        safe.into()
    }
}

/// Unknown codes go to `default`/Miscellaneous. Sanitization collisions merge, never overwrite.
pub fn group_points_by_layer(
    points: &[SurveyPoint],
    mapping: &BTreeMap<String, String>,
) -> io::Result<(BTreeMap<String, Vec<SurveyPoint>>, Vec<Finding>)> {
    check_points(points)?;
    let default = mapping.get("default").map(String::as_str).unwrap_or("Miscellaneous");
    let mut raw = BTreeMap::<&str, Vec<SurveyPoint>>::new();
    let mut raw_order = Vec::new();
    let mut unknown = BTreeSet::new();
    let mut findings = Vec::new();
    for p in points {
        let code = p.feature_code.trim();
        let layer = if code.is_empty() { default } else { mapping.get(code).map(String::as_str).unwrap_or(default) };
        if !code.is_empty() && !mapping.contains_key(code) {
            unknown.insert(code);
        }
        if !raw.contains_key(layer) {
            raw_order.push(layer);
        }
        raw.entry(layer).or_default().push(p.clone());
    }
    if !unknown.is_empty() {
        findings
            .push(Finding::warning(format!("unmapped_feature_codes: routed to {default:?}: {}", unknown.into_iter().collect::<Vec<_>>().join(", "))));
    }
    let mut layers = BTreeMap::<String, Vec<SurveyPoint>>::new();
    let mut case_names = BTreeMap::new();
    for name in raw_order {
        let points = raw.remove(name).unwrap_or_default();
        let safe = safe_layer(name);
        if safe != name {
            findings.push(Finding::warning(format!("unsafe_layer_name: {name:?} replaced with {safe:?}")));
        }
        // Windows file names are case-insensitive; keep one spelling when layers collide.
        let canonical = case_names.entry(safe.to_lowercase()).or_insert(safe);
        layers.entry(canonical.clone()).or_default().extend(points);
    }
    Ok((layers, findings))
}

pub(crate) fn csv_record(out: &mut String, fields: &[&str]) {
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        if field.contains([',', '"', '\r', '\n']) {
            out.push('"');
            out.push_str(&field.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(field);
        }
    }
    out.push('\n');
}

pub fn layer_csv(points: &[SurveyPoint]) -> io::Result<String> {
    check_points(points)?;
    let mut out = "point_id,x,y,z,feature_code,description\n".to_string();
    for p in points {
        csv_record(&mut out, &[&p.id, &p.easting.to_string(), &p.northing.to_string(), &p.elevation.to_string(), &p.feature_code, &p.description]);
    }
    Ok(out)
}

pub fn pnezd_csv(points: &[SurveyPoint], start_number: u64) -> io::Result<String> {
    check_points(points)?;
    let mut out = "point_number,northing,easting,elevation,description\n".to_string();
    for (i, p) in points.iter().enumerate() {
        let number = start_number.checked_add(i as u64).ok_or_else(|| invalid("point number overflow"))?;
        let description = if p.description.is_empty() { &p.id } else { &p.description };
        csv_record(&mut out, &[&number.to_string(), &p.northing.to_string(), &p.easting.to_string(), &p.elevation.to_string(), description]);
    }
    Ok(out)
}

pub struct XmlOptions<'a> {
    pub crs: &'a str,
    pub linear_unit: &'a str,
    /// Caller-supplied export date/time, validated through chrono without a platform clock.
    pub date: &'a str,
    pub time: &'a str,
}

fn xml(text: &str) -> io::Result<String> {
    if text.chars().any(|c| !(matches!(c, '\t' | '\r' | '\n') || c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')) {
        return Err(invalid("invalid XML character"));
    }
    Ok(text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;"))
}

/// LandXML 1.2 CgPoints only. Declares CRS/units; coordinates pass through unchanged.
pub fn landxml(points: &[SurveyPoint], options: &XmlOptions<'_>) -> io::Result<String> {
    check_points(points)?;
    let date = chrono::NaiveDate::parse_from_str(options.date, "%Y-%m-%d").map_err(|e| invalid(format!("invalid LandXML date: {e}")))?;
    let time = chrono::NaiveTime::parse_from_str(options.time, "%H:%M:%S").map_err(|e| invalid(format!("invalid LandXML time: {e}")))?;
    if date.format("%Y-%m-%d").to_string() != options.date || time.format("%H:%M:%S").to_string() != options.time {
        return Err(invalid("LandXML date/time must be YYYY-MM-DD and HH:MM:SS"));
    }
    let crs = options.crs.trim();
    let epsg_text = crs.split_once(':').map_or(crs, |(authority, code)| if authority.trim().eq_ignore_ascii_case("EPSG") { code.trim() } else { "" });
    let epsg = epsg_text
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0 && (4..=6).contains(&epsg_text.len()) && epsg_text.bytes().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| invalid("LandXML requires an EPSG CRS (e.g. EPSG:2256)"))?;
    let units = match options.linear_unit {
        "meter" => {
            "<Metric linearUnit=\"meter\" areaUnit=\"squareMeter\" volumeUnit=\"cubicMeter\" temperatureUnit=\"celsius\" pressureUnit=\"milliBars\" elevationUnit=\"meter\"/>"
        }
        "foot" | "USSurveyFoot" => &format!(
            "<Imperial linearUnit=\"{}\" areaUnit=\"squareFoot\" volumeUnit=\"cubicFeet\" temperatureUnit=\"fahrenheit\" pressureUnit=\"inHG\" elevationUnit=\"feet\"/>",
            options.linear_unit
        ),
        _ => return Err(invalid("LandXML linear_unit must be meter, foot or USSurveyFoot")),
    };
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" version=\"1.2\" date=\"{}\" time=\"{}\"><Units>{units}</Units><CoordinateSystem name=\"{}\" epsgCode=\"{epsg}\"/><CgPoints>\n",
        xml(options.date)?,
        xml(options.time)?,
        xml(crs)?
    );
    for p in points {
        out.push_str(&format!(
            "<CgPoint name=\"{}\" code=\"{}\" desc=\"{}\">{} {} {}</CgPoint>\n",
            xml(&p.id)?,
            xml(&p.feature_code)?,
            xml(&p.description)?,
            p.northing,
            p.easting,
            p.elevation
        ));
    }
    out.push_str("</CgPoints></LandXML>\n");
    Ok(out)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerStyle {
    pub cad_layer: String,
    pub color: Option<u16>,
    pub linetype: Option<String>,
}

pub struct CadPlan {
    pub mappings: Vec<(String, LayerStyle)>,
    pub crs: String,
    pub unmapped: Vec<String>,
    pub findings: Vec<Finding>,
}

pub fn validate_crs(crs: &str, findings: &mut Vec<Finding>) -> bool {
    if crs.trim().is_empty() {
        findings.push(Finding::error("missing_crs: no coordinate system provided"));
        false
    } else {
        true
    }
}

pub fn resolve_cad_plan(selected: &[String], config: &BTreeMap<String, LayerStyle>, crs: &str) -> CadPlan {
    let mut plan = CadPlan { mappings: Vec::new(), crs: crs.into(), unmapped: Vec::new(), findings: Vec::new() };
    validate_crs(crs, &mut plan.findings);
    for name in selected {
        match config.get(name).filter(|style| !style.cad_layer.trim().is_empty()) {
            Some(style) => plan.mappings.push((name.clone(), style.clone())),
            None => plan.unmapped.push(name.clone()),
        }
    }
    if !plan.unmapped.is_empty() {
        plan.findings.push(Finding::error(format!("unmapped_layers: {}", plan.unmapped.join(", "))));
    }
    plan
}

pub fn mapping_csv(plan: &CadPlan) -> String {
    let mut out = "gis_layer,cad_layer,color,linetype\n".to_string();
    for (name, style) in &plan.mappings {
        csv_record(
            &mut out,
            &[name, &style.cad_layer, &style.color.map(|c| c.to_string()).unwrap_or_default(), style.linetype.as_deref().unwrap_or_default()],
        );
    }
    out
}
