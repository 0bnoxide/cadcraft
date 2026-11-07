//! Convert only feature-code fields; reserve outputs without overwriting anything.
use super::{CommandSpec, bad, str_param};
use crate::{Result, Session};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const ID: &str = "pointconvert";
const MAX: u64 = 25_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(ID,"Point File Converter",run).enabled(super::always).noundo().params("{inputs:[path],map_path,code_column:1-based,delimiter?:comma|tab,prefix?:string,suffix?:_converted,output_dir?:path,tab_output?:false,code_separator?:character} -> {files,unmatched,short_rows}")]
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    std::fs::File::open(path).and_then(|f| f.take(MAX + 1).read_to_end(&mut b)).map_err(|_| bad(ID, "cannot read input"))?;
    if b.len() as u64 > MAX {
        return Err(bad(ID, "input exceeds 25 MB"));
    }
    Ok(b)
}

/// Return original field spans and decoded values, retaining quoting in untouched fields.
fn fields(row: &str, delimiter: u8) -> Result<Vec<(&str, String)>> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = row.as_bytes();
    let mut quoted = false;
    let mut i = 0;
    while i <= bytes.len() {
        if bytes.get(i) == Some(&b'"') {
            quoted = !quoted;
        }
        if i == bytes.len() || bytes.get(i) == Some(&delimiter) && !quoted {
            if quoted || out.len() >= 512 {
                return Err(bad(ID, "malformed or oversized row"));
            }
            let raw = row.get(start..i).ok_or_else(|| bad(ID, "invalid field boundary"))?;
            let value = if raw.starts_with('"') {
                let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')).ok_or_else(|| bad(ID, "unclosed quoted field"))?;
                let decoded = inner.replace("\"\"", "\"");
                if inner.replace("\"\"", "").contains('"') {
                    return Err(bad(ID, "malformed quoted field"));
                }
                decoded
            } else {
                if raw.contains('"') {
                    return Err(bad(ID, "malformed field"));
                }
                raw.into()
            };
            out.push((raw, value));
            start = i + 1;
        }
        i += 1;
    }
    Ok(out)
}

fn append(out: &mut String, value: &str, limit: usize) -> Result<()> {
    // Bound expansion before allocation: an allocator abort cannot be caught by Session's guard.
    if out.len().checked_add(value.len()).is_none_or(|len| len > limit) {
        return Err(bad(ID, "output exceeds 25 MB"));
    }
    out.try_reserve(value.len()).map_err(|_| bad(ID, "cannot allocate converted output"))?;
    out.push_str(value);
    Ok(())
}

fn encode(out: &mut String, value: &str, delimiter: u8, limit: usize) -> Result<()> {
    if value.contains([delimiter as char, '"', '\r', '\n']) {
        append(out, "\"", limit)?;
        for part in value.split_inclusive('"') {
            append(out, part, limit)?;
            if part.ends_with('"') {
                append(out, "\"", limit)?;
            }
        }
        append(out, "\"", limit)
    } else {
        append(out, value, limit)
    }
}

fn translate(
    value: &str,
    separator: Option<char>,
    map: &BTreeMap<String, String>,
    unmatched: &mut BTreeSet<String>,
    remaining: usize,
) -> Result<String> {
    let mut out = String::new();
    for (index, code) in value.split(|c| Some(c) == separator).enumerate() {
        if index > 0
            && let Some(sep) = separator
        {
            append(&mut out, sep.encode_utf8(&mut [0; 4]), remaining)?;
        }
        match map.get(code) {
            Some(replacement) => append(&mut out, replacement, remaining)?,
            None => {
                append(&mut out, code, remaining)?;
                unmatched.insert(code.into());
            }
        }
    }
    Ok(out)
}

fn map_text(cell: Option<&cadcraft_subsurface::Cell>) -> Result<String> {
    // A compressed sheet can contain a cell larger than the file-read cap.
    match cell {
        Some(cadcraft_subsurface::Cell::Text(value)) if value.len() as u64 > MAX => Err(bad(ID, "map field exceeds 25 MB")),
        Some(cadcraft_subsurface::Cell::Text(value)) => Ok(value.clone()),
        Some(cell) => Ok(cell.text()),
        None => Ok(String::new()),
    }
}

