//! Boring-log package import and validation into a [`crate::BoringSet`]. Owner: Claude (see
//! `docs/agents/slice-6-subsurface.md`). Behaviour reference: AutoGIS `import_boring_logs.py`.
//!
//! A package is a folder of CSV files or one workbook with a sheet per table. Tables are found
//! by file stem or sheet name, columns by header alias; both match ignoring case, spaces,
//! punctuation and a trailing `ft` unit (`TopDepth_ft`, `Top Depth`, `topdepth` are one column).

use crate::boring::{Boring, BoringSet, Finding, Lithology, Sample, Severity, WaterLevel, WellComponent};
use crate::sheet::{self, SheetError};
use crate::{Cell, Table};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

/// Most CSV files read from one package folder.
pub const MAX_FILES: usize = 64;
/// Most directory entries looked at in one package folder.
pub const MAX_ENTRIES: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub enum ImportError {
    Io(String),
    Sheet(SheetError),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Io(m) => write!(f, "cannot read boring package: {m}"),
            ImportError::Sheet(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<SheetError> for ImportError {
    fn from(e: SheetError) -> Self {
        ImportError::Sheet(e)
    }
}

/// Load a package from a folder of CSV files or from one workbook or CSV file.
pub fn load(path: &Path) -> Result<(BoringSet, Vec<Finding>), ImportError> {
    let io = |e: std::io::Error| ImportError::Io(format!("{}: {e}", path.display()));
    let too_large = |m: String| ImportError::Sheet(SheetError::TooLarge(m));
    let mut tables = Vec::new();
    if path.is_dir() {
        let mut files = Vec::new();
        for (n, entry) in std::fs::read_dir(path).map_err(io)?.enumerate() {
            if n >= MAX_ENTRIES {
                return Err(too_large(format!("more than {MAX_ENTRIES} entries in the package folder")));
            }
            let p = entry.map_err(io)?.path();
            if p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("csv")) {
                if files.len() >= MAX_FILES {
                    return Err(too_large(format!("more than {MAX_FILES} CSV files")));
                }
                files.push(p);
            }
        }
        files.sort();
        // The whole package shares one byte and cell budget, as if it were one workbook.
        let mut budget = sheet::MAX_BYTES;
        let mut cells = 0usize;
        for f in files {
            let bytes = read_file(&f, budget)?;
            budget -= bytes.len();
            for t in sheet::read(&bytes, &f.to_string_lossy())? {
                cells = cells.saturating_add(t.rows.iter().map(Vec::len).sum());
                if cells > sheet::MAX_CELLS {
                    return Err(too_large("package has more than 2,000,000 cells".into()));
                }
                tables.push(t);
            }
        }
    } else {
        tables = sheet::read(&read_file(path, sheet::MAX_BYTES)?, &path.to_string_lossy())?;
    }
    Ok(from_tables(&tables))
}

/// The file's bytes, refused once more than `limit` arrive (reads at most `limit + 1`).
fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, ImportError> {
    use std::io::Read;
    let io = |e: std::io::Error| ImportError::Io(format!("{}: {e}", path.display()));
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(io)?.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() > limit {
        return Err(SheetError::TooLarge(format!("{} goes past the 64 MiB package limit", path.display())).into());
    }
    Ok(bytes)
}

/// Lower-case letters and digits only, without a trailing `ft` unit.
fn norm(s: &str) -> String {
    let n: String = s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect();
    match n.strip_suffix("ft") {
        Some(base) if base.len() > 2 => base.to_string(),
        _ => n,
    }
}

const LOCATIONS: &[&str] = &["boringlocations", "borings", "locations", "boringlocation"];
const LITHOLOGY: &[&str] = &["lithology", "lithologyintervals", "lith"];
const SAMPLES: &[&str] = &["samples", "boringsamples", "sample"];
const WELLS: &[&str] = &["wellconstruction", "wells", "wellcomponents"];
const WATER: &[&str] = &["waterlevels", "water", "groundwaterlevels"];

const BORING_ID: &[&str] = &["boringid", "boring", "locationid", "location", "wellid", "holeid"];
const TOP: &[&str] = &["topdepth", "top", "from", "depthfrom"];
const BOTTOM: &[&str] = &["bottomdepth", "bottom", "bot", "to", "depthto"];

/// One table found in the package, with its header resolved.
struct Sheet<'a> {
    table: &'a Table,
    cols: HashMap<String, usize>,
}

