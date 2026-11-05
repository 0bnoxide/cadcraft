//! Spreadsheet I/O: read XLSX and CSV (XLS and ODS are refused for now, see [`read`]) into [`crate::Table`]s, and write tables to
//! XLSX. Owner: Claude (see `docs/agents/slice-6-subsurface.md`).
//!
//! Input bytes are hostile: every size is capped before it is allocated, and over-cap input is
//! an error rather than a silent truncation.

use crate::{Cell, Table};
use std::fmt;
use std::io::Cursor;

/// Largest input file accepted.
pub const MAX_BYTES: usize = 64 << 20;
/// Largest total uncompressed size of a zipped workbook (XLSX, ODS).
pub const MAX_UNZIPPED: u64 = 256 << 20;
/// Most rows in one sheet.
pub const MAX_ROWS: usize = 100_000;
/// Most columns in one sheet.
pub const MAX_COLS: usize = 512;
/// Most cells across a whole workbook.
pub const MAX_CELLS: usize = 2_000_000;

#[derive(Clone, Debug, PartialEq)]
pub enum SheetError {
    /// The input is over one of the caps above.
    TooLarge(String),
    /// The input isn't a readable spreadsheet, or a table can't be written.
    Format(String),
}

impl fmt::Display for SheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SheetError::TooLarge(m) => write!(f, "spreadsheet too large: {m}"),
            SheetError::Format(m) => write!(f, "spreadsheet format error: {m}"),
        }
    }
}

impl std::error::Error for SheetError {}

fn too_large(what: &str) -> SheetError {
    SheetError::TooLarge(what.to_string())
}

/// Read a spreadsheet. XLSX, XLS and ODS are recognised by content (XLS and ODS return a
/// "not available yet" error); anything else is read as CSV and gives one table named after
/// `name`'s file stem. A non-finite number is a format error naming its cell.
pub fn read(bytes: &[u8], name: &str) -> Result<Vec<Table>, SheetError> {
    if bytes.len() > MAX_BYTES {
        return Err(too_large("file over 64 MiB"));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        check_unzipped(bytes)?;
        if is_ods(bytes) {
            return read_ods(bytes);
        }
        read_xlsx(bytes)
    } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        read_xls(bytes)
    } else {
        Ok(vec![read_csv(bytes, stem(name))?])
    }
}

/// An ODS package starts with an uncompressed `mimetype` entry naming its type.
fn is_ods(b: &[u8]) -> bool {
    b.get(30..38) == Some(b"mimetype") && b.get(38..90).is_some_and(|m| m.windows(28).any(|w| w == b"application/vnd.oasis.opendo"))
}

fn stem(name: &str) -> &str {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.rsplit_once('.').map_or(base, |(s, _)| s)
}

fn u16_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from(u16::from_le_bytes(b.get(i..i.checked_add(2)?)?.try_into().ok()?)))
}

fn u32_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(b.get(i..i.checked_add(4)?)?.try_into().ok()?)))
}

/// Sum the declared uncompressed sizes in the zip central directory, so a small compressed
/// workbook can't expand past [`MAX_UNZIPPED`]. Zip64 archives are refused.
/// shortcut: trusts declared sizes (the zip reader reads at most those), upgrade to a counting
/// reader if calamine ever streams past them.
fn check_unzipped(b: &[u8]) -> Result<(), SheetError> {
    let bad = || SheetError::Format("damaged zip directory".into());
    let from = b.len().saturating_sub(22 + 0xFFFF);
    let eocd = (from..b.len().saturating_sub(21)).rev().find(|&i| b.get(i..i + 4) == Some(b"PK\x05\x06")).ok_or_else(bad)?;
    let entries = u16_at(b, eocd + 10).ok_or_else(bad)?;
    let mut at = usize::try_from(u32_at(b, eocd + 16).ok_or_else(bad)?).map_err(|_| bad())?;
    if entries == 0xFFFF || at == 0xFFFF_FFFF {
        return Err(too_large("zip64 workbook"));
    }
    let mut total: u64 = 0;
    for _ in 0..entries {
        if b.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(bad());
        }
        let size = u32_at(b, at + 24).ok_or_else(bad)?;
        if size == 0xFFFF_FFFF {
            return Err(too_large("zip64 workbook"));
        }
        total = total.saturating_add(size);
        if total > MAX_UNZIPPED {
            return Err(too_large("workbook expands past 256 MiB"));
        }
        let names = [28, 30, 32].iter().try_fold(46u64, |n, o| Some(n + u16_at(b, at + o)?)).ok_or_else(bad)?;
        at = at.checked_add(usize::try_from(names).map_err(|_| bad())?).ok_or_else(bad)?;
    }
    Ok(())
}

