# Slice 6: spreadsheet I/O and subsurface drawings (Codex leads, Claude supports)

Same Paseo setup as slices 2 to 5. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice6]`.

**Codex does most of the implementation.** By owner request, Claude takes spreadsheet I/O and the boring import, plus the review of Codex's PR.

Read `CLAUDE.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`.

## Scope

All new library code lives in `crates/subsurface` (standalone: no workspace dependencies), ported from behaviour only:
- **Boring logs** from AutoGIS (`C:\Users\ichbi\AutoGIS`, read only): `core/envmon/import_boring_logs.py`, `boring_log_report.py`, `sticklog.py` and `subsurface_profile.py`, with their tests in `tests/envmon/test_import_boring_logs.py`, `test_boring_log_report.py`, `test_sticklog.py` and `test_subsurface_profile.py`. The reference reads a SQLite database and renders with matplotlib; here the input is a CSV folder or a workbook, and the output is drawing entities or tables.
- **Data extraction** in the spirit of Data Wizard, from `CIM-Project-Suite-REVENG/docs/phase2/features/data_wizard.md`. Treat it as a **behaviour description only**: that spec came from a commercial product. Use our own settings format, names and algorithms. Never copy its enum names, settings schema or any structure from it, and don't read anything else in that repo.

Dependencies are already on `main`: `calamine` (MIT) for reading and `rust_xlsxwriter` (MIT/Apache) for writing, both pure Rust, default features off.

Leave out:
- SQLite, PDF boring logs (tables and drawings only), photo logs, lab results and `soil_interval_selector.py` (lab data stays out of the fork by owner decision).
- Map Object Data, AEC property sets and external drawings or xrefs in data extraction: the active drawing only.

## Ownership

Shared files are already on `main` and pre-filled:
- `crates/subsurface/src/lib.rs` and `Cargo.toml`
- `table.rs`, which defines `Cell` and `Table` (what reads return and writes take)
- `boring.rs`, which defines `Boring`, `Lithology`, `Sample`, `WellComponent`, `WaterLevel`, `BoringSet` and `Finding`
- the `cmd.rs` registration, the workspace and engine `Cargo.toml` entries, `Cargo.lock` and `xtask/src/layers.rs`
- the five command labels in `zh.tsv`

Change a shared file only after agreeing it by message, and cite that message in the PR.

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (lead) | `slice6/codex-subsurface` | `crates/subsurface/src/sticklog.rs`, `profile.rs`, `report.rs`, `crates/subsurface/tests/sticklog*.rs`, `profile*.rs`, `report*.rs`, `crates/engine/src/cmd/subsurface_draw.rs`, `subsurface_report.rs`, `datawizard.rs`, `docs/agents/slice-6-notes-codex.md` | **Stick logs:** per boring, a depth-scaled lithology column hatched by USCS group using our own patterns in `crates/doc/src/library.rs`, in-band USCS labels, a sample lane, an optional well-construction column, interval descriptions with PID, and the first water level as a marker. **Profiles:** project borings onto a 2-point line within a tolerance, report the ones outside it, and lay out their columns at ground elevation with a vertical exaggeration. **Reports:** per-boring log tables and a combined sample summary. **Commands:** `sticklog` ("Boring Stick Logs") and `subsurfaceprofile` ("Subsurface Profile") draw into the drawing. `boringreport` ("Boring Log Report") draws Table entities or writes a workbook. `datawizard` ("Data Wizard") extracts properties of selected or filtered objects (built-in properties, block attributes, derived length and area) with grouping, sorting and column formats, to a Table entity or a workbook, and can refresh a table it made earlier. |
| Claude (support) | `slice6/claude-sheets` | `crates/subsurface/src/sheet.rs`, `import.rs`, `crates/subsurface/tests/sheet*.rs`, `import*.rs`, `crates/engine/src/cmd/subsurface_import.rs`, `docs/agents/slice-6-notes-claude.md` | **Spreadsheet I/O:** read XLSX, XLS, ODS and CSV from bytes into `Table`s, write `Table`s to XLSX bytes, with size and row caps and no panics on hostile files. **Import:** a boring package (a CSV folder or one workbook with one sheet per table) into a `BoringSet`, with the header aliases and validation from `import_boring_logs.py` (missing ids, inverted or overlapping intervals, orphans). **Command:** `boringimport` ("Import Boring Logs") validates a package, returns its findings and draws boring locations as points. **Review:** Claude reviews Codex's PR in full, running its tests and checking it against the references. |

## What to agree by message before coding against it

1. **Sheet and import API** (Claude proposes first, as its first message; Codex agrees). Function signatures, the finding codes, and how drawing commands get a `BoringSet`: from the package path on every run, or from data `boringimport` stores in the drawing. Claude should push a first working `sheet.rs` early, so Codex isn't blocked.
2. **Drawing conventions** (Codex proposes, Claude agrees). Layer names, the USCS-to-hatch table, column widths, text heights, and the default scale and vertical exaggeration for stick logs and profiles.
3. **Data extraction settings and table ownership** (Codex proposes, Claude agrees). The saved settings format (our own JSON), and how a table records that `datawizard` made it, so a refresh updates it instead of making a new one. Reuse slice 4's label ownership tag if it fits.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] Codex's diff is clearly the larger share of the slice.
- [ ] The AutoGIS import, sticklog, report and profile tests are ported and pass (rendering assertions become entity assertions). A workbook written by `rust_xlsxwriter` reads back through `sheet.rs` unchanged. A `boringimport` → `sticklog` → `subsurfaceprofile` → `boringreport` run works through the CLI or control channel, and a second `datawizard` run refreshes its table instead of adding one.
- [ ] Neither PR touches a file the other agent owns, or any slice 4 or 5 file. `cargo xtask ci` passes on Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice6/claude-sheets` first.
