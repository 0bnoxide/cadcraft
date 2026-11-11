# Slice 10 notes: reviewer

Review-only role: the designs are settled by message, the code is on the author's branch(es), and this branch changes only this file.

## Message log

Times are local (UTC-07:00).

| Time | Direction | Summary |
|---|---|---|
| 2026-11-09 19:35 | sent | Hello with my agent id; asked for the three design proposals (spec and series plan, in-memory preparation and plotting, profile and package). |
| 2026-11-09 19:35 | received | Acknowledged. The author will send all three proposals together after reading the plot and proposal code and the references. Two stacked PRs: `figureexport` on `slice10/codex-figures`, then `packagedeliverables` with #44 and #45 on `slice10/codex-package`. |
| 2026-11-09 19:38 | received | The three design proposals in one message. |
| 2026-11-09 19:39 | sent | Agreed all three with six changes (listed below). |
| 2026-11-09 19:41 | received | Every change accepted. No fixed configuration path exists: `Project::open` needs `proposal_root` and `manifest_path`, so the author asked to add `manifest_path` to the `packagedeliverables` inputs. |
| 2026-11-09 19:41 | sent | Confirmed the `manifest_path` input (same inputs as the slice 9 follow-ons, no discovery). Proposal 3 fully agreed. |
| not recorded | received | Four messages that arrived while the reviewer was stopped by a usage limit: progress on the figure tests; PR #50 up at `fbc48358`; PR #52 up at `de9cdcd6`; both PRs green in hosted CI, with #53 filed. No reply was sent. |
| 2026-11-10 | received | From the owner's coordinator: #50, #52 and #56 were merged without review; do an after-the-fact review; the author's agent is closed, so send it nothing. |

## Design agreements

Agreed by message at 2026-11-09 19:39, before any dependent code. Checked first: issues #44 and #45, `run_plot` in `crates/engine/src/cmd/layout.rs`, and the reference `export_figures.py`, `map_series_plan.py`, `layout_manager.py` and the figure spec skeleton.

1. **Figure spec and series plan.** Agreed as proposed (closed JSON spec, four modes named as the brief says, bounds of 1,000 jobs and a 1 MiB spec, QA records with `error` and `warning` severities, whole-batch refusal on any QA error, which matches the reference's `export_blocked`). Changes asked for:
   - a name collision inside one batch is a validation error; `_vN` is only for names already on disk;
   - empty sites, events or specs after deduplication, and empty labels or figure ids, are errors;
   - a spec text map that defines `Site`, `Event` or `FigureId` is a validation error, not a silent override.
2. **In-memory preparation and plotting.** Agreed: a temporary session holds the prepared drawing and the existing `plot` command runs on it (`run_plot` needs only the session's drawing and the global plot hook, so `layout.rs` does not change). Changes asked for:
   - with several content viewports, only the largest by paper area takes the boundary view, so an inset keeps its saved view;
   - a test that the session's current drawing and undo state are untouched.
3. **Deliverable profile and package.** Agreed (closed profile, `checksums.json`, stored ZIP of at most 4,096 entries, 512 MiB per file and 2 GiB in total, references outside the root refused when drawings travel). Changes asked for:
   - no JSON discovery scan under the root; settled at 19:41 as a `manifest_path` input beside `proposal_root` and `profile_path`, reusing `Project::open` as `configuredrawing` and `intakesource` do;
   - one `_vN` number for the package folder and its ZIP together.
   - #44 as the issue describes (process id and UTC start time in the lock, never broken automatically); #45 all five items.

## Review

The usage limit stopped the reviewer before any review. #50, #52 and #56 were merged without one, and the owner asked for a review after the merge. It was done on `main` at `4ce9dd15`, on Windows, on 2026-11-10. No verdict message was sent because the author's agent was closed; the findings are on the PRs.

**Read in full:** `crates/proposal/src/figures.rs` and `deliverables.rs`, `crates/engine/src/cmd/figureexport.rs` and `packagedeliverables.rs`, both engine test files, and the changes to `proposal.rs`, `intakesource.rs`, `configuredrawing.rs`, `manifest.rs` and the manifests.

**Tests run:**
- `cargo nextest run -p cadcraft-proposal -p cadcraft-engine`: 1,700 passed, 0 failed.
- `cargo test -p cadcraft-engine --lib`: 529 passed, 5 failed. The five are new forced-failure tests that depend on which test sets the process-wide file hooks first (#66).
- Not run: `cargo xtask ci`, the CLI pass-criteria runs, anything on Linux.

**Findings posted:** [#50](https://github.com/0bnoxide/cadcraft/pull/50#issuecomment-6786916647), [#52](https://github.com/0bnoxide/cadcraft/pull/52#issuecomment-6786917037), [#56](https://github.com/0bnoxide/cadcraft/pull/56#issuecomment-6786917399). Nothing found needs a revert.

**The agreed design changes are all in the code:** in-batch name collisions are errors; empty labels and reserved text keys are errors; only the largest viewport takes the boundary view; the session's drawing and undo state are tested unchanged; `manifest_path` with `Project::open` and no discovery scan; one version number for the package folder and ZIP.

**Pass criteria this review cannot confirm:** the messaging counts; "agreed before the dependent code was committed" for the review half (the designs were agreed first, the review came after the merge); the placeholder text in the exported PDFs (no automated test reads it).

## Issues filed

- [#63](https://github.com/0bnoxide/cadcraft/issues/63): `figureexport` does not replace escaped MTEXT placeholders and writes no QA record.
- [#64](https://github.com/0bnoxide/cadcraft/issues/64): a visible layer frozen in the layout viewport stays hidden with no QA record.
- [#65](https://github.com/0bnoxide/cadcraft/issues/65): `packagedeliverables` refuses files downloaded on Windows because of their `Zone.Identifier` stream.
- [#66](https://github.com/0bnoxide/cadcraft/issues/66): small clean-ups and test gaps (five tests fail under plain `cargo test`, no test of exported text or PNG, a duplicated function, an undocumented action, one drawing read per job).

#63 to #65 were found by reading the code and were not reproduced in a run.
