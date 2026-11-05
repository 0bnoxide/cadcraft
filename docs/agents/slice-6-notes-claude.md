# Slice 6 notes (Claude, support)

## Message log (UTC)

| Time | Dir | Summary |
|---|---|---|
| 21:36 | Codex → Claude | Hello; asks for sheet/import API first |
| 21:37 | Claude → Codex | Sheet/import API proposal: signatures, caps, finding codes, stateless `path` loading, `BORING-LOC` |
| 21:40 | Codex → Claude | AGREED API; asks for non-finite rejection and expansion/aggregate caps |
| 21:41 | Claude → Codex | Adds MAX_CELLS, MAX_UNZIPPED zip pre-check |
| 21:44 | Codex → Claude | Drawing conventions and extraction settings/XDATA ownership proposal |
| 21:44 | Codex → Claude | Non-finite must be a Format error naming the cell; verify zip bound |
| 21:52 | Claude → Codex | Non-finite = error; zip bound verified; XLS/ODS refused (dense-range abort); AGREED drawing + extraction contracts |
| 21:55 | Codex → Claude | AGREED XLSX streaming and interim XLS/ODS refusal; document the gap |
| 21:58 | Claude → Codex | sheet.rs pushed @ dc7d845, draft PR #20 |
| 22:05 | Codex → Claude | Ack; GRAVEL pattern absent, DOTS fallback; asks for import head |
| 22:15 | Codex → Claude | Proposes stacking its branch on my final head |
| 22:16 | Claude → Codex | AGREED stacking; exposes `load` / `findings_json` |
| 22:20 | Codex → Claude | Review: bounded reads, package budget, CSV quote, aliases |
| 22:30 | Codex → Claude | Review: round-trip shape, error cells |
| 22:45 | Claude → Codex | Final head d6854ab, all findings fixed |
| 22:55 | Codex → Claude | Asks for real XLS/ODS support; shared Cargo.toml allowed by agreement |
| 23:00 | Claude → Codex | Plan: own ODS reader (zip + quick-xml), XLS preflight |
| 23:05 | Codex → Claude | AGREED Cargo.toml lines (msg e4a7ca35) and plan; round-trip normalisation accepted |
| 23:25 | Claude → Codex | XLS: own BIFF8/CFB reader instead of the calamine preflight |
| 23:30 | Codex → Claude | AGREED; adds guards; proposes Data Wizard aggregates |
| 23:31 | Claude → Codex | AGREED aggregates |
| 23:45 | Codex → Claude | PR #21 up for review @ f523009 |

## Spreadsheet safety

- **Zip expansion bound.** `check_unzipped` sums the central directory's declared uncompressed
  sizes (cap `MAX_UNZIPPED` = 256 MiB, zip64 refused) before calamine opens the file. The bound
  is real because zip 9.1.3 `read/readers.rs` `make_reader` wraps every entry, stored or
  compressed, in `Crc32Reader::with_size_limit(declared size)`.
- **Sparse-cell abort.** calamine's `worksheet_range` → `Range::from_sparse` (calamine 0.38
  `lib.rs:1051`) allocates the dense bounding box of the used cells. Two cells at `A1` and
  `XFD1048576` request ~17 G cells, and a failed allocation aborts the process. XLSX is
  therefore streamed with `Xlsx::worksheet_cells_reader`, and any cell past `MAX_ROWS` or
  `MAX_COLS` is refused before allocation. Padding counts toward `MAX_CELLS`. Regressions:
  `far_away_cell_is_refused_before_allocation` and `padding_counts_toward_the_cell_cap` in
  `crates/subsurface/tests/sheet.rs`.
- **XLS and ODS (superseded below).** calamine builds XLS sheets densely at open (`xls.rs:724`,
  up to 65,536 × 65,536) and ODS up to 1,048,576 × 16,384, with no streaming API. Both return
  "not available yet; save as XLSX or CSV" for now. A bounded reader needs a CFB parser
  (XLS) or `zip` + XML access (ODS), which means new dependencies or a shared `Cargo.toml`
  change. Agreed with Codex as an interim measure (21:55).

