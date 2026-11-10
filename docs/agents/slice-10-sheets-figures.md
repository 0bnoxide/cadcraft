# Slice 10: sheets and figures (Codex codes, Claude reviews)

Same Paseo setup as slices 7 to 9. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice10]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews Codex's PR(s) in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it.

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## Scope

Two new commands that turn drawings into outgoing files, plus two open follow-up issues. Neither command ever writes a drawing. Both keep the slice 7 shape: inputs, validate, deterministic plan, preview, approval bound to the plan hash, staged execution, verify, receipt. Both plot through the existing `plot` command path (`crates/engine/src/cmd/layout.rs`, `plotters.rs`); add no second plotting route.

References, read only. Port the behaviour and guardrails, not the ArcGIS or Civil 3D API:

- The AutoGIS repository (path in the kickoff): `autogis/core/envmon/map_series_plan.py`, `layout_manager.py`, `export_figures.py`, `report_figure_package.py`, the figure spec skeleton `autogis/config/_templates/site_skeleton/figure_spec.yaml`, and the tests `tests/envmon/test_export_figures_preexport_qa.py` and `test_report_figure_package.py`.
- The AutoGIS-Civil3D repository (path in the kickoff): `docs/superpowers/specs/2026-09-11-follow-on-workflow-candidates-design.md`, candidate 2 (Package Deliverables).

1. **Export Figures** (`figureexport`). Inputs: one or more figure spec files (JSON, closed fields), a list of site labels, a list of event labels, a series mode, an output format (`pdf` or `png`) and an output folder.
   - **Figure spec.** Names a source drawing, one layout, the required layers, the visible and hidden layers, a text map, an optional extent boundary layer with a buffer percentage, and a figure id. A missing or empty layout name is an error, never "every layout".
   - **Plan (the map series planner).** Expand site x event x spec into an ordered job list with deterministic, unique, filesystem-safe output names. Port the four modes of `map_series_plan.py` with the same grouping order and naming, and deduplicate inputs keeping their order. With `S`, `E`, `F` the site, event and figure id made filesystem-safe (each run of characters outside `A-Za-z0-9_.-` becomes `_`): `per_site` loops site, event, spec and names `S_E_F`; `per_map_type` loops spec, site, event and names `F_S_E`; `historical` loops event, site, spec and names `S_F_E`; `combined_appendix` loops like `per_site` and names `Appendix_NNN_S_E_F` with a three-digit running number from 001. Site and event are plain labels used for naming and text; they carry no lab-data meaning.
   - **Prepare, in memory only.** For each job, open the source drawing and apply the spec to the in-memory copy: turn the listed layers on or off; replace text by `{{Name}}` placeholder in the layout's TEXT, MTEXT and block attribute values (the job's site, event and figure id are always available as values); and, when a boundary layer is named, set the layout's viewport view to the extents of that layer's model-space entities plus the buffer. The source drawing file is never written; verify that its hash is unchanged after the run.
   - **Pre-export QA.** Block the job on: a missing layout, an xref or image whose file can't be found, a visible or required layer that doesn't exist. Warn on: a required layer with no entities, a placeholder left without a value, a boundary layer that is missing or empty (the view then stays as saved). "Could not check" is reported as such and never read as "checked and fine".
   - **Export.** Plot each job's layout to the staged output, then publish with slice 9's no-replace move. An existing output name is never overwritten: the new file gets a `_v2`, `_v3` suffix, and the plan shows the final name. The receipt lists every job with its output path, SHA-256, and QA records.
   - Not portable, leave out: data-source repathing, definition queries, the GDB export registry, TIFF and JPEG output.
