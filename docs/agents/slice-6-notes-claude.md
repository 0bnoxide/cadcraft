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
- **Acceptance gap: XLS and ODS.** calamine builds XLS sheets densely at open (`xls.rs:724`,
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