impl<'a> Sheet<'a> {
    fn new(table: &'a Table) -> Self {
        let mut cols = HashMap::new();
        for (i, h) in table.header().iter().enumerate() {
            cols.entry(norm(h)).or_insert(i);
        }
        Sheet { table, cols }
    }

    /// Data rows with their 1-based sheet row; rows with no content are skipped.
    fn rows(&self) -> impl Iterator<Item = (usize, &'a [Cell])> + 'a {
        let rows = self.table.rows.get(1..).unwrap_or_default();
        rows.iter().enumerate().filter(|(_, r)| r.iter().any(|c| *c != Cell::Empty)).map(|(i, r)| (i + 2, r.as_slice()))
    }

    fn cell<'r>(&self, row: &'r [Cell], aliases: &[&str]) -> Option<&'r Cell> {
        aliases.iter().find_map(|a| self.cols.get(*a)).and_then(|&i| row.get(i))
    }

    fn text(&self, row: &[Cell], aliases: &[&str]) -> String {
        self.cell(row, aliases).map(Cell::text).unwrap_or_default()
    }

    fn num(&self, row: &[Cell], aliases: &[&str]) -> Option<f64> {
        self.cell(row, aliases).and_then(Cell::number)
    }

    fn flag(&self, row: &[Cell], aliases: &[&str]) -> bool {
        match self.cell(row, aliases) {
            Some(Cell::Bool(b)) => *b,
            Some(Cell::Number(n)) => *n != 0.0,
            Some(c) => matches!(c.text().to_ascii_lowercase().as_str(), "true" | "yes" | "y" | "x" | "1"),
            None => false,
        }
    }
}

fn finding(severity: Severity, code: &str, message: String, boring_id: Option<&str>, source: Option<(&str, usize)>) -> Finding {
    Finding { severity, code: code.into(), message, boring_id: boring_id.map(str::to_string), source: source.map(|(s, r)| (s.to_string(), r)) }
}

