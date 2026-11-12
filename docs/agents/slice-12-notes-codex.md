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
- [ ] PR 1: host-free `frames` geometry and independently calculated tests; ranged/windowed `view` rendering and datum calculation; drawing commands and bounded ownership storage; #87 regression; command registration and labels.
- [ ] PR 2: explicit-role template import, current-drawing sheet create/update, ownership and collision/refusal tests.
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
