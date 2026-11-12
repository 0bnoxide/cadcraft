# Slice 12 implementation record

Spec: [plan and profile sheets](slice-12-plan-profile-sheets.md).

## Scope and plan

All implementation is written here, in the three prescribed stacked branches.
No merge authority. Username-only contributor credit; no contributor-file change.
Private build output: `target/slice12-codex-frames` in the source checkout.
No dependencies, dialogs, DST, sheet index, pipe parts, plotting or remembered settings.

- [x] Read the required briefs and the five named classes of read-only REVENG evidence.
- [x] Check main, overlapping PRs and #85/#87; claim #87.
- [x] Establish a clean alignment baseline: 48 tests pass.
- [x] PR 1 code/validation: host-free `frames` geometry and independently calculated tests; ranged/windowed `view` rendering and datum calculation; drawing commands and bounded ownership storage; #87 regression; command registration and labels. Review pending.
- [x] PR 2 code/local validation: explicit-role template import, current-drawing sheet create/update, ownership and collision/refusal tests. Publication/hosted checks/review follow.
- [ ] PR 3: agreed drawings storage, staged exclusive publish, reopened output verification and rollback tests.
- [ ] Before each commit/push: inspect diff and run the complete `cargo xtask ci` gate.
- [ ] Request exact-head review through Paseo, resolve findings, record posted verdict and notes PR. Refresh main before stacked review; retarget after predecessor merge. Never merge.

Each implementation task starts with a failing behavioral test, then the minimum implementation and a passing check. Changes reach `crates/alignment`, `alignment_store.rs`, `profile_cmd.rs`, new sheet command modules, `cmd.rs`, and the new label rows. Placeholder and publish helpers are shared only when sheet commands need them. Existing drawings and unrelated files are preserved; commands mutate a staged drawing once, so ordinary errors cannot leave partial edits.

## Agreed design

1. Fixed spacing advances from the actual preceding matchline; range ends stay exact. Interior stations round down, then retreat outside strict interior-joint buffers. Automatic spacing searches candidates downward to find the longest admissible rounded station; bounded work fails explicitly. Frames use chord midpoint, ordered axis and left offset. Containment includes POSITION_TOLERANCE and is certified, never silently clipped.
2. Generalize bounded alignment XRECORD helpers by key. A visible MTEXT group anchor and reciprocal child ownership records prevent adopting copies. Read rectangular vertices in original order; retain chord half-length for station recovery, preserve unchanged stations exactly, refuse stretching. Gaps/overlaps after moves are reported. Layouts use a dedicated owned paper marker. DXF and DWG reopen twice must preserve metadata.
3. Explicit viewport-role layers `SHEET-PLAN-1`, `SHEET-PROFILE-1`, `SHEET-PLAN-2`, `SHEET-PROFILE-2`. Required role set is exact; profile fit is checked. Matchline block and attribute are explicit. Site rectangle follows its positive uniform insert transform once; paper dimensions are model dimensions divided by sheet scale. Table conflicts use current definitions and warn; unequal block definitions refuse. North arrows use absolute viewport twist. Odd two-up layouts erase unused slots and arrows.
4. Buffered profile ranges clamp to horizontal extent and report it. Fixed-height datum is floor((selected statistic - height/2)/increment)*increment. Median is the vertex median including interpolated ends and analytic design extrema, not a length-weighted median. Selected profile must cover the datum range. Views stack down, replace owned predecessors and report clipped profile spans. Vertical grid lines lie on absolute round stations. COPY drops ground and retains design without a new zero-displacement exception.
5. Drawings proposal sent; agreement pending. No dependent implementation until agreement.

## Paseo message log

Times below are UTC recording times for the initial exchange (before implementation); message ids identify the exact exchanges. All outgoing messages use `[slice12]` and the sender id after initial id discovery.

