//! RTK point import and QA flags. Reference: AutoGIS `core/envmon/import_rtk_survey.py`,
//! `validate_rtk_survey.py`. Owner: Claude (see `docs/agents/slice-1-survey.md`).
//!
//! Two CSV layouts are read. A headered file names its columns (see [`ColumnMap`]). A headerless
//! file is PNEZD or PENZD, optionally with extra columns; its northing/easting order is given or
//! guessed from the magnitude of the two coordinate columns.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{BufRead, BufReader, Read};

use crate::{Finding, Severity, SurveyPoint};

/// Most data rows one file may hold.
pub const MAX_ROWS: usize = 1_000_000;
/// Default horizontal and vertical precision thresholds, in the input's units.
pub const DEFAULT_HRMS: f64 = 0.03;
pub const DEFAULT_VRMS: f64 = 0.05;
/// Fix types that count as RTK; anything else set is flagged `fix_type_not_rtk`.
pub const RTK_FIX_TYPES: [&str; 3] = ["RTK_FIXED", "RTK_FLOAT", "NETWORK_RTK"];
/// Names `extra_columns` may use for columns 6 onwards of a headerless file.
pub const EXTRA_COLUMNS: [&str; 8] = ["hrms", "vrms", "pdop", "satellites", "fix_type", "collected_at", "operator", "feature_code"];
/// Columns 6-11 of the built-in 11-column headerless layout.
const DEFAULT_11COL_EXTRA: [&str; 6] = ["hrms", "vrms", "pdop", "satellites", "fix_type", "collected_at"];

/// One RTK shot: the point plus the receiver's quality data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RtkPoint {
    pub point: SurveyPoint,
    pub hrms: Option<f64>,
    pub vrms: Option<f64>,
    pub pdop: Option<f64>,
    pub satellites: Option<u32>,
    pub fix_type: String,
    pub collected_at: String,
    pub operator: String,
}

/// Column order of a headerless file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoordOrder {
    /// Guess from the coordinate magnitudes; refuse when both columns have as many digits.
    #[default]
    Auto,
    /// Point, northing, easting, elevation, description.
    Pnezd,
    /// Point, easting, northing, elevation, description.
    Penzd,
}

/// Header names of a headered file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMap {
    pub point_id: String,
    pub northing: String,
    pub easting: String,
    pub elevation: String,
    pub feature_code: String,
    pub description: String,
    pub hrms: String,
    pub vrms: String,
    pub fix_type: String,
    pub collected_at: String,
    pub operator: String,
}