/// "A1"-style name of a 0-based cell, for error messages.
fn cell_ref(row: u32, col: u32) -> String {
    let mut letters = String::new();
    let mut c = col + 1;
    while c > 0 {
        let d = (c - 1) % 26;
        letters.insert(0, char::from(b'A' + d as u8));
        c = (c - 1) / 26;
    }
    format!("{letters}{}", u64::from(row) + 1)
}

fn far_cell(sheet: &str, row: u64, col: u64) -> SheetError {
    let at = match (u32::try_from(row), u32::try_from(col)) {
        (Ok(r), Ok(c)) => cell_ref(r, c),
        _ => format!("row {} column {}", row.saturating_add(1), col.saturating_add(1)),
    };
    too_large(&format!("sheet {sheet:?} has a cell at {at} past the row or column cap"))
}

/// Store a non-empty cell at a position already checked against [`MAX_ROWS`] and
/// [`MAX_COLS`], padding the row with `Empty`. Padding counts toward `cells`, since the table
/// really holds it.
fn put(rows: &mut Vec<Vec<Cell>>, cells: &mut usize, r: usize, c: usize, value: Cell) -> Result<(), SheetError> {
    if rows.len() <= r {
        rows.resize_with(r + 1, Vec::new);
    }
    let Some(row) = rows.get_mut(r) else { return Ok(()) };
    if row.len() <= c {
        *cells = cells.saturating_add(c + 1 - row.len());
        if *cells > MAX_CELLS {
            return Err(too_large("more than 2,000,000 cells"));
        }
        row.resize(c + 1, Cell::Empty);
    }
    if let Some(slot) = row.get_mut(c) {
        *slot = value;
    }
    Ok(())
}

/// XLSX is streamed cell by cell so a hostile far-away cell is refused before anything is
/// allocated for it (calamine's `worksheet_range` allocates the dense bounding box first).
fn read_xlsx(bytes: &[u8]) -> Result<Vec<Table>, SheetError> {
    use calamine::{DataRef, Reader, Xlsx};
    let fmt_err = |e: calamine::XlsxError| SheetError::Format(e.to_string());
    let mut book: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(fmt_err)?;
    let mut cells = 0usize;
    let mut out = Vec::new();
    for name in book.sheet_names() {
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        let mut reader = book.worksheet_cells_reader(&name).map_err(fmt_err)?;
        while let Some(cell) = reader.next_cell().map_err(fmt_err)? {
            let (r, c) = cell.get_position();
            let (ri, ci) = (r as usize, c as usize);
            if ri >= MAX_ROWS || ci >= MAX_COLS {
                return Err(far_cell(&name, ri as u64, ci as u64));
            }
            let value = match cell.get_value() {
                DataRef::Empty => Cell::Empty,
                DataRef::Error(e) => {
                    return Err(SheetError::Format(format!("sheet {name:?} cell {} holds the error {e}", cell_ref(r, c))));
                }
                DataRef::String(s) => Cell::Text(s.clone()),
                DataRef::SharedString(s) => Cell::Text((*s).to_string()),
                DataRef::Bool(b) => Cell::Bool(*b),
                DataRef::Int(i) => Cell::Number(*i as f64),
                DataRef::Float(f) if f.is_finite() => Cell::Number(*f),
                DataRef::Float(_) => {
                    return Err(SheetError::Format(format!("sheet {name:?} cell {} is not a finite number", cell_ref(r, c))));
                }
                DataRef::DateTime(d) => Cell::Text(d.to_string()),
                DataRef::DateTimeIso(s) | DataRef::DurationIso(s) => Cell::Text(s.clone()),
            };
            if value == Cell::Empty {
                continue;
            }
            put(&mut rows, &mut cells, ri, ci, value)?;
        }
        out.push(Table { name, rows });
    }
    Ok(out)
}

/// One attribute by local name (namespace prefix ignored), unescaped.
fn attr(e: &quick_xml::events::BytesStart, local: &[u8]) -> Result<Option<String>, SheetError> {
    for a in e.attributes() {
        let a = a.map_err(|e| SheetError::Format(e.to_string()))?;
        if a.key.local_name().as_ref().as_bytes() == local {
            let v = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|e| SheetError::Format(e.to_string()))?;
            return Ok(Some(v.into_owned()));
        }
    }
    Ok(None)
}