| Recorded UTC | Direction | Message | Summary |
|---|---|---|---|
| 2026-11-12T10:15Z | received | 331cb47a | Reviewer id and five-proposal request. |
| 2026-11-12T10:15Z | sent | 959118c5 | Hello; required reading first, no code before agreement. |
| 2026-11-12T10:15Z | sent | 5bf938a0 | Proposal 1: tiling. |
| 2026-11-12T10:15Z | received | 4fbca3d2 | Proposal 1 agreed with five pins. |
| 2026-11-12T10:15Z | sent | f3d36386 | Accepted pins; proposal 2 storage. |
| 2026-11-12T10:15Z | received | e73c2301 | Proposal 2 agreed with ordered axis, chord recovery, no stretching. |
| 2026-11-12T10:15Z | sent | bac9deed | Accepted changes; visible anchor; proposal 3 template contract. |
| 2026-11-12T10:15Z | sent | 8bcbcb32 | Proposal 4 profiles; scanner authentication retries. |
| 2026-11-12T10:15Z | received | 84bd48fc | Proposal 3 agreed with table and site-transform changes. |
| 2026-11-12T10:15Z | sent | 802023fd | Accepted changes; proposal 5 drawings. |
| 2026-11-12T10:15Z | received | 83e431ce | Proposal 4 agreed with no separate zero-copy rule and four pins. |
| 2026-11-12T10:18Z | sent | 2f80cc05 | Accepted changes; PR1 begins, proposal 5 pending quota reset. |
| 2026-11-12T10:18Z | received | a265411c / 9d5a2168 | Usage quota; reset 07:10 Denver. No proposal 5 agreement or code review implied. |

## Evidence

- Source main: `7f242d0ac5295c82628ad7f28b2fdcbd6421577f`, equal to fetched origin/main.
- Required deterministic secrets scans pass with authenticated access. Sandbox-only invocations began returning `Not authenticated`; retry with existing authentication succeeds.
- `cargo test -p cadcraft-alignment`: 48 passed before changes.
- #87 claim: https://github.com/0bnoxide/cadcraft/issues/87#issuecomment-6820856176

### PR 1 progress, 2026-11-12T10:45Z

- Geometry/window tests first failed for the missing APIs, then all 55 alignment tests passed.
- Engine regressions first failed on missing commands and on #87 (copied ground still present).
- Six new integration tests now pass: road/profile stages and single-step undo; twice-reopened DXF/DWG ownership; COPY; copied anchor and invalid input; placed site blocks and mirrored refusal; named matchline attributes and missing-name refusal; moved chord stations and stretched-frame refusal.
- A new rerun assertion caught orphaned ownership records (540 objects after rerun versus 292 before). Cleanup now removes only the generated ownership record/dictionary, retaining other applications' entries; the assertion passes.
- A corrupt-manifest test initially accepted a frame whose stored width differed from paper width times scale. It now refuses and the test passes.
- Full `cargo xtask ci` is running. Workspace fmt and clippy have passed; test binaries compiled. No push, commit, PR or posted review yet.
- Latest test-only addition independently checks the second line/arc/line frame too; run the alignment tests again before committing.
- Sent Paseo `cffe66e0` (recorded 10:45Z): 55 alignment tests, engine regressions building, confirmed COPY always drops ground in `translated_if_present` because MOVE/ROTATE instead make the authoritative carrier stale. Received quota notification `d199a0f6`; proposal 5 remains unagreed.

Next: finish CI, run a real CLI road/frame/profile/save/reopen check, inspect all new modules, record exact passing head, then commit/push PR 1 and request its review through Paseo. Continue PR 2 from that commit under designs 1–4. Do not implement drawings storage pending proposal 5 agreement.

### PR 1 validation, 2026-11-12T10:48Z

- `cargo xtask ci` with the normal Windows user access: all seven gates passed. Report: **4251 passed, 28 skipped**. The preceding restricted run stopped at four MCP token-file `PermissionDenied` failures; the unrestricted rerun passed all four without source changes.
- A real `cadcraft-cli run` sequence created a 1000-unit alignment, design profile, four offset road frames, saved DXF, reopened it, created four owned profile views, saved DWG, reopened that DWG and replaced the four views, then saved DXF. A missing-alignment request failed and left its input's SHA-256 unchanged. The generated files are private build artifacts, never committed.
- The line/arc/line known-answer test now checks both frames' exact ranges, independently calculated centres and chord angles.
- Diff scope checked; no dependency, CI, parity-document or contributor-file changes.
- Paseo message `37796927` sent a progress/reminder and kept proposal 5 explicitly pending; quota response `0bd6c7a5` gave no agreement or review verdict.

### PR 1 publication and PR 2 progress, recorded 2026-11-12T11:20Z

