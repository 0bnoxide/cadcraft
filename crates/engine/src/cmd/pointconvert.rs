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

fn encode(value: &str, delimiter: u8) -> String {
    if value.contains([delimiter as char, '"', '\r', '\n']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value.into() }
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
        if row.iter().all(|c| c.text().is_empty()) {
            continue;
        }
        let raw = |i| match row.get(i) {
            Some(cadcraft_subsurface::Cell::Text(s)) => s.clone(),
            Some(c) => c.text(),
            None => String::new(),
        };
        let key = raw(from);
        let value = raw(to);
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
        result.push_str(bom);
        for (index, line) in text.split_inclusive('\n').enumerate() {
            let body = line.trim_end_matches(['\r', '\n']);
            let ending = line.get(body.len()..).unwrap_or("");
            if body.trim_start().starts_with(['#', ';']) || body.trim().is_empty() {
                result.push_str(line);
                continue;
            }
            let parsed = fields(body, delimiter)?;
            if column >= parsed.len() {
                short_rows.push(json!({"file":path,"row":index+1}));
                result.push_str(line);
                continue;
            }
            for (i, (original, value)) in parsed.iter().enumerate() {
                if i > 0 {
                    result.push(if tab { '\t' } else { delimiter as char });
                }
                let translated = if i == column {
                    let convert = |code: &str, misses: &mut BTreeSet<String>| -> String {
                        map.get(code).cloned().unwrap_or_else(|| {
                            misses.insert(code.into());
                            code.into()
                        })
                    };
                    Some(if let Some(sep) = separator {
                        value.split(sep).map(|c| convert(c, &mut unmatched)).collect::<Vec<_>>().join(&sep.to_string())
                    } else {
                        convert(value, &mut unmatched)
                    })
                } else {
                    None
                };
                let value = translated.as_deref().unwrap_or(value);
                if tab {
                    if value.contains(['\t', '\r', '\n']) {
                        return Err(bad(ID, "tab output cannot contain embedded tabs or newlines"));
                    }
                    result.push_str(value);
                } else if let Some(value) = translated {
                    result.push_str(&encode(&value, delimiter));
                } else {
                    result.push_str(original);
                }
            }
            result.push_str(ending);
            if result.len() as u64 > MAX {
                return Err(bad(ID, "output exceeds 25 MB"));
            }
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