2. **Package Deliverables** (`packagedeliverables`). Inputs: a proposal root made by `newproposal`, and a deliverable profile file (JSON, closed fields). Owner decision: this one command also covers the reference figure package (`report_figure_package.py`), so there is one packaging writer, not two.
   - **Profile.** Which sheets ship (sheet roles from the proposal's sheet index, or all), the PDF naming rule, whether the sheet drawings travel with the PDFs, and an optional list of extra files, each with a role (`figure_pdf`, `figure_png`, `data_csv`, `qa_report`, `boring_log`, `other`) that decides its subfolder.
   - **Plan.** One plot action per selected sheet, using that sheet drawing's own layout and page setup. When drawings travel, the reference closure of each selected sheet (xrefs and images, followed with a bounded depth). One copy action per extra file. One checksum manifest and one ZIP holding exactly the planned files.
   - **Preflight.** A selected sheet whose drawing or layout is missing fails preflight and nothing is plotted. A missing extra file, a link or reparse point, a destination name the `paths` rules reject, and two files that would land on the same destination are refused before anything is written.
   - **Verify.** PDF count equals selected sheet count and each PDF maps to one sheet; every file in the ZIP is in the plan and every planned file is in the ZIP; each entry's hash matches the manifest; the ZIP's hash is in the receipt. The package folder and ZIP are staged and published with the no-replace move; a failure leaves no partial package and no success receipt.
   - **Never:** edits a drawing, overwrites an earlier package, emails or uploads anything.
   - The ZIP writer uses the `zip` crate already in `Cargo.lock` (`crates/subsurface` depends on it; MIT). Bound entry counts and sizes. Do not touch `crates/handoff`: contract v1 is frozen.
   - Lab-data roles from the reference (`compliance_table`, `coc`, `source_gdb`) are out, per the owner's lab-data decision.
3. **Follow-ups.** Close each from the PR:
   - **#44:** a lock file left by a killed run blocks every later run. Fix as the issue describes; the new commands use the same lock.
   - **#45:** the clean-ups listed in the issue and its comments (duplicate `reject_link`, the empty `_superseded/` folder after a rollback, no `Failed` receipt on disk, register-only intake rewriting `BaseModel`, derived extents z). If one turns out not to be worth its code, say why on the issue instead.

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice10/codex-figures` (and `slice10/codex-package` if split) | A new `crates/figures` crate for the host-free parts (spec and profile parsing, series planning, naming, QA record types, package plan) if that keeps `engine` thin, else `crates/proposal`; `crates/engine/src/cmd/figureexport.rs` and `packagedeliverables.rs` with their registration in `cmd.rs`; `crates/engine/src/cmd/proposal.rs` and `crates/proposal` for #44 and #45 and for sharing the staging helpers; `zh.tsv` rows for new labels; `Cargo.toml`/`Cargo.lock` only for the new crate and `zip`; `docs/agents/slice-10-notes-codex.md` | Items 1 to 3. New commands: `figureexport` ("Export Figures") and `packagedeliverables` ("Package Deliverables"), each with the same plan, preview and approve flow as `newproposal`. |
| Claude (review) | `slice10/claude-review` | `docs/agents/slice-10-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews Codex's PR(s) in full, runs the tests, checks them against the references, and posts findings on the PR. |

This slice is large. Codex should split it into two stacked PRs: item 1 first, then items 2 and 3. Say so by message first. A new crate must respect `cargo xtask layers`.

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Figure spec and series plan.** The spec schema and its validation, the job naming per mode, the versioned-name rule, and the QA record codes and severities.
2. **In-memory preparation and plotting.** How a drawing is opened and changed without touching the session's current drawing or the file, how the viewport view is set, which text objects take placeholders, and how `plot` is called for a layout of that in-memory drawing. Confirm in the code what `plot` can do before proposing; if it can't plot a drawing that isn't the session's current one, propose the smallest change that makes it possible.
3. **Deliverable profile and package.** The profile schema, the PDF naming rule, the reference closure walk and its bounds, the folder layout, the checksum manifest format, and the ZIP limits.

## Owner decisions

These were open owner inputs in the references. The owner confirmed all five on 2026-11-10; they are written into the scope above.

1. One PDF per sheet (not one multi-page PDF per deliverable), so the count rule is "PDFs equal selected sheets".
2. Deliverable profile defaults: all sheets ship, PDFs are named `<sheet number>_<sheet title>.pdf` after the `paths` rules, and drawings do not travel unless the profile says so.
3. One packaging command: the figure package is the "extra files" part of `packagedeliverables`.
4. Figure specs and the deliverable profile are standalone JSON files passed to the commands. The proposal manifest stays at version 2.
5. Site and event are plain labels. Nothing in this slice reads lab data.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branch(es). Claude's branch changes only its notes file.
- [ ] Through the CLI or control channel:
  - `figureexport` with two sites, two events and one spec in `per_site` mode plans four uniquely named jobs, and the plan is byte-identical on a second run;
  - an approved run writes four PDFs whose text shows the replaced placeholders, and the source drawing's hash is unchanged;
  - a second approved run writes `_v2` files and overwrites nothing;
  - a spec naming a missing layout, and a drawing with an unresolved xref, are each blocked with the issue code and nothing is written for that job;
  - on a project made by `newproposal`, `packagedeliverables` produces one PDF per selected sheet and a ZIP whose entries equal the plan, with hashes matching the checksum manifest;
  - a profile selecting a sheet whose layout is missing fails preflight and plots nothing;
  - a changed sheet drawing after planning makes the approval stale.
- [ ] Forced failure tests show no partial output and no success receipt after a failed publish, for both commands.
- [ ] Issues #44 and #45 are closed by the PR(s), or closed with the reason an item was not done.
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, and each PR merges without anyone fixing a conflict by hand. Merge Codex's PR(s) first, then Claude's notes.
