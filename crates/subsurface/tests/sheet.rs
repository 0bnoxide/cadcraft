use cadcraft_subsurface::sheet::{self, MAX_COLS, MAX_ROWS, SheetError};
use cadcraft_subsurface::{Cell, Table};

fn t(s: &str) -> Cell {
    Cell::Text(s.into())
}

#[test]
fn xlsx_round_trip_is_unchanged() {
    let tables = vec![
        Table {
            name: "boring_locations".into(),
            rows: vec![
                vec![t("BoringID"), t("Northing"), t("Easting"), t("Active")],
                vec![t("B-1"), Cell::Number(1000.5), Cell::Number(-2.25), Cell::Bool(true)],
                vec![t("B-2"), Cell::Empty, Cell::Number(3.0), Cell::Bool(false)],
            ],
        },
        Table {
            name: "lithology".into(),
            rows: vec![vec![t("BoringID"), Cell::Empty, t("USCS")], vec![], vec![t("B-1"), Cell::Number(1e-9), t("SM")]],
        },
    ];
    let bytes = sheet::write_xlsx(&tables).unwrap();
    let back = sheet::read(&bytes, "pkg.xlsx").unwrap();
    assert_eq!(back, tables);
}

#[test]
fn xlsx_round_trip_drops_trailing_blanks_and_empty_text() {
    let rows = vec![vec![t("x"), Cell::Empty], vec![Cell::Empty, t(""), t("y")], vec![Cell::Empty], vec![]];
    let back = sheet::read(&sheet::write_xlsx(&[Table { name: "s".into(), rows }]).unwrap(), "s.xlsx").unwrap();
    assert_eq!(back[0].rows, vec![vec![t("x")], vec![Cell::Empty, Cell::Empty, t("y")]]);
}

#[test]
fn error_cells_are_refused() {
    let wb = with_sheet(&sheet_xml(r#"<row r="1"><c r="A1" t="e"><v>#DIV/0!</v></c></row>"#));
    assert!(matches!(sheet::read(&wb, "x.xlsx"), Err(SheetError::Format(m)) if m.contains("A1")));
}

#[test]
fn sheet_names_are_sanitised_and_unique() {
    let long = "x".repeat(40);
    let tables = vec![
        Table { name: "a/b".into(), rows: vec![vec![t("1")]] },
        Table { name: "A_B".into(), rows: vec![vec![t("2")]] },
        Table { name: long.clone(), rows: vec![] },
        Table { name: "  ".into(), rows: vec![] },
    ];
    let back = sheet::read(&sheet::write_xlsx(&tables).unwrap(), "x.xlsx").unwrap();
    let names: Vec<_> = back.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names[0], "a_b");
    assert_eq!(names[1], "A_B~2");
    assert_eq!(names[2], "x".repeat(31));
    assert_eq!(names[3], "Sheet");
}

#[test]
fn non_finite_numbers_are_refused_on_write() {
    let tables = vec![Table { name: "s".into(), rows: vec![vec![t("a"), Cell::Number(f64::NAN)]] }];
    let err = sheet::write_xlsx(&tables).unwrap_err();
    assert!(matches!(&err, SheetError::Format(m) if m.contains("B1")), "{err}");
}

#[test]
fn write_caps_rows_and_columns() {
    let wide = vec![Table { name: "w".into(), rows: vec![vec![Cell::Empty; MAX_COLS + 1]] }];
    assert!(matches!(sheet::write_xlsx(&wide), Err(SheetError::TooLarge(_))));
    let tall = vec![Table { name: "t".into(), rows: vec![vec![]; MAX_ROWS + 1] }];
    assert!(matches!(sheet::write_xlsx(&tall), Err(SheetError::TooLarge(_))));
}

#[test]
fn csv_quotes_crlf_bom_and_blank_cells() {
    let src = "\u{feff}BoringID,Description,Top\r\nB-1,\"silty, \"\"fine\"\" sand\",0\r\n\r\nB-2,\"two\nlines\", \n";
    let tables = sheet::read(src.as_bytes(), "C:\\pkg\\lithology.csv").unwrap();
    assert_eq!(tables.len(), 1);
    let tb = &tables[0];
    assert_eq!(tb.name, "lithology");
    assert_eq!(tb.header(), ["BoringID", "Description", "Top"]);
    assert_eq!(tb.rows[1], vec![t("B-1"), t("silty, \"fine\" sand"), t("0")]);
    assert!(tb.rows[2].is_empty());
    assert_eq!(tb.rows[3], vec![t("B-2"), t("two\nlines"), Cell::Empty]);
    assert_eq!(tb.rows.len(), 4);
}

#[test]
fn csv_unterminated_quote_is_an_error() {
    assert!(matches!(sheet::read(b"a,\"open\nb,c\n", "q.csv"), Err(SheetError::Format(m)) if m.contains("never closed")));
}

#[test]
fn csv_caps() {
    let wide = ",".repeat(MAX_COLS);
    assert!(matches!(sheet::read(wide.as_bytes(), "w.csv"), Err(SheetError::TooLarge(_))));
    let tall = "a\n".repeat(MAX_ROWS + 1);
    assert!(matches!(sheet::read(tall.as_bytes(), "t.csv"), Err(SheetError::TooLarge(_))));
    let big = vec![b'a'; sheet::MAX_BYTES + 1];
    assert!(matches!(sheet::read(&big, "b.csv"), Err(SheetError::TooLarge(_))));
}

#[test]
fn truncated_xls_is_a_format_error() {
    let xls = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    assert!(matches!(sheet::read(&xls, "a.xls"), Err(SheetError::Format(m)) if m.contains("XLS")));
}

/// Build a minimal stored zip (no compression) from `(name, data)` entries.
fn zip(entries: &[(&str, &[u8])], lie_size: Option<u32>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let off = out.len() as u32;
        let size = lie_size.unwrap_or(data.len() as u32);
        let mut local = Vec::new();
        local.extend_from_slice(b"PK\x03\x04");
        local.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        local.extend_from_slice(&crc32(data).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&local);
        out.extend_from_slice(data);
        central.extend_from_slice(b"PK\x01\x02");
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        central.extend_from_slice(&crc32(data).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&off.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn zip_bomb_directory_is_too_large() {
    let bomb = zip(&[("a", b"x"), ("b", b"y")], Some(200 << 20));
    assert!(matches!(sheet::read(&bomb, "a.xlsx"), Err(SheetError::TooLarge(_))));
}

/// A minimal stored (uncompressed) XLSX with one sheet "s" whose XML is `sheet_xml`.
fn with_sheet(sheet_xml: &str) -> Vec<u8> {
    let parts: &[(&str, &str)] = &[
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="s" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet_xml),
    ];
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (*n, d.as_bytes())).collect();
    zip(&refs, None)
}

fn sheet_xml(cells: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{cells}</sheetData></worksheet>"#
    )
}

