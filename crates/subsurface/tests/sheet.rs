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
fn xls_and_ods_report_not_available() {
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
        local.extend_from_slice(&0u32.to_le_bytes()); // crc
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&local);
        out.extend_from_slice(data);
        central.extend_from_slice(b"PK\x01\x02");
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        central.extend_from_slice(&0u32.to_le_bytes());
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
fn ods_is_refused_and_zip_bomb_directory_is_too_large() {
    let ods = zip(&[("mimetype", b"application/vnd.oasis.opendocument.spreadsheet")], None);
    assert!(matches!(sheet::read(&ods, "a.ods"), Err(SheetError::Format(m)) if m.contains("ODS")));
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
