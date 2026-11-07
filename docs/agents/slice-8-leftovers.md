# Slice 8: survey and drone leftovers (Codex codes, Claude reviews)

Same Paseo setup as slice 7. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice8]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews Codex's PR in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it.

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## Scope

Five small items that extend crates that already exist. No new crate, no new dependency. Port behaviour and the tests' intent, not the reference structure.

1. **Checkpoint CSV for `droneqa`.** From the AutoGIS reference repo (read only) `autogis/core/envmon/drone_checkpoint_qa.py` and `tests/envmon/test_drone_checkpoint_qa.py`: read a GCP checkpoint CSV (expected and measured XYZ per point), compute horizontal and vertical RMSE, and grade them against thresholds. Reuse the RMSE and grading code `droneqa` already has for PIX4D reports; the CSV is a second input, not a second engine.
2. **Supersede in `qareview`.** From the AIGIS3D reference repo (read only) `src/agol_uas_qa/workflow.py` (`supersede`, `SupersedesGlobalID`) and its tests in `tests/test_workflow.py` and `tests/test_workflow_adversarial.py`: commit a replacement draft that names the prior record, mark the prior record superseded, refuse a prior record from another mission or a draft that does not name it, and append an audit entry. Storage stays the drawing or JSON that slice 3 uses (no AGOL).
3. **Callout placement overrides.** From AutoGIS `autogis/core/envmon/manage_callout_overrides.py` and `tests/envmon/test_manage_callout_overrides.py`, `test_cli_callout_overrides.py`: a per-location manual placement (anchor, offset, preferred quadrant, locked) that a `labelbatch` or `callout` rerun keeps instead of re-placing. The reference stores rows in a GDB table; here they live in the drawing. Reuse slice 4's owner tag (`labels::template::{APP_ID, Owner}`) if it fits.
4. **Point File Converter.** From `CIM-Project-Suite-REVENG/docs/phase2/features/point_file_converter.md` only. Treat it as a **behaviour description only**: it came from a commercial product. Use our own names, settings and file format, never its enum names, settings schema or structure, and don't read anything else in that repo. Behaviour: translate the feature-code column of one or more point files (CSV or text) using a code map read with `subsurface::sheet` (XLSX/XLS/ODS/CSV), keep every other column and comment line unchanged, write one output per input with a prefix/suffix and optional tab delimiter, report unmatched codes, and refuse to overwrite an input or an existing output.

5. **Fix #29.** `newproposal` sets a sheet drawing's `MEASUREMENT` header to 0 when the plot units are inches, which turns a metric template imperial. Keep the template's `MEASUREMENT` and store plot units on the layout's page setup only (or fix the paper code that reads `MEASUREMENT`), with a regression test that fails before the fix. Close #29 from the PR.

Left out of this slice, by earlier owner decisions:
- `soil_interval_selector`: it tiers lab results against screening levels, and lab data stays out of the fork (2026-11-04).
- `landxml_transform`: it is a pyproj CRS reprojection with datum-grid selection, and the fork has no PROJ (pure Rust).

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice8/codex-leftovers` | the existing `crates/droneqa`, `crates/labels` and `crates/survey` sources and tests it needs, the matching `crates/engine/src/cmd/*.rs` (including `proposal.rs` for #29), `zh.tsv` rows for new labels, `docs/agents/slice-8-notes-codex.md` | The four items above. New commands: `pointconvert` ("Point File Converter") and `calloutoverride` ("Callout Override", actions set, clear and list). The checkpoint CSV is a new input to the existing `droneqa` command and supersede a new action of `qareview`, unless Claude agrees otherwise. |
| Claude (review) | `slice8/claude-review` | `docs/agents/slice-8-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews Codex's PR in full, runs its tests, checks it against the references, and posts findings on the PR. |

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Checkpoint CSV and supersede.** The CSV header aliases and finding codes, how it reaches the existing grading code, and the supersede params, refusal codes and audit entry.
2. **Override storage.** Where an override lives in the drawing, how a rerun finds it, and what happens when its source location is gone.
3. **Point File Converter.** The code-map layout, which column holds the code, comment and multi-code handling, and output naming and collision rules.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branch. Claude's branch changes only its notes file.
- [ ] The listed AutoGIS and AIGIS3D tests are ported (adapted to our storage) and pass. Through the CLI or control channel: a checkpoint CSV grades a mission; a supersede marks the old record and a cross-mission supersede is refused; a `labelbatch` rerun keeps an overridden callout where it was put; `pointconvert` writes converted files and lists unmatched codes, and a second run onto the same outputs is refused; a `newproposal` sheet from a metric template keeps `MEASUREMENT` 1 with inch plot units.
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice8/codex-leftovers` first, then Claude's notes.