#[test]
fn hand_built_xlsx_reads_and_pads_to_a1() {
    let wb = with_sheet(&sheet_xml(r#"<row r="2"><c r="B2" t="inlineStr"><is><t>B-1</t></is></c><c r="C2"><v>4.5</v></c></row>"#));
    let back = sheet::read(&wb, "x.xlsx").unwrap();
    assert_eq!(back[0].rows, vec![vec![], vec![Cell::Empty, t("B-1"), Cell::Number(4.5)]]);
}

#[test]
fn far_away_cell_is_refused_before_allocation() {
    let wb = with_sheet(&sheet_xml(r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>2</v></c></row>"#));
    assert!(matches!(sheet::read(&wb, "x.xlsx"), Err(SheetError::TooLarge(m)) if m.contains("XFD1048576")));
}

#[test]
fn padding_counts_toward_the_cell_cap() {
    // One cell per row in the last allowed column: 100_000 x 512 padded cells is over the cap.
    let mut cells = String::new();
    for r in 1..=5_000 {
        cells.push_str(&format!(r#"<row r="{r}"><c r="SR{r}"><v>1</v></c></row>"#));
    }
    let wb = with_sheet(&sheet_xml(&cells));
    assert!(matches!(sheet::read(&wb, "x.xlsx"), Err(SheetError::TooLarge(_))));
}

#[test]
fn hostile_bytes_never_panic() {
    let good = sheet::write_xlsx(&[Table { name: "s".into(), rows: vec![vec![t("a"), Cell::Number(1.0)]] }]).unwrap();
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..300 {
        let mut b = good.clone();
        for _ in 0..8 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let i = (seed as usize) % b.len();
            b[i] = (seed >> 32) as u8;
        }
        let _ = sheet::read(&b, "f.xlsx");
        let _ = sheet::read(&b[..(seed as usize) % b.len()], "f.xlsx");
    }
    for junk in [&b"PK\x03\x04"[..], b"PK\x03\x04PK\x05\x06", &[0xD0, 0xCF, 0x11, 0xE0], b"\"unterminated", b""] {
        let _ = sheet::read(junk, "j");
    }
}

fn ods(tables: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>{tables}</office:spreadsheet></office:body></office:document-content>"#
    );
    zip(&[("mimetype", b"application/vnd.oasis.opendocument.spreadsheet"), ("content.xml", content.as_bytes())], None)
}

#[test]
fn ods_reads_types_repeats_paragraphs_and_skips_comments() {
    let wb = ods(concat!(
        r#"<table:table table:name="lithology"><table:table-column/>"#,
        r#"<table:table-row><table:table-cell office:value-type="string"><text:p>BoringID</text:p></table:table-cell>"#,
        r#"<table:table-cell table:number-columns-repeated="2"/><table:table-cell office:value-type="string"><text:p>Top</text:p></table:table-cell></table:table-row>"#,
        r#"<table:table-row table:number-rows-repeated="2"><table:table-cell office:value-type="string"><office:annotation><text:p>note</text:p></office:annotation><text:p>B&amp;1</text:p><text:p>two<text:s text:c="2"/>x</text:p></table:table-cell>"#,
        r#"<table:table-cell office:value-type="float" office:value="2.5" table:number-columns-repeated="2"><text:p>2.5</text:p></table:table-cell>"#,
        r#"<table:table-cell office:value-type="boolean" office:boolean-value="true"/></table:table-row>"#,
        r#"<table:table-row table:number-rows-repeated="1048000"><table:table-cell table:number-columns-repeated="16384"/></table:table-row>"#,
        r#"</table:table><table:table table:name="dates"><table:table-row><table:table-cell table:number-columns-repeated="3"/>"#,
        r#"<table:table-cell office:value-type="date" office:date-value="2026-01-02"><text:p>02/01/26</text:p></table:table-cell></table:table-row></table:table>"#,
    ));
    let back = sheet::read(&wb, "p.ods").unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(back[0].name, "lithology");
    assert_eq!(back[0].header(), ["BoringID", "", "", "Top"]);
    let data = vec![t("B&1\ntwo  x"), Cell::Number(2.5), Cell::Number(2.5), Cell::Bool(true)];
    assert_eq!(back[0].rows[1..], [data.clone(), data]);
    assert_eq!(back[1].rows, vec![vec![Cell::Empty, Cell::Empty, Cell::Empty, t("2026-01-02")]]);
}

#[test]
fn hostile_ods_repeats_are_refused_before_allocation() {
    let far_row = ods(
        r#"<table:table table:name="s"><table:table-row table:number-rows-repeated="1048000"><table:table-cell/></table:table-row><table:table-row><table:table-cell office:value-type="float" office:value="1"/></table:table-row></table:table>"#,
    );
    assert!(matches!(sheet::read(&far_row, "a.ods"), Err(SheetError::TooLarge(_))));
    let wide = ods(
        r#"<table:table table:name="s"><table:table-row><table:table-cell office:value-type="float" office:value="1" table:number-columns-repeated="4000000000"/></table:table-row></table:table>"#,
    );
    assert!(matches!(sheet::read(&wide, "a.ods"), Err(SheetError::TooLarge(_))));
    let tall = ods(
        r#"<table:table table:name="s"><table:table-row table:number-rows-repeated="99999"><table:table-cell office:value-type="float" office:value="1" table:number-columns-repeated="500"/></table:table-row></table:table>"#,
    );
    assert!(matches!(sheet::read(&tall, "a.ods"), Err(SheetError::TooLarge(m)) if m.contains("cells")));
    let overflow = ods(r#"<table:table table:name="s"><table:table-row table:number-rows-repeated="99999999999999999999999"/></table:table>"#);
    assert!(matches!(sheet::read(&overflow, "a.ods"), Err(SheetError::Format(_))));
    let nan = ods(
        r#"<table:table table:name="s"><table:table-row><table:table-cell office:value-type="float" office:value="NaN"/></table:table-row></table:table>"#,
    );
    assert!(matches!(sheet::read(&nan, "a.ods"), Err(SheetError::Format(m)) if m.contains("A1")));
}

/// CRC-32 (IEEE), as zip entries carry it.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[test]
fn hostile_ods_bytes_never_panic() {
    let good = ods(
        r#"<table:table table:name="s"><table:table-row table:number-rows-repeated="3"><table:table-cell office:value-type="float" office:value="1" table:number-columns-repeated="3"><text:p>a<text:s text:c="9"/></text:p></table:table-cell></table:table-row></table:table>"#,
    );
    assert!(sheet::read(&good, "g.ods").is_ok());
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..500 {
        let mut b = good.clone();
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let i = 90 + (seed as usize) % (b.len() - 90); // keep the mimetype so the ODS path runs
            b[i] = (seed >> 40) as u8;
        }
        let _ = sheet::read(&b, "f.ods");
    }
}

fn rec(typ: u16, data: &[u8]) -> Vec<u8> {
    let mut r = typ.to_le_bytes().to_vec();
    r.extend_from_slice(&(data.len() as u16).to_le_bytes());
    r.extend_from_slice(data);
    r
}

/// A row/column/xf prefix for a BIFF cell record.
fn rcx(r: u16, c: u16) -> Vec<u8> {
    [r.to_le_bytes(), c.to_le_bytes(), 0u16.to_le_bytes()].concat()
}

const BOF8: [u8; 16] = [0x00, 0x06, 0x05, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// A BIFF8 workbook stream: one sheet "Data" holding `cells` (already-encoded records), with
/// `sst` as the shared string table records (SST and any CONTINUE).
fn biff(sst: &[u8], cells: &[u8]) -> Vec<u8> {
    let sheet_rec_len = 4 + 4 + 2 + 2 + "Data".len();
    let globals_len = 4 + BOF8.len() + sheet_rec_len + sst.len() + 4;
    let mut s = rec(0x0809, &BOF8);
    let mut bs = (globals_len as u32).to_le_bytes().to_vec();
    bs.extend_from_slice(&[0, 0, 4, 0]);
    bs.extend_from_slice(b"Data");
    s.extend(rec(0x0085, &bs));
    s.extend_from_slice(sst);
    s.extend(rec(0x000A, &[]));
    assert_eq!(s.len(), globals_len);
    s.extend(rec(0x0809, &BOF8));
    s.extend_from_slice(cells);
    s.extend(rec(0x000A, &[]));
    s
}

/// A version 3 compound file holding `stream` as "Workbook"; streams under 4096 bytes go in
/// the mini stream. `cycle` makes the stream's sector chain point at itself.
fn cfb(stream: &[u8], cycle: bool) -> Vec<u8> {
    const END: u32 = 0xFFFF_FFFE;
    const FREE: u32 = 0xFFFF_FFFF;
    let mini = stream.len() < 4096;
    let mut body = stream.to_vec();
    if mini {
        body.resize(stream.len().div_ceil(64) * 64, 0);
    }
    let nbody = body.len().div_ceil(512);
    let first_body = if mini { 3 } else { 2 };
    let mut fat = vec![FREE; 128];
    fat[0] = 0xFFFF_FFFD;
    fat[1] = END;
    if mini {
        fat[2] = END;
    }
    for i in 0..nbody {
        fat[first_body + i] = if i + 1 == nbody { END } else { (first_body + i + 1) as u32 };
    }
    if cycle && !mini {
        fat[first_body] = first_body as u32;
    }
    let mut minifat = vec![FREE; 128];
    if mini {
        let n = body.len() / 64;
        for (i, m) in minifat.iter_mut().enumerate().take(n) {
            *m = if i + 1 == n { END } else { (i + 1) as u32 };
        }
        if cycle {
            minifat[0] = 0;
        }
    }
    let entry = |name: &str, typ: u8, start: u32, size: u32| {
        let mut e = vec![0u8; 128];
        let units: Vec<u8> = name.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        e[..units.len()].copy_from_slice(&units);
        e[0x40..0x42].copy_from_slice(&(units.len() as u16).to_le_bytes());
        e[0x42] = typ;
        e[0x44..0x50].copy_from_slice(&[0xFF; 12]);
        e[0x74..0x78].copy_from_slice(&start.to_le_bytes());
        e[0x78..0x7C].copy_from_slice(&size.to_le_bytes());
        e
    };
    let mut dir = if mini { entry("Root Entry", 5, first_body as u32, body.len() as u32) } else { entry("Root Entry", 5, END, 0) };
    dir.extend(entry("Workbook", 2, if mini { 0 } else { first_body as u32 }, stream.len() as u32));
    dir.resize(512, 0);
    let mut h = vec![0u8; 512];
    h[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    for (o, v) in [(0x18, 0x3Eu16), (0x1A, 3), (0x1C, 0xFFFE), (0x1E, 9), (0x20, 6)] {
        h[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    for (o, v) in [(0x2C, 1u32), (0x30, 1), (0x38, 4096), (0x3C, if mini { 2 } else { END }), (0x40, u32::from(mini)), (0x44, END), (0x48, 0)] {
        h[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    for i in 0..109 {
        h[0x4C + 4 * i..0x50 + 4 * i].copy_from_slice(&(if i == 0 { 0 } else { FREE }).to_le_bytes());
    }
    let words = |w: &[u32]| w.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
    let mut out = h;
    out.extend(words(&fat));
    out.extend(dir);
    if mini {
        out.extend(words(&minifat));
    }
    body.resize(nbody * 512, 0);
    out.extend(body);
    out
}

fn sample_cells() -> (Vec<u8>, Vec<u8>) {
    // SST: "BoringID" whole, then "Top" split by a CONTINUE that switches to UTF-16.
    let mut sst = 2u32.to_le_bytes().to_vec();
    sst.extend(2u32.to_le_bytes());
    sst.extend([8, 0, 0]);
    sst.extend(b"BoringID");
    sst.extend([3, 0, 0]);
    sst.extend(b"T");
    let mut sst_recs = rec(0x00FC, &sst);
    sst_recs.extend(rec(0x003C, &[1, b'o', 0, b'p', 0]));
    let mut cells = Vec::new();
    cells.extend(rec(0x00FD, &[rcx(0, 0), 0u32.to_le_bytes().to_vec()].concat()));
    cells.extend(rec(0x00FD, &[rcx(0, 1), 1u32.to_le_bytes().to_vec()].concat()));
    cells.extend(rec(0x0204, &[rcx(1, 0), vec![3, 0, 0], b"B-1".to_vec()].concat()));
    cells.extend(rec(0x0203, &[rcx(1, 1), 2.5f64.to_le_bytes().to_vec()].concat()));
    // MULRK at row 2, columns 1..=2: integer 7 and 1.23 (123 / 100).
    let mut mulrk = [2u16.to_le_bytes(), 1u16.to_le_bytes()].concat();
    mulrk.extend([0, 0]);
    mulrk.extend(((7u32 << 2) | 2).to_le_bytes());
    mulrk.extend([0, 0]);
    mulrk.extend(((123u32 << 2) | 3).to_le_bytes());
    mulrk.extend(2u16.to_le_bytes());
    cells.extend(rec(0x00BD, &mulrk));
    cells.extend(rec(0x0205, &[rcx(2, 3), vec![1, 0]].concat()));
    // A formula with a cached text result, then its STRING record.
    let mut f = rcx(3, 0);
    f.extend([0, 0, 0, 0, 0, 0, 0xFF, 0xFF]);
    f.extend([0; 6]);
    cells.extend(rec(0x0006, &f));
    cells.extend(rec(0x0207, &[vec![2, 0, 0], b"ok".to_vec()].concat()));
    (sst_recs, cells)
}

fn expected() -> Vec<Vec<Cell>> {
    vec![
        vec![t("BoringID"), t("Top")],
        vec![t("B-1"), Cell::Number(2.5)],
        vec![Cell::Empty, Cell::Number(7.0), Cell::Number(1.23), Cell::Bool(true)],
        vec![t("ok")],
    ]
}

#[test]
fn xls_reads_cells_from_regular_and_mini_streams() {
    let (sst, cells) = sample_cells();
    let stream = biff(&sst, &cells);
    assert!(stream.len() < 4096);
    let small = sheet::read(&cfb(&stream, false), "a.xls").unwrap();
    assert_eq!(small.len(), 1);
    assert_eq!(small[0].name, "Data");
    assert_eq!(small[0].rows, expected());
    let mut big = stream.clone();
    big.resize(5000, 0); // trailing bytes after the sheet EOF are ignored
    assert_eq!(sheet::read(&cfb(&big, false), "a.xls").unwrap()[0].rows, expected());
}

#[test]
fn hostile_xls_is_refused() {
    let (sst, cells) = sample_cells();
    let stream = biff(&sst, &cells);
    let mut big = stream.clone();
    big.resize(5000, 0);
    for cyclic in [cfb(&stream, true), cfb(&big, true)] {
        assert!(matches!(sheet::read(&cyclic, "c.xls"), Err(SheetError::Format(m)) if m.contains("loops")));
    }
    let far = rec(0x0203, &[rcx(0, 600), 1f64.to_le_bytes().to_vec()].concat());
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &far), false), "f.xls"), Err(SheetError::TooLarge(_))));
    let mut wide = [0u16.to_le_bytes(), 400u16.to_le_bytes()].concat();
    for _ in 0..200 {
        wide.extend([0, 0, 2, 0, 0, 0]);
    }
    wide.extend(599u16.to_le_bytes());
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &rec(0x00BD, &wide)), false), "w.xls"), Err(SheetError::TooLarge(_))));
    let err = rec(0x0205, &[rcx(0, 0), vec![7, 1]].concat());
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &err), false), "e.xls"), Err(SheetError::Format(m)) if m.contains("A1")));
    let nan = rec(0x0203, &[rcx(0, 0), f64::NAN.to_le_bytes().to_vec()].concat());
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &nan), false), "n.xls"), Err(SheetError::Format(_))));
    let mut odd = rcx(0, 0);
    odd.extend([9, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &rec(0x0006, &odd)), false), "o.xls"), Err(SheetError::Format(m)) if m.contains("formula")));
    let missing = rec(0x00FD, &[rcx(0, 0), 9u32.to_le_bytes().to_vec()].concat());
    assert!(matches!(sheet::read(&cfb(&biff(&sst, &missing), false), "m.xls"), Err(SheetError::Format(_))));
    let mut biff5 = biff(&sst, &cells);
    biff5[4..6].copy_from_slice(&0x0500u16.to_le_bytes());
    assert!(matches!(sheet::read(&cfb(&biff5, false), "5.xls"), Err(SheetError::Format(m)) if m.contains("not supported")));
}