/// Build a boring set from already-read tables, then [`validate`] it. Findings come in input
/// order: missing tables, row problems, then cross-checks.
pub fn from_tables(tables: &[Table]) -> (BoringSet, Vec<Finding>) {
    let mut found = Vec::new();
    let mut find = |kind: &str, names: &[&str]| -> Option<Sheet> {
        let t = tables.iter().find(|t| names.contains(&norm(&t.name).as_str()));
        if t.is_none() {
            found.push(finding(Severity::Info, "missing_table", format!("No {kind} table in the package"), None, None));
        }
        t.map(Sheet::new)
    };
    let locs = find("boring locations", LOCATIONS);
    let lith = find("lithology", LITHOLOGY);
    let samples = find("samples", SAMPLES);
    let wells = find("well construction", WELLS);
    let water = find("water levels", WATER);

    let mut set = BoringSet::default();
    // Every row needs a boring id; a row without one is reported and skipped.
    let id_of = |s: &Sheet, row: &[Cell], at: usize, found: &mut Vec<Finding>| -> Option<String> {
        let id = s.text(row, BORING_ID);
        if id.is_empty() {
            let msg = format!("{} row {at} has no boring id", s.table.name);
            found.push(finding(Severity::Error, "missing_boring_id", msg, None, Some((&s.table.name, at))));
            return None;
        }
        Some(id)
    };
    // Interval rows need both depths; drops are counted and reported once per table, with the
    // first dropped row as the source.
    let depths = |s: &Sheet, row: &[Cell], at: usize, dropped: &mut Vec<usize>| -> Option<(f64, f64)> {
        let d = s.num(row, TOP).zip(s.num(row, BOTTOM));
        if d.is_none() {
            dropped.push(at);
        }
        d
    };
    let report_dropped = |s: &Sheet, kind: &str, dropped: &[usize], found: &mut Vec<Finding>| {
        if let Some(&first) = dropped.first() {
            let msg = format!("{} {kind} row(s) dropped: missing or invalid top or bottom depth", dropped.len());
            found.push(finding(Severity::Warning, &format!("{kind}_rows_dropped_missing_depth"), msg, None, Some((&s.table.name, first))));
        }
    };

    if let Some(s) = &locs {
        for (at, row) in s.rows() {
            let Some(boring_id) = id_of(s, row, at, &mut found) else { continue };
            set.borings.push(Boring {
                boring_id,
                site_id: s.text(row, &["siteid", "site"]),
                location_type: s.text(row, &["locationtype", "type"]),
                northing: s.num(row, &["northing", "north", "y"]),
                easting: s.num(row, &["easting", "east", "x"]),
                ground_elevation: s.num(row, &["groundelevation", "groundelev", "elevation", "gse"]),
                toc_elevation: s.num(row, &["tocelevation", "toc", "topofcasing"]),
                total_depth: s.num(row, &["totaldepth", "td", "depth"]),
                status: s.text(row, &["status"]),
                completion_type: s.text(row, &["completiontype", "completion"]),
                coordinate_system: s.text(row, &["coordinatesystem", "crs"]),
                vertical_datum: s.text(row, &["verticaldatum", "datum"]),
            });
        }
    }
    if let Some(s) = &lith {
        let mut dropped = Vec::new();
        for (at, row) in s.rows() {
            let Some(boring_id) = id_of(s, row, at, &mut found) else { continue };
            let Some((top_depth, bottom_depth)) = depths(s, row, at, &mut dropped) else { continue };
            set.lithology.push(Lithology {
                boring_id,
                top_depth,
                bottom_depth,
                uscs: s.text(row, &["uscs", "uscscode", "uscssymbol"]),
                primary_material: s.text(row, &["primarymaterial", "material"]),
                color: s.text(row, &["color", "colour"]),
                moisture: s.text(row, &["moisture"]),
                consistency: s.text(row, &["consistency", "density"]),
                pid_ppm: s.num(row, &["pidppm", "pid"]),
                description: s.text(row, &["description", "desc"]),
            });
        }
        report_dropped(s, "lithology", &dropped, &mut found);
    }
    if let Some(s) = &samples {
        let mut dropped = Vec::new();
        for (at, row) in s.rows() {
            let Some(boring_id) = id_of(s, row, at, &mut found) else { continue };
            let Some((top_depth, bottom_depth)) = depths(s, row, at, &mut dropped) else { continue };
            set.samples.push(Sample {
                sample_id: s.text(row, &["sampleid", "sample"]),
                boring_id,
                sample_type: s.text(row, &["sampletype", "type"]),
                top_depth,
                bottom_depth,
                recovery: s.num(row, &["recovery", "recoveryin", "recoverypct"]),
                blow_counts: s.text(row, &["blowcounts", "blows", "spt"]),
                lab_submitted: s.flag(row, &["labsubmitted", "lab"]),
            });
        }
        report_dropped(s, "sample", &dropped, &mut found);
    }
    if let Some(s) = &wells {
        let mut dropped = Vec::new();
        for (at, row) in s.rows() {
            let Some(boring_id) = id_of(s, row, at, &mut found) else { continue };
            let Some((top_depth, bottom_depth)) = depths(s, row, at, &mut dropped) else { continue };
            set.wells.push(WellComponent {
                boring_id,
                component: s.text(row, &["component", "componenttype", "type"]),
                top_depth,
                bottom_depth,
                diameter: s.num(row, &["diameter", "diameterin"]),
                material: s.text(row, &["material"]),
                slot_size: s.text(row, &["slotsize", "slot"]),
            });
        }
        report_dropped(s, "well", &dropped, &mut found);
    }
    if let Some(s) = &water {
        for (at, row) in s.rows() {
            let Some(boring_id) = id_of(s, row, at, &mut found) else { continue };
            set.water.push(WaterLevel {
                boring_id,
                observed: s.text(row, &["observed", "observationdatetime", "observationdate", "date", "datetime", "measured"]),
                depth_to_water: s.num(row, &["depthtowater", "dtw", "waterdepth"]),
                kind: s.text(row, &["kind", "observationtype", "type"]),
            });
        }
    }
    found.extend(validate(&set));
    (set, found)
}