- PR 1: https://github.com/0bnoxide/cadcraft/pull/90, exact head `e39869125e40acaefbad31416cd46ddbe097672a`. Hosted Linux and Windows gates both passed at that head. No posted review verdict; draft stays pending review.
- #85 item 5 comment: https://github.com/0bnoxide/cadcraft/issues/85#issuecomment-6821545441 (other items stay open).
- `slice12/codex-sheets` is stacked on that commit, in its own worktree with private `target/slice12-codex-sheets`. Fetched main and merged `origin/main` before review preparation: already up to date at `7f242d0ac`.
- `sheetcreate` and `sheetupdate` implement current-drawing sheets under agreements 1–4. All arrangements, site sheets, explicit roles, names/numbers, placeholders, table/block conflicts, owned output replacement, added/deleted frames, gaps/overlaps, north-arrow rotation, and DXF/DWG reopen are exercised.
- `add_frames` is explicit adoption of unowned objects. A new road rectangle has no prior chord, so its full ordered first edge initializes the chord. Existing frames always use the half-chord already stored. New identities are monotonic; copies remain unowned until explicitly added.
- First sheet tests failed on missing `sheetcreate`; five then passed. Added tests caught a removed active paper layout retaining stale UI space, dropped profile clipping reports, cyclic template blocks and scale overflow. Each now has its direct refusal/update path. Fifteen targeted sheet tests and engine Clippy passed before the final block-metadata collision case was added.
- Full CI is in progress. Final block-collision check must include metadata (units etc.), not only base/annotative/geometry, before PR 2 is committed.
- Unsupported reference import is deliberately refused, tracked in https://github.com/0bnoxide/cadcraft/issues/91. No source data is silently dropped; template input stays unchanged.

| Recorded UTC | Direction | Message | Summary |
|---|---|---|---|
| 2026-11-12T10:49Z | sent | d50f2cb2 | PR #90 exact-head full review requested; all local gates; PR 2 begins; proposal 5 pending. |
| 2026-11-12T10:49Z | received | 7ee1dbe0 | Usage quota, reset 07:10 Denver; no review performed. |
| 2026-11-12T11:09Z | sent | dd2d7561 | Current-sheet progress and fixes; new-frame initialization clarified; proposal 5/review still pending. |
| 2026-11-12T11:09Z | received | c768b02b | Usage quota; no design/review verdict. |

### Final PR 2 checks, recorded 2026-11-12T11:35Z

- Eighteen sheet integration tests pass, including the two final regressions: plan-only updates refresh matchlines even without profile settings; a missing owned profile component refuses a profile sheet. Initial creation refreshes the existing owned profile figures from their settings after validating those figures' ownership/completeness.
- The full gate caught the block-metadata collision regression before publication. The fix compares every block metadata field, ignoring only the source entity handles and case of the block name. A separate failing test confirmed no-op updates dropped overlap reports; they now report the same overlaps without mutating the drawing.
- Template paths are stored as canonical paths so reopening from another working directory uses the same template.
- Real CLI: built an original DXF template using layout, viewport, block/insert and text commands; road frames → profile figures → four plan/profile sheets → DWG → reopened update from another working directory → moved-frame update. All operations succeeded. Repeat this on the final rebuilt binary after the final gate.
- The previous full gate passed all seven steps at 4267 passed/28 skipped before the last two tests/fixes. The final full gate is running on the 18-test implementation; do not cite the earlier run as final-head proof.
- Follow-ups: #91 tracks unsupported reference/template data; #92 tracks quadratic metadata persistence discovered by tracing the shared XRECORD writer. #92 has not been benchmarked. The storage helper documents the known limit; no unrelated performance redesign was added.

### PR 2 acceptance, recorded 2026-11-12T11:40Z

- Final `cargo xtask ci`: **4269 passed, 28 skipped; all seven steps passed**, including wasm and layers. This run includes the final 18 sheet tests and both last path fixes.
- Repeated the CLI creation/reopen check on the final rebuilt binary: four current-drawing sheets and four owned profile views, DWG save, and update after reopening from another working directory. Template itself was authored by the CLI using original primitives and TEXT placeholders.
- #93 tracks the shared publish helper's unsupported-filesystem fallback race, found by source review, not reproduced on NTFS. It is distinct from closed #32's empty-directory case. It must be addressed/refused before external sheet drawings claim unconditional no-replace publication; no external drawing publication code exists in this branch.
- No PR has been merged. Review verdicts and proposal 5 agreement remain pending the reviewer quota reset (07:10 America/Denver).