impl Default for ColumnMap {
    fn default() -> Self {
        Self {
            point_id: "PointID".into(),
            northing: "Northing".into(),
            easting: "Easting".into(),
            elevation: "Elevation_ft".into(),
            feature_code: "FeatureCode".into(),
            description: "Description".into(),
            hrms: "HRMS_ft".into(),
            vrms: "VRMS_ft".into(),
            fix_type: "FixType".into(),
            collected_at: "CollectedAt".into(),
            operator: "Operator".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub columns: ColumnMap,
    pub order: CoordOrder,
    /// Names (from [`EXTRA_COLUMNS`]) of a headerless file's columns 6 onwards.
    pub extra_columns: Option<Vec<String>>,
}

/// Why a file could not be read. Nothing is imported when this is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(msg.into()))
}

/// The points read, and warnings about how they were read.
#[derive(Debug, Clone, Default)]
pub struct Import {
    pub points: Vec<RtkPoint>,
    pub findings: Vec<Finding>,
}

/// One CSV row and its 1-based line number.
pub(crate) type Row = (usize, Vec<String>);

/// Longest line, most fields on one line, and most fields kept from one file.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_FIELDS: usize = 256;
pub const MAX_CELLS: usize = 16_000_000;

/// Reads non-blank CSV rows, refusing more than [`MAX_ROWS`] rows, a line over
/// [`MAX_LINE_BYTES`] or [`MAX_FIELDS`] fields, and more than [`MAX_CELLS`] fields in all, each
/// checked before the memory is taken.
// shortcut: a quoted field cannot span lines; switch to a full CSV reader if field software
// starts writing multi-line descriptions.
pub(crate) fn read_rows(r: impl Read) -> Result<Vec<Row>, ParseError> {
    let mut reader = BufReader::new(r);
    let mut rows = Vec::new();
    let mut cells_kept = 0usize;
    let mut buf = Vec::new();
    for line_no in 1usize.. {
        buf.clear();
        let limit = u64::try_from(MAX_LINE_BYTES).unwrap_or(u64::MAX).saturating_add(1);
        let n = reader.by_ref().take(limit).read_until(b'\n', &mut buf).map_err(|e| ParseError(format!("line {line_no}: {e}")))?;
        if n == 0 {
            break;
        }
        if buf.len() > MAX_LINE_BYTES {
            return err(format!("line {line_no} is longer than {MAX_LINE_BYTES} bytes"));
        }
        let text = std::str::from_utf8(&buf).map_err(|_| ParseError(format!("line {line_no} is not UTF-8")))?;
        let text = text.trim_end_matches(['\n', '\r']);
        let text = if line_no == 1 { text.strip_prefix('\u{feff}').unwrap_or(text) } else { text };
        let fields = text.bytes().filter(|&b| b == b',').count().saturating_add(1);
        if fields > MAX_FIELDS {
            return err(format!("line {line_no} has more than {MAX_FIELDS} fields"));
        }
        if text.trim().trim_matches(|c: char| c == ',' || c.is_whitespace()).is_empty() {
            continue;
        }
        cells_kept = cells_kept.saturating_add(fields);
        if rows.len() >= MAX_ROWS || cells_kept > MAX_CELLS {
            return err(format!("the file has more than {MAX_ROWS} rows or {MAX_CELLS} fields"));
        }
        rows.push((line_no, split_csv_line(text)));
    }
    Ok(rows)
}

/// Splits one line on commas; `"` quotes a field and `""` inside quotes is a literal quote.
pub(crate) fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// A finite number, or `None` for a blank or unreadable cell (NaN and infinity are unreadable).
pub(crate) fn number(cell: &str) -> Option<f64> {
    cell.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub(crate) fn cell(row: &[String], i: usize) -> &str {
    row.get(i).map_or("", |s| s.trim())
}

/// `FIXED`, `FLOAT`, `NETWORK RTK` and `NRTK` (any case) to their RTK names; others unchanged.
pub fn normalize_fix_type(raw: &str) -> String {
    let raw = raw.trim();
    match raw.to_ascii_uppercase().as_str() {
        "FIXED" => "RTK_FIXED".into(),
        "FLOAT" => "RTK_FLOAT".into(),
        "NETWORK RTK" | "NRTK" => "NETWORK_RTK".into(),
        _ => raw.into(),
    }
}

/// Reads an RTK CSV, headered or headerless (row 1 is data when its columns 2-4 are numbers).
pub fn parse_rtk_csv(r: impl Read, opts: &ImportOptions) -> Result<Import, ParseError> {
    let rows = read_rows(r)?;
    let mut findings = Vec::new();
    let Some((_, first)) = rows.first() else {
        return Ok(Import::default());
    };
    let headerless = first.len() >= 4 && first.iter().skip(1).take(3).all(|c| number(c).is_some());
    let points = if headerless {
        parse_headerless(&rows, opts, &mut findings)?
    } else {
        if opts.order != CoordOrder::Auto || opts.extra_columns.as_ref().is_some_and(|c| !c.is_empty()) {
            findings.push(Finding::warning("format_options_ignored: `format` and `extra_columns` do nothing on a headered CSV"));
        }
        parse_headered(&rows, &opts.columns, &mut findings)
    };
    Ok(Import { points, findings })
}

/// Rows without a numeric northing, easting and elevation are skipped (with one warning).
/// Fix types are kept as written, as the reference does.
fn parse_headered(rows: &[Row], cm: &ColumnMap, findings: &mut Vec<Finding>) -> Vec<RtkPoint> {
    let Some(((_, header), data)) = rows.split_first() else {
        return Vec::new();
    };
    // The last column of a repeated name wins, as with a dictionary per row.
    let col = |name: &str| header.iter().rposition(|h| h == name);
    let get = |row: &[String], name: &str| col(name).map_or("", |i| cell(row, i)).to_string();
    let mut out = Vec::with_capacity(data.len());
    let mut skipped = 0usize;
    for (_, row) in data {
        let (Some(northing), Some(easting), Some(elevation)) =
            (number(&get(row, &cm.northing)), number(&get(row, &cm.easting)), number(&get(row, &cm.elevation)))
        else {
            skipped += 1;
            continue;
        };
        out.push(RtkPoint {
            point: SurveyPoint {
                id: get(row, &cm.point_id),
                northing,
                easting,
                elevation,
                feature_code: get(row, &cm.feature_code),
                description: get(row, &cm.description),
            },
            hrms: number(&get(row, &cm.hrms)),
            vrms: number(&get(row, &cm.vrms)),
            pdop: None,
            satellites: None,
            fix_type: get(row, &cm.fix_type),
            collected_at: get(row, &cm.collected_at),
            operator: get(row, &cm.operator),
        });
    }
    if skipped > 0 {
        findings.push(Finding::warning(format!(
            "skipped_rows: {skipped} row(s) without a numeric {}, {} and {} were skipped",
            cm.northing, cm.easting, cm.elevation
        )));
    }
    out
}

fn extra_fields(width: usize, extra: Option<&[String]>, findings: &mut Vec<Finding>) -> Result<Vec<String>, ParseError> {
    let Some(extra) = extra else {
        return match width {
            11 => Ok(DEFAULT_11COL_EXTRA.iter().map(|s| s.to_string()).collect()),
            5 => Ok(Vec::new()),
            _ => err(format!(
                "headerless parsing only supports 5-column PNEZD/PENZD or 11-column extended layouts; got {width} \
                 columns. Use `extra_columns` to declare a custom layout."
            )),
        };
    };
    let unknown: Vec<&str> = extra.iter().map(String::as_str).filter(|c| !EXTRA_COLUMNS.contains(c)).collect();
    if !unknown.is_empty() {
        return err(format!("unrecognized `extra_columns` field(s): {}. Valid names: {}.", unknown.join(", "), EXTRA_COLUMNS.join(", ")));
    }
    if width == 5 {
        findings.push(Finding::warning("extra_columns_noop: `extra_columns` given on a 5-column file; there were no extra columns to map"));
        return Ok(Vec::new());
    }
    if width.checked_sub(5) != Some(extra.len()) {
        return err(format!(
            "`extra_columns` names {} field(s) but the file has {width} column(s), 5 of them base columns; \
             the count must match exactly, or data would be silently dropped or misaligned.",
            extra.len()
        ));
    }
    Ok(extra.to_vec())
}

fn required(row: &Row, i: usize, label: &str) -> Result<f64, ParseError> {
    let raw = cell(&row.1, i);
    number(raw).map_or_else(|| err(format!("row {}: {label} value '{raw}' is not a valid number", row.0)), Ok)
}

/// Digits before the decimal point of `v`, as the order guess compares them.
fn digit_count(v: f64) -> usize {
    format!("{}", v.abs().trunc()).len()
}

fn mean(vals: &[f64]) -> Result<f64, ParseError> {
    let m = vals.iter().sum::<f64>() / vals.len() as f64;
    if m.is_finite() { Ok(m) } else { err("the coordinates are too large to average") }
}

/// Whether column 2 is northing (else column 3 is).
fn column2_is_northing(order: CoordOrder, c1: &[f64], c2: &[f64], findings: &mut Vec<Finding>) -> Result<bool, ParseError> {
    match order {
        CoordOrder::Pnezd => return Ok(true),
        CoordOrder::Penzd => return Ok(false),
        CoordOrder::Auto => {}
    }
    let (avg1, avg2) = (mean(c1)?, mean(c2)?);
    let (d1, d2) = (digit_count(avg1), digit_count(avg2));
    if d1 == d2 {
        return err(format!(
            "cannot auto-detect Northing/Easting column order: both columns have the same magnitude (~{d1} digits \
             average); pass `format` pnezd or penzd explicitly."
        ));
    }
    // The column with more digits is taken to be northing.
    let (n2, first, second) = if d2 > d1 { (false, "Easting", "Northing") } else { (true, "Northing", "Easting") };
    findings.push(Finding::warning(format!(
        "guessed_coord_order: column 2 = {first} (avg magnitude ~{avg1:.0}, {d1} digits), column 3 = {second} (avg \
         magnitude ~{avg2:.0}, {d2} digits); pass `format` pnezd or penzd to override if wrong"
    )));
    Ok(n2)
}

fn parse_headerless(rows: &[Row], opts: &ImportOptions, findings: &mut Vec<Finding>) -> Result<Vec<RtkPoint>, ParseError> {
    let width = rows.first().map_or(0, |r| r.1.len());
    let ragged: Vec<&Row> = rows.iter().filter(|r| r.1.len() != width).collect();
    if let Some((line, cells)) = ragged.first() {
        return err(format!(
            "ragged headerless CSV: row {line} has {} column(s), expected {width} (based on row 1); {} row(s) \
             disagree on column count. Headerless parsing needs the same column count on every row.",
            cells.len(),
            ragged.len()
        ));
    }
    let extra = extra_fields(width, opts.extra_columns.as_deref(), findings)?;
    let c1 = rows.iter().map(|r| required(r, 1, "Coord1")).collect::<Result<Vec<_>, _>>()?;
    let c2 = rows.iter().map(|r| required(r, 2, "Coord2")).collect::<Result<Vec<_>, _>>()?;
    let n2 = column2_is_northing(opts.order, &c1, &c2, findings)?;
    let mut out = Vec::with_capacity(rows.len());
    for (row, (&a, &b)) in rows.iter().zip(c1.iter().zip(&c2)) {
        let (northing, easting) = if n2 { (a, b) } else { (b, a) };
        let mut p = RtkPoint {
            point: SurveyPoint {
                id: cell(&row.1, 0).into(),
                northing,
                easting,
                elevation: required(row, 3, "Elevation")?,
                feature_code: String::new(),
                description: cell(&row.1, 4).into(),
            },
            ..RtkPoint::default()
        };
        for (i, name) in extra.iter().enumerate() {
            let raw = cell(&row.1, i.saturating_add(5));
            match name.as_str() {
                "hrms" => p.hrms = number(raw),
                "vrms" => p.vrms = number(raw),
                "pdop" => p.pdop = number(raw),
                // Truncated like the reference's int(float(v)); a count out of range is dropped.
                "satellites" => p.satellites = number(raw).map(f64::trunc).filter(|v| (0.0..=f64::from(u32::MAX)).contains(v)).map(|v| v as u32),
                "fix_type" => p.fix_type = normalize_fix_type(raw),
                "collected_at" => p.collected_at = raw.into(),
                "operator" => p.operator = raw.into(),
                "feature_code" => p.point.feature_code = raw.into(),
                _ => {}
            }
        }
        out.push(p);
    }
    Ok(out)
}

/// QA flags for one point: precision over a threshold, or a fix type that is set but not RTK.
pub fn qa_flags(p: &RtkPoint, hrms_max: f64, vrms_max: f64) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if p.hrms.is_some_and(|v| v > hrms_max) {
        flags.push("hrms_exceeds_threshold");
    }
    if p.vrms.is_some_and(|v| v > vrms_max) {
        flags.push("vrms_exceeds_threshold");
    }
    if !p.fix_type.is_empty() && !RTK_FIX_TYPES.contains(&p.fix_type.to_ascii_uppercase().as_str()) {
        flags.push("fix_type_not_rtk");
    }
    flags
}

fn show(v: Option<f64>) -> String {
    v.map_or_else(|| "none".into(), |v| v.to_string())
}

/// Checks a batch: repeated point ids, then each point's flags, then a summary line. An id
/// passes once when none of its shots is flagged.
pub fn validate(points: &[RtkPoint], hrms_max: f64, vrms_max: f64) -> Vec<Finding> {
    let info = |message: String| Finding { severity: Severity::Info, message };
    let mut out = Vec::new();
    if points.is_empty() {
        out.push(info("validation_complete: RTK validation: 0 points, nothing to check.".into()));
        return out;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut first_seen = Vec::new();
    for p in points {
        let n = counts.entry(p.point.id.as_str()).or_insert(0);
        if *n == 0 {
            first_seen.push(p.point.id.as_str());
        }
        *n += 1;
    }
    for id in first_seen {
        if let Some(&n) = counts.get(id).filter(|&&n| n > 1) {
            out.push(Finding::warning(format!("duplicate_point_id: PointID '{id}' appears {n} times")));
        }
    }
    let flags: Vec<Vec<&str>> = points.iter().map(|p| qa_flags(p, hrms_max, vrms_max)).collect();
    let flagged: HashSet<&str> = points.iter().zip(&flags).filter(|(_, f)| !f.is_empty()).map(|(p, _)| p.point.id.as_str()).collect();
    let mut passed: HashSet<&str> = HashSet::new();
    for (p, f) in points.iter().zip(&flags) {
        let id = p.point.id.as_str();
        if !flagged.contains(id) {
            passed.insert(id);
        }
        for flag in f {
            out.push(Finding::warning(format!("{flag}: {id} (HRMS={}, VRMS={}, FixType={})", show(p.hrms), show(p.vrms), p.fix_type)));
        }
    }
    out.push(info(format!("validation_complete: RTK validation: {}/{} points QA pass.", passed.len(), points.len())));
    out
}