/// Cross-check a boring set: duplicate ids, inverted and overlapping intervals, rows that
/// name an unknown boring, borings without coordinates, then a closing summary.
pub fn validate(set: &BoringSet) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut known = HashSet::new();
    for b in &set.borings {
        if !known.insert(b.boring_id.as_str()) {
            let msg = format!("Boring {:?} is listed more than once", b.boring_id);
            out.push(finding(Severity::Error, "duplicate_boring_id", msg, Some(&b.boring_id), None));
        }
        if b.northing.is_none() || b.easting.is_none() {
            let msg = format!("Boring {:?} has no northing or easting", b.boring_id);
            out.push(finding(Severity::Warning, "missing_coordinates", msg, Some(&b.boring_id), None));
        }
    }

    let intervals = set
        .lithology
        .iter()
        .map(|l| ("lithology", &l.boring_id, l.top_depth, l.bottom_depth))
        .chain(set.samples.iter().map(|s| ("sample", &s.boring_id, s.top_depth, s.bottom_depth)))
        .chain(set.wells.iter().map(|w| ("well", &w.boring_id, w.top_depth, w.bottom_depth)));
    for (kind, id, top, bottom) in intervals {
        if top > bottom {
            let msg = format!("{id}: {kind} interval {top}-{bottom} has its top below its bottom");
            out.push(finding(Severity::Error, "inverted_interval", msg, Some(id), None));
        }
    }

    let mut by_boring: HashMap<&str, Vec<&Lithology>> = HashMap::new();
    for l in &set.lithology {
        by_boring.entry(&l.boring_id).or_default().push(l);
    }
    // Report borings in input order so findings are stable.
    let mut seen = HashSet::new();
    for l in &set.lithology {
        if !seen.insert(l.boring_id.as_str()) {
            continue;
        }
        let Some(ivs) = by_boring.get_mut(l.boring_id.as_str()) else { continue };
        ivs.sort_by(|a, b| a.top_depth.total_cmp(&b.top_depth));
        for pair in ivs.windows(2) {
            if let [a, b] = pair
                && a.bottom_depth > b.top_depth
            {
                let msg =
                    format!("{}: interval {}-{} overlaps the next interval starting at {}", l.boring_id, a.top_depth, a.bottom_depth, b.top_depth);
                out.push(finding(Severity::Error, "overlapping_intervals", msg, Some(&l.boring_id), None));
            }
        }
    }

    let orphans = set
        .lithology
        .iter()
        .map(|l| ("lithology", &l.boring_id, String::new()))
        .chain(set.samples.iter().map(|s| ("sample", &s.boring_id, format!("Sample {:?} ", s.sample_id))))
        .chain(set.wells.iter().map(|w| ("well", &w.boring_id, String::new())))
        .chain(set.water.iter().map(|w| ("water", &w.boring_id, String::new())));
    for (kind, id, what) in orphans {
        if !known.contains(id.as_str()) {
            let msg = if what.is_empty() {
                format!("A {kind} row references unknown boring {id:?}")
            } else {
                format!("{what}references unknown boring {id:?}")
            };
            out.push(finding(Severity::Warning, &format!("{kind}_boring_not_in_locations"), msg, Some(id), None));
        }
    }

    let msg = format!(
        "Boring package: {} locations, {} intervals, {} samples, {} well components, {} water levels.",
        set.borings.len(),
        set.lithology.len(),
        set.samples.len(),
        set.wells.len(),
        set.water.len()
    );
    out.push(finding(Severity::Info, "validation_complete", msg, None, None));
    out
}