## Import and command

- `import::load(path)` reads a folder of `*.csv` files (at most 64, sorted) or one workbook or
  CSV file, then calls `from_tables` and `validate`. Header aliases cover the AutoGIS
  `import_boring_logs.py` names (`BoringID`, `TopDepth_ft`, …) plus spaced/underscored forms.
- Finding codes as agreed at 21:40Z. `*_rows_dropped_missing_depth` carries the first
  dropped row as its source.
- `boringimport {path, layer?, text_height?}` draws a POINT and an id TEXT per located boring
  on `BORING-LOC`. It returns the findings plus `unlocated` ids. `subsurface_import::load`
  and `findings_json` are `pub(crate)` for Codex's commands.
- Ported AutoGIS tests: `crates/subsurface/tests/import.rs`. The arcpy-only import test
  has no counterpart.
- Review fixes from Codex (22:20Z and 22:30Z): bounded `File::take` reads, a shared 64 MiB
  and 2 M-cell budget per package folder, at most 10,000 directory entries, `read_dir`
  errors propagated, an error on an unterminated CSV quote, and the reference aliases
  `component_type`, `observation_datetime` and `observation_type`. XLSX error cells are a
  Format error naming the cell. Round-trip scope is documented on `write_xlsx` and tested:
  trailing blanks and empty text don't survive XLSX.

## XLS and ODS readers (agreed 23:05Z and 23:30Z)

- **ODS:** our own reader streams `content.xml` (zip 9 + quick-xml 0.42; the shared
  `Cargo.toml` change was agreed in Paseo message e4a7ca35). Repeated rows and cells cost
  nothing until they hold a value. Each stored copy is checked against the caps first.
  Tests: `ods_reads_types_repeats_paragraphs_and_skips_comments`,
  `hostile_ods_repeats_are_refused_before_allocation` and `hostile_ods_bytes_never_panic`.
- **XLS:** calamine also allocates from a hostile DIMENSIONS record (`reserve(rows*cols)`),
  underflows a `u16` in MULRK and calls `with_capacity` on length fields. So XLS uses our own
  bounded CFB reader (chains capped at the file's sector count, mini stream supported) and a
  BIFF8 cell reader. Calamine never sees XLS. BIFF5 and older is a clear Format error. Dates
  stay serial numbers (`shortcut:` comment). Tests:
  `xls_reads_cells_from_regular_and_mini_streams`, `hostile_xls_is_refused` (chain cycles,
  far cells, a wide MULRK, error cells, NaN, unknown formula types, missing strings, BIFF5)
  and `hostile_xls_bytes_never_panic`.
- Codex ODS review (msg ec142613): a workbook-wide `MAX_TEXT` (256 MiB) budget is checked in
  `put` before any clone, which covers shared and repeated strings in XLSX, ODS and XLS.
  `text:s` counts are validated against it. ODS booleans must be true/false/1/0. A table
  left open at end of content is a Format error. Tests: `repeated_text_is_capped_before_it_is_copied`
  and `ods_spaces_booleans_and_truncation_are_checked`.
- My review of #21 @ f523009 (comment 6670976425): changes requested. Codex accepted all
  findings (msg 12ff981e), including a signed, unclamped profile station.
- Codex static review of 2f2fedb (msg 45655b59): new row slots count toward `MAX_CELLS` before
  any resize. `MAX_SHEETS` = 256 applies to XLSX, ODS and XLS. A text formula without its
  STRING record is a Format error, the SST must hold its declared unique count, and the XLS
  globals must end with EOF. Tests: `far_rows_on_many_sheets_and_too_many_sheets_are_refused`
  and `xls_lost_formula_text_cut_sst_and_missing_globals_eof_are_errors`.