/// A repeat count attribute: absent is 1; zero or unparsable is a format error.
fn repeat(e: &quick_xml::events::BytesStart, local: &[u8]) -> Result<u64, SheetError> {
    match attr(e, local)? {
        None => Ok(1),
        Some(v) => v.trim().parse::<u64>().ok().filter(|&n| n > 0).ok_or_else(|| SheetError::Format(format!("bad repeat count {v:?}"))),
    }
}

/// An ODS cell being read: its column repeat, typed value from attributes, and text content.
struct OdsCell {
    repeat: u64,
    value: Option<Cell>,
    text: String,
    paragraphs: usize,
}

/// The typed value an ODS cell's attributes give, if any.
fn ods_value(e: &quick_xml::events::BytesStart, row: u64, col: u64) -> Result<Option<Cell>, SheetError> {
    Ok(match attr(e, b"value-type")?.as_deref() {
        Some("float" | "percentage" | "currency") => {
            let v = attr(e, b"value")?.unwrap_or_default();
            let n = v.trim().parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| {
                let at = u32::try_from(row).ok().zip(u32::try_from(col).ok()).map_or(String::new(), |(r, c)| cell_ref(r, c));
                SheetError::Format(format!("cell {at} has the number value {v:?}, which is not a finite number"))
            })?;
            Some(Cell::Number(n))
        }
        Some("boolean") => Some(Cell::Bool(attr(e, b"boolean-value")?.is_some_and(|v| v.trim() == "true"))),
        Some("date") => attr(e, b"date-value")?.map(Cell::Text),
        Some("time") => attr(e, b"time-value")?.map(Cell::Text),
        Some("string") => attr(e, b"string-value")?.map(Cell::Text),
        _ => None,
    })
}

/// ODS is read straight from `content.xml`. Repeated rows and cells cost nothing until they
/// hold a value; every stored copy is checked against the caps first.
fn read_ods(bytes: &[u8]) -> Result<Vec<Table>, SheetError> {
    use quick_xml::events::Event;
    use std::io::Read;
    fn fmt_err(e: impl fmt::Display) -> SheetError {
        SheetError::Format(e.to_string())
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(fmt_err)?;
    let content = zip.by_name("content.xml").map_err(fmt_err)?;
    let mut xml = quick_xml::Reader::from_reader(std::io::BufReader::new(content.take(MAX_UNZIPPED)));
    let mut buf = Vec::new();
    let mut skip = Vec::new();
    let mut out = Vec::new();
    let mut cells = 0usize;
    // The open table, the position in it, the open row's repeat and values, the open cell.
    let mut table: Option<Table> = None;
    let (mut row, mut col, mut row_repeat) = (0u64, 0u64, 1u64);
    let mut row_values: Vec<(u64, u64, Cell)> = Vec::new();
    let mut cell: Option<OdsCell> = None;
    loop {
        buf.clear();
        let event = xml.read_event_into(&mut buf).map_err(fmt_err)?;
        let mut ended: Option<Vec<u8>> = None;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                let local = e.local_name().as_ref().as_bytes().to_vec();
                match local.as_slice() {
                    b"table" => {
                        table = Some(Table { name: attr(e, b"name")?.unwrap_or_default(), rows: Vec::new() });
                        row = 0;
                    }
                    b"table-row" => {
                        row_repeat = repeat(e, b"number-rows-repeated")?;
                        col = 0;
                        row_values.clear();
                    }
                    b"table-cell" | b"covered-table-cell" => {
                        let value = ods_value(e, row, col)?;
                        cell = Some(OdsCell { repeat: repeat(e, b"number-columns-repeated")?, value, text: String::new(), paragraphs: 0 });
                    }
                    b"annotation" if !empty => {
                        // A comment's text is not the cell's value.
                        xml.read_to_end_into(e.name(), &mut skip).map_err(fmt_err)?;
                        skip.clear();
                    }
                    b"p" if !empty => {
                        if let Some(c) = cell.as_mut() {
                            if c.paragraphs > 0 {
                                c.text.push('\n');
                            }
                            c.paragraphs += 1;
                        }
                    }
                    b"s" => {
                        if let Some(c) = cell.as_mut() {
                            // shortcut: a run of spaces is capped at 1,024; raise it if real text needs more.
                            let n = attr(e, b"c")?.and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(1).min(1024);
                            c.text.extend(std::iter::repeat_n(' ', n));
                        }
                    }
                    b"tab" => cell.iter_mut().for_each(|c| c.text.push('\t')),
                    b"line-break" => cell.iter_mut().for_each(|c| c.text.push('\n')),
                    _ => {}
                }
                if empty {
                    ended = Some(local);
                }
            }
            Event::End(e) => ended = Some(e.local_name().as_ref().as_bytes().to_vec()),
            Event::Text(t) => {
                if let Some(c) = cell.as_mut() {
                    c.text.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(c) = cell.as_mut() {
                    let ch = match r.resolve_char_ref().map_err(fmt_err)? {
                        Some(ch) => Some(ch),
                        None => match r.xml10_content().as_ref() {
                            "lt" => Some('<'),
                            "gt" => Some('>'),
                            "amp" => Some('&'),
                            "apos" => Some('\''),
                            "quot" => Some('"'),
                            _ => None,
                        },
                    };
                    c.text.extend(ch);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        match ended.as_deref() {
            Some(b"table-cell" | b"covered-table-cell") => {
                let Some(c) = cell.take() else { continue };
                let value = match c.value {
                    Some(v) => v,
                    None if !c.text.is_empty() => Cell::Text(c.text),
                    None => Cell::Empty,
                };
                if value != Cell::Empty {
                    if col.saturating_add(c.repeat) > MAX_COLS as u64 {
                        let name = table.as_ref().map_or("", |t| t.name.as_str());
                        return Err(far_cell(name, row, col.saturating_add(c.repeat) - 1));
                    }
                    row_values.push((col, c.repeat, value));
                }
                col = col.saturating_add(c.repeat);
            }
            Some(b"table-row") => {
                if let Some(t) = table.as_mut().filter(|_| !row_values.is_empty()) {
                    if row.saturating_add(row_repeat) > MAX_ROWS as u64 {
                        return Err(far_cell(&t.name, row.saturating_add(row_repeat) - 1, 0));
                    }
                    // Every index fits usize: rows are under MAX_ROWS, columns under MAX_COLS.
                    // `put` fails past MAX_CELLS, which bounds these loops.
                    for r in row..row + row_repeat {
                        for (c0, n, v) in &row_values {
                            for c in *c0..*c0 + *n {
                                put(&mut t.rows, &mut cells, r as usize, c as usize, v.clone())?;
                            }
                        }
                    }
                }
                row = row.saturating_add(row_repeat);
            }
            Some(b"table") => out.extend(table.take()),
            _ => {}
        }
    }
    Ok(out)
}

/// Little-endian u32s in `b`, ignoring a short tail.
fn u32s(b: &[u8]) -> impl Iterator<Item = u32> + '_ {
    b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))
}