#[test]
fn hostile_xls_bytes_never_panic() {
    let (sst, cells) = sample_cells();
    let mut big = biff(&sst, &cells);
    big.resize(5000, 0);
    for good in [cfb(&biff(&sst, &cells), false), cfb(&big, false)] {
        let mut seed = 0x1234_5678_9abc_def1u64;
        for _ in 0..500 {
            let mut b = good.clone();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let i = 8 + (seed as usize) % (b.len() - 8); // keep the signature so the XLS path runs
                b[i] = (seed >> 40) as u8;
            }
            let _ = sheet::read(&b, "f.xls");
        }
    }
}

#[test]
fn repeated_text_is_capped_before_it_is_copied() {
    let big = "x".repeat(300_000);
    let wb = ods(&format!(
        r#"<table:table table:name="s"><table:table-row table:number-rows-repeated="2"><table:table-cell office:value-type="string" table:number-columns-repeated="512"><text:p>{big}</text:p></table:table-cell></table:table-row></table:table>"#
    ));
    assert!(matches!(sheet::read(&wb, "t.ods"), Err(SheetError::TooLarge(m)) if m.contains("text")));
}

#[test]
fn ods_spaces_booleans_and_truncation_are_checked() {
    let cell = |inner: &str| ods(&format!(r#"<table:table table:name="s"><table:table-row>{inner}</table:table-row></table:table>"#));
    let huge = cell(r#"<table:table-cell office:value-type="string"><text:p>a<text:s text:c="999999999999"/></text:p></table:table-cell>"#);
    assert!(matches!(sheet::read(&huge, "s.ods"), Err(SheetError::TooLarge(_))));
    let bad = cell(r#"<table:table-cell office:value-type="string"><text:p>a<text:s text:c="-3"/></text:p></table:table-cell>"#);
    assert!(matches!(sheet::read(&bad, "s.ods"), Err(SheetError::Format(_))));
    let yes = cell(r#"<table:table-cell office:value-type="boolean" office:boolean-value="1"/>"#);
    assert_eq!(sheet::read(&yes, "b.ods").unwrap()[0].rows, vec![vec![Cell::Bool(true)]]);
    for v in [r#" office:boolean-value="maybe""#, ""] {
        let b = cell(&format!(r#"<table:table-cell office:value-type="boolean"{v}/>"#));
        assert!(matches!(sheet::read(&b, "b.ods"), Err(SheetError::Format(m)) if m.contains("A1")));
    }
    let cut = ods(r#"<table:table table:name="s"><table:table-row><table:table-cell office:value-type="float" office:value="1"/></table:table-row>"#);
    assert!(matches!(sheet::read(&cut, "c.ods"), Err(SheetError::Format(_))));
}
