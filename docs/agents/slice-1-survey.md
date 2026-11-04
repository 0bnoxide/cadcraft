# Slice 1: survey points (Claude + Codex)

This slice is split between two agents running as Paseo sessions on the same machine. It is also a test of whether that setup works, so follow the process below exactly and record what goes wrong.

Read `CLAUDE.md` first. Its rules apply to both agents: never crash, no `unwrap`/`expect`/indexing on input, `cargo xtask ci` before every push, clean-room.

## Ownership

Each agent edits **only** its own files. The scaffold (this file, `crates/survey/src/lib.rs`, `point.rs`, the `Cargo.toml` files, `cmd.rs` registration) is already on `main`. Nobody edits it without the other agent's agreement in a PR comment.

| Agent | Branch | Files | Ports (AutoGIS reference, behaviour only) |
|---|---|---|---|
| Claude | `slice1/claude-rtk` | `crates/survey/src/rtk.rs`, `crates/survey/src/control.rs`, `crates/survey/tests/rtk*.rs`, `crates/engine/src/cmd/survey_points.rs`, `docs/agents/slice-1-notes-claude.md` | `import_rtk_survey.py` (headered and headerless CSV, coordinate-order detection, fix-type normalisation), `validate_rtk_survey.py`, `assign_qa_flags`, `rtk_control_check.py` |
| Codex | `slice1/codex-levels` | `crates/survey/src/level_loop.rs`, `crates/survey/src/export.rs`, `crates/survey/tests/level*.rs`, `crates/survey/tests/export*.rs`, `crates/engine/src/cmd/survey_level.rs`, `docs/agents/slice-1-notes-codex.md` | `level_loop.py` (`process_level_loop`, `select_elevations_for_update`; ADR-0026 rules), `civil3d_points.py` (PNEZD), `export_survey_cad.py` (layer grouping, CSV, LandXML CgPoints), `cad_layer_map.py` (mapping resolution, CRS check) |

Leave out anything that writes to a geodatabase or calls arcpy (`update_well_elevations`, the `.pyt` paths). Port the reference tests in `AutoGIS/tests/test_level_loop.py`, `test_rtk_control_check.py` and `test_export_survey_cad.py` as Rust tests.

## Shared contract

- Points are `cadcraft_survey::SurveyPoint`. Messages are `Finding` (`Warning` / `Error`), in the order found.
- Parsers take `impl Read` (and file paths only in the engine command), never panic, and cap row counts at 1,000,000.
- Units are the input's own; do not convert feet to metres.
- New dependencies must be pure Rust and MIT/Apache/BSD. Add them to `crates/survey/Cargo.toml` only, and say so in the PR.
- Commands follow the existing `CommandSpec` pattern (see `crates/engine/src/cmd/handoff.rs`). Command ids: `rtkimport`, `controlcheck` (Claude); `levelloop`, `pointexport` (Codex).

## Process

1. Each agent branches from `main`, works only on its files, and opens a PR against `main`.
2. Each agent reviews the **other** agent's PR: read the diff, run its tests locally, leave at least one concrete comment (a defect, a missing case, or an explicit "checked X, no issue").
3. The author answers every comment with a fix or a reason.
4. Greg merges. The second PR rebases onto `main` if needed.
5. If you need something from the other agent (a contract change, a question), ask in a PR comment, not by editing their files.

## Pass criteria for the Paseo test

- [ ] Both sessions ran in Paseo on ValiantObn0xid3 against the same clone, on separate branches (or worktrees).
- [ ] Neither PR touches a file the other agent owns.
- [ ] Each PR has a review from the other agent, with every comment answered.
- [ ] `cargo xtask ci` passes on Windows for each PR head, and again on `main` after both merge.
- [ ] Both PRs merged without Greg resolving a conflict by hand.
- [ ] Each agent writes its notes file (what worked, what broke, how long it took) in its own PR.