/// The named stream from a compound file (the container XLS uses). Every sector chain is walked
/// at most once per sector in the file, so a cyclic or oversized chain is an error rather than a
/// loop, and nothing read can exceed the file's own size.
fn cfb_stream(b: &[u8], names: &[&str]) -> Result<Vec<u8>, SheetError> {
    let bad = |m: &str| SheetError::Format(format!("damaged XLS container: {m}"));
    let u16_ = |o: usize| u16_at(b, o).ok_or_else(|| bad("short header"));
    let u32_ = |o: usize| u32_at(b, o).map(|v| v as u32).ok_or_else(|| bad("short header"));
    let shift = u16_(0x1E)?;
    if !(shift == 9 || shift == 12) || u16_(0x20)? != 6 {
        return Err(bad("unsupported sector size"));
    }
    let ssz = 1usize << shift;
    let nsect = b.len().saturating_sub(512) / ssz + 1;
    let sector = |i: u32| -> Option<&[u8]> {
        let off = (i as usize).checked_add(1)?.checked_mul(ssz)?;
        b.get(off..off.checked_add(ssz)?.min(b.len())).filter(|s| !s.is_empty())
    };
    const LAST: u32 = 0xFFFF_FFFA; // ids from here up are markers, not sectors
    // FAT sector ids: 109 in the header, then the DIFAT chain.
    let mut fat_ids: Vec<u32> = (0..109).filter_map(|i| u32_at(b, 0x4C + 4 * i).map(|v| v as u32)).filter(|&v| v < LAST).collect();
    let mut next = u32_(0x44)?;
    let mut steps = 0;
    while next < LAST {
        steps += 1;
        if steps > nsect {
            return Err(bad("DIFAT chain loops"));
        }
        let s = sector(next).ok_or_else(|| bad("DIFAT sector past the end"))?;
        let ids: Vec<u32> = u32s(s).collect();
        let (link, body) = ids.split_last().ok_or_else(|| bad("empty DIFAT sector"))?;
        fat_ids.extend(body.iter().copied().filter(|&v| v < LAST));
        next = *link;
    }
    if fat_ids.len() > nsect {
        return Err(bad("more FAT sectors than the file holds"));
    }
    let mut fat = Vec::new();
    for id in fat_ids {
        let s = sector(id).ok_or_else(|| bad("FAT sector past the end"))?;
        fat.extend(u32s(s));
    }
    // A chain's sectors, in order, through `table`, at most `cap` long.
    let chain = |start: u32, table: &[u32], cap: usize| -> Result<Vec<u32>, SheetError> {
        let mut out = Vec::new();
        let mut s = start;
        while s < LAST {
            if out.len() >= cap {
                return Err(bad("sector chain loops"));
            }
            out.push(s);
            s = *table.get(s as usize).ok_or_else(|| bad("sector chain leaves the table"))?;
        }
        Ok(out)
    };
    let read = |start: u32| -> Result<Vec<u8>, SheetError> {
        let mut out = Vec::new();
        for s in chain(start, &fat, nsect)? {
            out.extend_from_slice(sector(s).ok_or_else(|| bad("sector past the end"))?);
        }
        Ok(out)
    };
    let dir = read(u32_(0x30)?)?;
    let v3 = u16_(0x1A)? == 3;
    let entry = |e: &[u8]| -> Option<(String, u8, u32, u64)> {
        let len = (u16_at(e, 0x40)? as usize).min(64).saturating_sub(2);
        let units: Vec<u16> = e.get(..len)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let size = u64::from_le_bytes(e.get(0x78..0x80)?.try_into().ok()?);
        Some((String::from_utf16_lossy(&units), *e.get(0x42)?, u32_at(e, 0x74)? as u32, if v3 { size & 0xFFFF_FFFF } else { size }))
    };
    let entries: Vec<_> = dir.as_chunks::<128>().0.iter().map(|e| entry(e)).collect();
    let root = entries.first().cloned().flatten().ok_or_else(|| bad("no root entry"))?;
    let (_, _, start, size) = entries
        .iter()
        .flatten()
        .find(|(n, t, _, _)| *t == 2 && names.iter().any(|w| n.eq_ignore_ascii_case(w)))
        .cloned()
        .ok_or_else(|| SheetError::Format("not an Excel workbook: no Workbook stream".into()))?;
    let size = usize::try_from(size).map_err(|_| bad("stream size"))?;
    if size > b.len() {
        return Err(bad("stream larger than the file"));
    }
    let mut data = if size < u32_(0x38)? as usize {
        // Small streams live in 64-byte mini sectors inside the root entry's stream.
        let mini = read(root.2)?;
        let mut minifat = Vec::new();
        for s in chain(u32_(0x3C)?, &fat, nsect)? {
            minifat.extend(u32s(sector(s).ok_or_else(|| bad("mini FAT past the end"))?));
        }
        let mut out = Vec::new();
        for m in chain(start, &minifat, mini.len() / 64 + 1)? {
            let off = (m as usize).checked_mul(64).ok_or_else(|| bad("mini sector"))?;
            out.extend_from_slice(mini.get(off..off + 64).ok_or_else(|| bad("mini sector past the end"))?);
        }
        out
    } else {
        read(start)?
    };
    if data.len() < size {
        return Err(bad("stream is cut short"));
    }
    data.truncate(size);
    Ok(data)
}