fn run(_: &mut Session, p: &Value) -> Result<Value> {
    for key in ["delimiter", "prefix", "suffix", "output_dir", "map_path"] {
        if p.get(key).is_some_and(|v| !v.is_string()) {
            return Err(bad(ID, "path and naming parameters must be strings"));
        }
    }
    let paths =
        p.get("inputs").and_then(Value::as_array).filter(|a| !a.is_empty() && a.len() <= 100).ok_or_else(|| bad(ID, "1 to 100 inputs required"))?;
    let column =
        p.get("code_column").and_then(Value::as_u64).filter(|n| (1..=512).contains(n)).ok_or_else(|| bad(ID, "code_column must be 1 to 512"))?
            as usize
            - 1;
    let delimiter = match str_param(p, "delimiter").unwrap_or("comma") {
        "comma" => b',',
        "tab" => b'\t',
        _ => return Err(bad(ID, "delimiter must be comma or tab")),
    };
    let tab = match p.get("tab_output") {
        None => false,
        Some(Value::Bool(b)) => *b,
        _ => return Err(bad(ID, "tab_output must be boolean")),
    };
    let separator = match p.get("code_separator") {
        None => None,
        Some(Value::String(s)) if s.chars().count() == 1 => s.chars().next(),
        _ => return Err(bad(ID, "code_separator must be one character")),
    };
    let prefix = str_param(p, "prefix").unwrap_or("");
    let suffix = str_param(p, "suffix").unwrap_or("_converted");
    if prefix.len() + suffix.len() > 128 || [prefix, suffix].iter().any(|s| s.contains(['/', '\\', ':']) || *s == "." || *s == "..") {
        return Err(bad(ID, "invalid output naming"));
    }
    let map_path = Path::new(str_param(p, "map_path").ok_or_else(|| bad(ID, "map_path required"))?);
    let sheets = cadcraft_subsurface::sheet::read(&read(map_path)?, map_path.to_str().ok_or_else(|| bad(ID, "invalid map path"))?)
        .map_err(|e| bad(ID, e.to_string()))?;
    let sheet = sheets.first().ok_or_else(|| bad(ID, "map has no sheet"))?;
    let from = sheet.column("from").ok_or_else(|| bad(ID, "from header required"))?;
    let to = sheet.column("to").ok_or_else(|| bad(ID, "to header required"))?;
    let mut map = BTreeMap::new();
    for row in sheet.rows.iter().skip(1) {
        if row
            .iter()
            .all(|c| matches!(c, cadcraft_subsurface::Cell::Empty) || matches!(c, cadcraft_subsurface::Cell::Text(value) if value.trim().is_empty()))
        {
            continue;
        }
        let key = map_text(row.get(from))?;
        let value = map_text(row.get(to))?;
        if key.is_empty() || map.insert(key, value.clone()).is_some_and(|old| old != value) {
            return Err(bad(ID, "blank or conflicting map key"));
        }
    }
    let mut inputs = BTreeSet::new();
    let mut planned = Vec::new();
    let mut destinations = BTreeSet::new();
    let mut unmatched = BTreeSet::new();
    let mut short_rows = Vec::new();
    let mut total_bytes = 0_usize;
    for input in paths {
        let path = Path::new(input.as_str().ok_or_else(|| bad(ID, "input must be path"))?);
        let canonical = path.canonicalize().map_err(|_| bad(ID, "input missing"))?;
        if !inputs.insert(canonical.clone()) {
            return Err(bad(ID, "duplicate input"));
        }
        let parent = str_param(p, "output_dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| canonical.parent().unwrap_or(Path::new(".")).to_path_buf())
            .canonicalize()
            .map_err(|_| bad(ID, "output directory missing"))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| bad(ID, "invalid filename"))?;
        let extension = path.extension().and_then(|s| s.to_str()).map(|s| format!(".{s}")).unwrap_or_default();
        let target = parent.join(format!("{prefix}{stem}{suffix}{extension}"));
        let identity = target.to_string_lossy().to_lowercase();
        if target.exists() || !destinations.insert(identity) {
            return Err(bad(ID, "output collision"));
        }
        let bytes = read(path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| bad(ID, "input must be UTF-8"))?;
        let mut result = String::new();
        let (bom, text) = text.strip_prefix('\u{feff}').map_or(("", text), |s| ("\u{feff}", s));
        append(&mut result, bom, MAX as usize)?;
        for (index, line) in text.split_inclusive('\n').enumerate() {
            let body = line.trim_end_matches(['\r', '\n']);
            let ending = line.get(body.len()..).unwrap_or("");
            if body.trim_start().starts_with(['#', ';']) || body.trim().is_empty() {
                append(&mut result, line, MAX as usize)?;
                continue;
            }
            let parsed = fields(body, delimiter)?;
            if column >= parsed.len() {
                short_rows.push(json!({"file":path,"row":index+1}));
                append(&mut result, line, MAX as usize)?;
                continue;
            }
            for (i, (original, value)) in parsed.iter().enumerate() {
                if i > 0 {
                    append(&mut result, (if tab { '\t' } else { delimiter as char }).encode_utf8(&mut [0; 4]), MAX as usize)?;
                }
                let translated = if i == column {
                    Some(translate(value, separator, &map, &mut unmatched, (MAX as usize).saturating_sub(result.len()))?)
                } else {
                    None
                };
                let value = translated.as_deref().unwrap_or(value);
                if tab {
                    if value.contains(['\t', '\r', '\n']) {
                        return Err(bad(ID, "tab output cannot contain embedded tabs or newlines"));
                    }
                    append(&mut result, value, MAX as usize)?;
                } else if let Some(value) = translated {
                    encode(&mut result, &value, delimiter, MAX as usize)?;
                } else {
                    append(&mut result, original, MAX as usize)?;
                }
            }
            append(&mut result, ending, MAX as usize)?;
        }
        total_bytes = total_bytes.saturating_add(result.len());
        if total_bytes > 64_000_000 {
            return Err(bad(ID, "batch exceeds 64 MB"));
        }
        planned.push((target, result));
    }
    // Reserve every destination before writing, so a collision never leaves converted files.
    let mut reserved = Vec::new();
    for (path, _) in &planned {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => reserved.push((path.clone(), file)),
            Err(_) => {
                for (path, file) in reserved {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                }
                return Err(bad(ID, "output collision or permission denied"));
            }
        }
    }
    for ((_, text), (_, file)) in planned.iter().zip(reserved.iter_mut()) {
        if file.write_all(text.as_bytes()).and_then(|_| file.sync_all()).is_err() {
            for (path, file) in reserved {
                drop(file);
                let _ = std::fs::remove_file(path);
            }
            return Err(bad(ID, "output write failed"));
        }
    }
    Ok(json!({"files":planned.iter().map(|(p,_)|p).collect::<Vec<_>>(),"unmatched":unmatched,"short_rows":short_rows}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separated_replacements_refuse_expansion_beyond_remaining_output_budget() {
        let map = BTreeMap::from([("A".into(), "12345".into())]);
        let mut unmatched = BTreeSet::new();
        assert!(translate("A;A;Z", Some(';'), &map, &mut unmatched, 10).is_err());
        assert!(unmatched.is_empty(), "stop before inspecting later codes, rather than checking after collect/join");
        assert_eq!(translate("A;A", Some(';'), &map, &mut BTreeSet::new(), 11).unwrap(), "12345;12345");
    }

    #[test]
    fn escaped_output_and_utf8_separators_count_bytes_before_appending() {
        let mut out = "1234".to_string();
        assert!(encode(&mut out, "a\"b", b',', 9).is_err());
        assert!(out.len() <= 9);
        let mut exact = "1234".to_string();
        encode(&mut exact, "a\"b", b',', 10).unwrap();
        assert_eq!(exact, "1234\"a\"\"b\"");
        let map = BTreeMap::from([("A".into(), "é".into())]);
        assert!(translate("A💠A", Some('💠'), &map, &mut BTreeSet::new(), 7).is_err());
        assert_eq!(translate("A💠A", Some('💠'), &map, &mut BTreeSet::new(), 8).unwrap(), "é💠é");
    }

    #[test]
    fn oversized_map_value_is_refused_before_cloning() {
        let cell = cadcraft_subsurface::Cell::Text("x".repeat(MAX as usize + 1));
        assert!(map_text(Some(&cell)).is_err());
    }
}
