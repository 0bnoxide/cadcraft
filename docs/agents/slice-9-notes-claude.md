# Slice 9 notes: review

Role: review only. All code is on `slice9/codex-follow-ons`. This branch changes only this file.

## Message log (UTC)

| Time (sent: clock; received: approximate) | Direction | Summary |
|---|---|---|
| 2026-11-10T00:05Z | sent | Hello: reviewer id, request for the three design proposals. |
| 2026-11-10T00:06Z | received | Acknowledged; proposals delayed while the author sorts out a secrets-scanner sign-in on its side. |
| 2026-11-10T00:07Z | received | Scanner sign-in resolved; the author is reading the brief, proposals next, no code yet. |
| 2026-11-10T00:08Z | received | The three design proposals (staging/swap, manifest v2 and intake, Configure Drawing). |
| 2026-11-10T00:10Z | sent | Verdicts: all three agreed with the changes listed below; one open question on `manifest_path`. |
| 2026-11-10T00:10Z | received | All amendments accepted, no PR split. `manifest_path` only locates ProjectConfiguration; a differing manifest is refused as `manifest_mismatch`. Closed style field lists. Proposed a fixed 1x1 image size. |
| 2026-11-10T00:10Z | sent | Agreed `manifest_mismatch`, the style field lists and the #34/#36 approach. Changed images: reuse the existing `imageattach` sizing; 1x1 only as a recorded fallback. |
| 2026-11-10T00:13Z | received | Image change accepted: intake calls the existing `imageattach` code during planning; unit square only on decode failure, with the reason in the row. Regression tests fail on the baseline as expected. |
| 2026-11-10T00:30Z | received | Status: implementation in place, testing before the PR. Two points to check at review: the image clip is canonicalised by a host-format round trip before plan hashing, and settings verification ignores `HANDSEED`. |
| 2026-11-10T00:37Z | received | Status: fmt and clippy pass; `cargo xtask ci` on Windows fails rebuilding its own running `xtask.exe` (filed as #41), so the author runs the seven gates through a separate driver target directory. No CI or config files changed. |
| 2026-11-10T00:44Z | received | Status: seven gates passed through the separate-driver command (4070 passed, 28 skipped); CLI acceptance passed; raw CSV hashing added for #34 staleness. Repeating CI on the final changes, PR next. |
| 2026-11-10T00:53Z (earlier) | received | PR up: #43 at head `243dbc4c`, review requested. |
| 2026-11-10T00:53Z (earlier) | received | The author found a host-name guard using ASCII-only comparison; fix to come with the review fixes. |
| 2026-11-10T00:53Z (earlier) | received | The author's plan for the five findings, with two scope questions. |
| 2026-11-10T00:53Z | sent | Review done: changes requested, five findings on the PR; scope questions answered; #44 and #45 filed. |
| 2026-11-10T01:00Z | received | Full-drawing comparison differs only by `EXTMIN`/`EXTMAX`, which the writer adds with the new image; the author proposes deriving the expected extents rather than exempting them. Also fixing dimension text-style name spelling. |
| 2026-11-10T01:00Z | sent | Agreed both; exemption of exactly those two values only as a documented fallback. |
| 2026-11-10T01:03Z | received | Status: all five fixes in place locally with the two extra fixes; extents are explicit plan actions; full CI and CLI check running before the push. |
| 2026-11-10T01:06Z | received | Fixes pushed to #43 at head `762deafb`; hosted gates running. |
| 2026-11-10T01:10Z | received | Hosted gates still running; the author asks for the link to the register-only note so every unfixed finding is linked from the PR. |
| 2026-11-10T02:11Z | sent | Verdict: #43 approved at `762deafb`, no open findings, hosted gates pass; the two extra notes are items 4 and 5 on #45. (The author's agent session had been closed by then; the message was queued.) |
| 2026-11-10T02:15Z | received | Final confirmation from the author: #43 open at the approved head `762deafb`, hosted gates pass, no open threads, #44 and #45 linked from the PR, local gates and CLI acceptance pass. No merge; the owner merges #43, then the notes PR. |

## Design agreements

Agreed by message at 2026-11-10T00:10Z, before any dependent code (the author reported no code written). Changes requested on top of the author's proposals:

1. **Staging and swap.** Agreed: sibling stage file, reopen and verify before the swap, prior file kept in `_superseded/`, target hash rechecked just before the swap, restore on a failed second move, one rename helper shared with the project publish. Changes:
   - `std::fs::rename` replaces an existing file on Windows (it refuses only directories), so file moves outside the `rustix` path use `hard_link` then `remove_file`, with check-then-rename as the documented last fallback. A test proves a move onto an existing file fails.
   - The stage file is created with `create_new`, link checks cover `_superseded` and the stage path, and every failure path removes the stage file.
   - The injected move closures stay crate-private.
2. **Manifest version 2 and intake.** Agreed: `source_roles`, `source_location`, `drawing_settings`, the `manifest_v2_required` code, the register row format and the format-to-action table. Changes:
   - No `source_attachment` manifest field. Fixed origin, scale 1, rotation 0, recorded in the plan and register.
   - A failure leaves the register and `BaseModel` at their prior bytes and removes only this run's copies.
   - Duplicate or already-used reference names are refused at plan time.
   - Sources are hashed by streaming.
   - `manifest_path` only locates the ProjectConfiguration record. The embedded manifest is authoritative and a differing supplied one is refused as `manifest_mismatch` (settled 2026-11-10T00:10Z).
   - Images reuse the existing `imageattach` sizing (true aspect ratio, no new decoder). A fixed 1x1 size is only a fallback, recorded in the register row (accepted 2026-11-10T00:13Z).
3. **Configure Drawing.** Agreed: the six header values with ranges, layer patches, annotation-scale upserts, the action and issue codes, empty re-diff on reopen. Changes:
   - Style patches take a closed field list. Text: `name`, `font`, `height`, `width_factor`, `oblique`, `annotative`. Dimension: `name`, `text_style`, `scale`, `arrow_size`, `text_height`, `decimals`, `angular_decimals`, `annotative`.
   - An empty plan writes nothing at all.
   - The staged copy keeps the target's format, or the plan refuses.

## Review

### Round 1: PR #43 at `243dbc4c`, changes requested

Ran on Windows: `cadcraft-proposal` and `cadcraft-droneqa` suites (pass), 14 engine workflow tests (pass), clippy on the three crates (clean). Not run: full `cargo xtask ci`, wasm, the CLI acceptance run, Linux or macOS.

Findings, posted on the PR:

1. Intake refuses sources under any linked parent folder; only the source itself should be checked.
2. `configuredrawing` accepts intake copies and `_superseded/` files as targets.
3. The lossless and "nothing else changed" comparisons cover only part of the drawing.
4. No test for the agreed `imageattach` sizing path.
5. Attachments are not checked for resolving to the copied file.

Non-blocking notes filed as issues: [#44](https://github.com/0bnoxide/cadcraft/issues/44) (stale lock file) and [#45](https://github.com/0bnoxide/cadcraft/issues/45) (clean-ups).

### Round 2: PR #43 at `762deafb`, approved

Read the incremental diff from `243dbc4c` in full. All five findings are fixed, each with a test:

1. Only the source entry is checked for links; a source under a redirected parent is accepted.
2. Targets under the source location or any `_superseded` folder are refused as `protected_target`.
3. `same_drawing` compares the whole `Drawing`, normalising only `HANDSEED`, `handseed` and `Foreign.targets`. `EXTMIN`/`EXTMAX` are derived and shown as `set_extent` plan actions. A lossy writer is refused as `lossy_format`.
4. A 2x1 PNG gives `imageattach_default` at 2:1; undecodable bytes give the recorded unit-square fallback.
5. After publication each copy's hash is rechecked and each relative reference must resolve to the copied file.

Also in the push: the host-name guard uses `paths::case_fold`, and a dimension style's text-style reference takes the existing style's spelling.

Ran on Windows at `762deafb`: `cadcraft-proposal` and `cadcraft-droneqa` suites (pass), 17 engine workflow and `pointconvert` tests (pass), clippy on the three crates (clean). Hosted gates on ubuntu-24.04 and windows-latest pass on the same head. Not run locally: full `cargo xtask ci`, wasm, the CLI acceptance run, Linux or macOS.

Non-blocking, added to [#45](https://github.com/0bnoxide/cadcraft/issues/45#issuecomment-6755983604): a register-only intake still rewrites and swaps `BaseModel`, and the derived extents set z to 0.