/// BIFF records from `pos`: (type, data, offset of the next record).
fn biff_record(s: &[u8], pos: usize) -> Option<(u16, &[u8], usize)> {
    let typ = u16_at(s, pos)? as u16;
    let len = u16_at(s, pos + 2)? as usize;
    let data = s.get(pos + 4..pos + 4 + len)?;
    Some((typ, data, pos + 4 + len))
}

/// Reads across a record and its CONTINUE records, as long strings in the SST span them.
struct Segments<'a> {
    segs: Vec<&'a [u8]>,
    i: usize,
    at: usize,
}

impl Segments<'_> {
    fn byte(&mut self) -> Option<u8> {
        loop {
            let seg = self.segs.get(self.i)?;
            if let Some(&b) = seg.get(self.at) {
                self.at += 1;
                return Some(b);
            }
            self.i += 1;
            self.at = 0;
        }
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        for _ in 0..n {
            self.byte()?;
        }
        Some(())
    }

    /// `cch` characters; a CONTINUE boundary inside them starts with a fresh high-byte flag.
    fn chars(&mut self, cch: usize, mut high: bool) -> Option<String> {
        let mut units = Vec::with_capacity(cch.min(4096));
        while units.len() < cch {
            if self.segs.get(self.i).is_some_and(|s| self.at >= s.len()) {
                self.i += 1;
                self.at = 0;
                high = self.byte()? & 1 != 0;
            }
            units.push(if high { self.u16()? } else { u16::from(self.byte()?) });
        }
        Some(String::from_utf16_lossy(&units))
    }
}

/// An XLUnicodeString at the start of `d` (cch u16, flags, characters), no CONTINUE.
fn xl_string(d: &[u8]) -> Option<String> {
    let mut s = Segments { segs: vec![d], i: 0, at: 0 };
    let cch = s.u16()? as usize;
    let high = s.byte()? & 1 != 0;
    s.chars(cch, high)
}

/// An RK number: a 30-bit integer or the top of a double, optionally divided by 100.
fn rk(v: u32) -> f64 {
    let n = if v & 2 != 0 { f64::from((v as i32) >> 2) } else { f64::from_bits(u64::from(v & 0xFFFF_FFFC) << 32) };
    if v & 1 != 0 { n / 100.0 } else { n }
}

/// XLS (BIFF8, Excel 97 to 2003), read by our own code: calamine allocates from hostile
/// header fields before any cap can apply. Values are numbers, text and booleans; dates stay
/// serial numbers.
/// shortcut: number formats are not read, so dates come back as serial numbers; read FORMAT
/// and XF records if a caller needs them as dates.
fn read_xls(bytes: &[u8]) -> Result<Vec<Table>, SheetError> {
    let s = cfb_stream(bytes, &["Workbook", "Book"])?;
    let bad = |m: String| SheetError::Format(m);
    let (typ, bof, _) = biff_record(&s, 0).ok_or_else(|| bad("empty XLS workbook".into()))?;
    if typ != 0x0809 || u16_at(bof, 0) != Some(0x0600) {
        return Err(bad("Excel 5/95 and older XLS workbooks are not supported; save as XLSX".into()));
    }
    // Workbook globals: worksheet names and positions, and the shared string table.
    let mut sheets: Vec<(String, usize)> = Vec::new();
    let mut strings: Vec<String> = Vec::new();
    let mut pos = 0;
    while let Some((typ, d, next)) = biff_record(&s, pos) {
        match typ {
            0x0085 if d.get(5) == Some(&0) => {
                let at = u32_at(d, 0).ok_or_else(|| bad("short sheet record".into()))? as usize;
                let name = d.get(6..).and_then(|n| {
                    let mut seg = Segments { segs: vec![n], i: 0, at: 0 };
                    let cch = seg.byte()? as usize;
                    let high = seg.byte()? & 1 != 0;
                    seg.chars(cch, high)
                });
                sheets.push((name.ok_or_else(|| bad("bad sheet name".into()))?, at));
            }
            0x00FC => {
                let mut segs = vec![d];
                let mut p = next;
                while let Some((0x003C, c, n)) = biff_record(&s, p) {
                    segs.push(c);
                    p = n;
                }
                let mut seg = Segments { segs, i: 0, at: 0 };
                seg.skip(8).ok_or_else(|| bad("short string table".into()))?;
                while let Some(cch) = seg.u16() {
                    let flags = seg.byte().ok_or_else(|| bad("cut string table".into()))?;
                    let runs = if flags & 8 != 0 { seg.u16().map(usize::from) } else { Some(0) };
                    let ext = if flags & 4 != 0 { seg.u32().map(|n| n as usize) } else { Some(0) };
                    let text = runs.zip(ext).and_then(|(r, e)| Some((seg.chars(cch as usize, flags & 1 != 0)?, r, e)));
                    let (text, runs, ext) = text.ok_or_else(|| bad("cut string table".into()))?;
                    seg.skip(runs * 4).and_then(|_| seg.skip(ext)).ok_or_else(|| bad("cut string table".into()))?;
                    strings.push(text);
                }
            }
            0x000A => break,
            _ => {}
        }
        pos = next;
    }
    let mut cells = 0usize;
    let mut out = Vec::new();
    for (name, start) in sheets {
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        let at = |r: u16, c: u16| cell_ref(u32::from(r), u32::from(c));
        let mut store = |rows: &mut Vec<Vec<Cell>>, r: u16, c: u16, v: Cell| -> Result<(), SheetError> {
            let (ri, ci) = (usize::from(r), usize::from(c));
            if ri >= MAX_ROWS || ci >= MAX_COLS {
                return Err(far_cell(&name, ri as u64, ci as u64));
            }
            match v {
                Cell::Number(n) if !n.is_finite() => Err(bad(format!("sheet {name:?} cell {} is not a finite number", at(r, c)))),
                Cell::Empty => Ok(()),
                v => put(rows, &mut cells, ri, ci, v),
            }
        };
        let (mut pos, mut depth, mut formula_at) = (start, 0usize, None);
        let rc = |d: &[u8]| Some((u16_at(d, 0)? as u16, u16_at(d, 2)? as u16));
        let short = || bad(format!("sheet {name:?} has a cut record"));
        loop {
            let Some((typ, d, next)) = biff_record(&s, pos) else {
                return Err(bad(format!("sheet {name:?} ends without an EOF record")));
            };
            pos = next;
            match (typ, depth) {
                (0x0809, _) if pos - d.len() - 4 != start => depth += 1,
                (0x000A, 0) => break,
                (0x000A, _) => depth -= 1,
                (_, 1..) => {}
                (0x0203, _) => {
                    let ((r, c), v) = rc(d).zip(d.get(6..14).and_then(|v| v.try_into().ok())).ok_or_else(short)?;
                    store(&mut rows, r, c, Cell::Number(f64::from_le_bytes(v)))?;
                }
                (0x027E, _) => {
                    let ((r, c), v) = rc(d).zip(u32_at(d, 6)).ok_or_else(short)?;
                    store(&mut rows, r, c, Cell::Number(rk(v as u32)))?;
                }
                (0x00BD, _) => {
                    let (r, first) = rc(d).ok_or_else(short)?;
                    let recs = d.get(4..d.len().saturating_sub(2)).filter(|b| b.len() % 6 == 0).ok_or_else(short)?;
                    for (i, rec) in recs.as_chunks::<6>().0.iter().enumerate() {
                        let c = u16::try_from(usize::from(first) + i).map_err(|_| short())?;
                        store(&mut rows, r, c, Cell::Number(rk(u32_at(rec, 2).ok_or_else(short)? as u32)))?;
                    }
                }
                (0x00FD, _) => {
                    let ((r, c), i) = rc(d).zip(u32_at(d, 6)).ok_or_else(short)?;
                    let text = strings.get(i as usize).ok_or_else(|| bad(format!("sheet {name:?} cell {} names a missing string", at(r, c))))?;
                    store(&mut rows, r, c, Cell::Text(text.clone()))?;
                }
                (0x0204 | 0x00D6, _) => {
                    let ((r, c), text) = rc(d).zip(d.get(6..).and_then(xl_string)).ok_or_else(short)?;
                    store(&mut rows, r, c, Cell::Text(text))?;
                }
                (0x0205, _) => {
                    let ((r, c), (v, err)) = rc(d).zip(d.get(6).zip(d.get(7))).ok_or_else(short)?;
                    if *err != 0 {
                        return Err(bad(format!("sheet {name:?} cell {} holds an error value", at(r, c))));
                    }
                    store(&mut rows, r, c, Cell::Bool(*v != 0))?;
                }
                (0x0006, _) => {
                    let ((r, c), v) = rc(d).zip(d.get(6..14)).ok_or_else(short)?;
                    formula_at = None;
                    if v.get(6..8) == Some(&[0xFF, 0xFF]) {
                        match v.first() {
                            Some(0) => formula_at = Some((r, c)), // text follows in a STRING record
                            Some(1) => store(&mut rows, r, c, Cell::Bool(v.get(2).is_some_and(|b| *b != 0)))?,
                            Some(2) => return Err(bad(format!("sheet {name:?} cell {} holds an error value", at(r, c)))),
                            Some(3) => {} // an empty string result
                            _ => return Err(bad(format!("sheet {name:?} cell {} has an unknown formula result type", at(r, c)))),
                        }
                    } else {
                        let n = f64::from_le_bytes(v.try_into().map_err(|_| short())?);
                        store(&mut rows, r, c, Cell::Number(n))?;
                    }
                }
                (0x0207, _) => {
                    if let Some((r, c)) = formula_at.take() {
                        store(&mut rows, r, c, Cell::Text(xl_string(d).ok_or_else(short)?))?;
                    }
                }
                _ => {}
            }
        }
        out.push(Table { name, rows });
    }
    Ok(out)
}

/// RFC 4180 CSV: quoted fields, doubled quotes, CRLF or LF. Invalid UTF-8 is replaced.
/// Cells are `Text`, or `Empty` when blank; [`Cell::number`] parses them on demand.
fn read_csv(bytes: &[u8], name: &str) -> Result<Table, SheetError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut cells = 0usize;
    let mut chars = text.chars().peekable();
    let mut push_field = |row: &mut Vec<Cell>, field: &mut String| -> Result<(), SheetError> {
        if row.len() >= MAX_COLS {
            return Err(too_large(&format!("more than {MAX_COLS} columns")));
        }
        cells += 1;
        if cells > MAX_CELLS {
            return Err(too_large("more than 2,000,000 cells"));
        }
        let f = std::mem::take(field);
        row.push(if f.trim().is_empty() { Cell::Empty } else { Cell::Text(f) });
        Ok(())
    };
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => push_field(&mut row, &mut field)?,
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n') | (false, '\r') => {
                push_field(&mut row, &mut field)?;
                if rows.len() >= MAX_ROWS {
                    return Err(too_large(&format!("more than {MAX_ROWS} rows")));
                }
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return Err(SheetError::Format(format!("{name}: a quoted field is never closed")));
    }
    if !field.is_empty() || !row.is_empty() {
        push_field(&mut row, &mut field)?;
        if rows.len() >= MAX_ROWS {
            return Err(too_large(&format!("more than {MAX_ROWS} rows")));
        }
        rows.push(row);
    }
    // A blank line is one empty field; keep it as an empty row.
    for r in &mut rows {
        if r.len() == 1 && r.first() == Some(&Cell::Empty) {
            r.clear();
        }
    }
    Ok(Table { name: name.to_string(), rows })
}

/// Sheet names: no `[]:*?/\`, at most 31 characters, not blank, unique (case-insensitively).
fn sheet_name(want: &str, used: &mut Vec<String>) -> String {
    let clean: String = want.chars().map(|c| if "[]:*?/\\".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim().trim_matches('\'');
    let base: String = if clean.is_empty() { "Sheet".into() } else { clean.chars().take(31).collect() };
    let mut name = base.clone();
    let mut n = 1;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
        n += 1;
        let tail = format!("~{n}");
        name = base.chars().take(31 - tail.len()).collect::<String>() + &tail;
    }
    used.push(name.clone());
    name
}

/// Write tables to XLSX bytes, one sheet per table. A non-finite number is an error.
///
/// XLSX stores no empty cells, so reading the result back gives the same table except that
/// trailing `Empty` cells in a row and trailing empty rows are gone, and an empty `Text` reads
/// back as `Empty` (both have the same [`Cell::text`]). Leading and inner blanks
/// keep their places.
pub fn write_xlsx(tables: &[Table]) -> Result<Vec<u8>, SheetError> {
    let fmt_err = |e: rust_xlsxwriter::XlsxError| SheetError::Format(e.to_string());
    let mut book = rust_xlsxwriter::Workbook::new();
    let mut used = Vec::new();
    let mut cells = 0usize;
    for t in tables {
        let sheet = book.add_worksheet();
        sheet.set_name(sheet_name(&t.name, &mut used)).map_err(fmt_err)?;
        if t.rows.len() > MAX_ROWS {
            return Err(too_large(&format!("table {:?} has more than {MAX_ROWS} rows", t.name)));
        }
        for (r, row) in t.rows.iter().enumerate() {
            if row.len() > MAX_COLS {
                return Err(too_large(&format!("table {:?} has more than {MAX_COLS} columns", t.name)));
            }
            cells = cells.saturating_add(row.len());
            if cells > MAX_CELLS {
                return Err(too_large("more than 2,000,000 cells"));
            }
            // Both fit: MAX_ROWS and MAX_COLS are far below u32 and u16.
            let r = u32::try_from(r).map_err(|_| too_large("row index"))?;
            for (c, cell) in row.iter().enumerate() {
                let c = u16::try_from(c).map_err(|_| too_large("column index"))?;
                match cell {
                    Cell::Text(s) => sheet.write_string(r, c, s).map(|_| ()),
                    Cell::Number(n) if n.is_finite() => sheet.write_number(r, c, *n).map(|_| ()),
                    Cell::Number(_) => {
                        let at = cell_ref(r, u32::from(c));
                        return Err(SheetError::Format(format!("table {:?} cell {at} is not a finite number", t.name)));
                    }
                    Cell::Bool(b) => sheet.write_boolean(r, c, *b).map(|_| ()),
                    Cell::Empty => Ok(()),
                }
                .map_err(fmt_err)?;
            }
        }
    }
    book.save_to_buffer().map_err(fmt_err)
}
