# Slice 12 notes: review (Claude)

Review-only role. All code is on the Codex branches; this branch changes only this file.

## Status

| PR | Head reviewed | Verdict | Non-blocking notes |
|---|---|---|---|
| #90 frames and profile views | `14da7dd36` | Changes requested, one item: matchlines are drawn twice at shared stations and at the outer range ends. | #95 |
| #94 sheets, `storage: current` | `cd4834783` | Approve once the #90 matchline fix is carried in. | #98 |
| #96 sheets, `storage: drawings` | `3ef16fca7` | Approve once the #90 matchline fix is carried in. | #99 |

GitHub does not allow a request-changes or approve review from the account that opened the PR, so each review is posted as a comment review with the verdict in its first line.

The matchline fix was acknowledged by Codex at 19:40 and was not yet pushed when these notes were written. Its delta on the three new heads still needs a check before merge.

## Message log (UTC)

Times before 15:29 for messages received while this agent was paused on quota are the order of arrival, not exact.

| Time | Direction | Summary |
|---|---|---|
| 2026-11-12 10:08 | Claude → Codex | Hello, own id, asked for the five design proposals, frame tiling first. |
| 2026-11-12 10:12 | Codex → Claude | Hello; reading the briefs and reference, proposals to follow, no code before agreement. |
| 2026-11-12 10:20 | Codex → Claude | Proposal 1, frame tiling. |
| 2026-11-12 10:22 | Claude → Codex | Proposal 1 agreed with five points pinned. |
| 2026-11-12 10:30 | Codex → Claude | Proposal 1 pins accepted. Proposal 2, frame and sheet storage. |
| 2026-11-12 10:33 | Claude → Codex | Proposal 2 agreed with three changes and two pins; asked what the anchor entity is. |
| 2026-11-12 10:41 | Codex → Claude | Proposal 2 changes accepted; anchor is a visible MTEXT. Proposal 3, template contract. |
| 2026-11-12 10:45 | Claude → Codex | Proposal 3 agreed with two changes and four pins. |
| 2026-11-12 10:55 | Codex → Claude | Proposal 4, profile view range. |
| 2026-11-12 10:58 | Claude → Codex | Proposal 4 agreed with one change and four pins; PR 1 unblocked on confirmation. |
| after 10:58 | Codex → Claude | Proposal 3 changes accepted. Proposal 5, sheet drawings. |
| after 10:58 | Codex → Claude | Proposal 4 change and pins accepted; starting PR 1; proposal 5 held for explicit agreement. |
| after 10:58 | Codex → Claude | #87: there is no shared ground-profile helper for MOVE, so COPY always drops ground and keeps design. |
| after 10:58 | Codex → Claude | PR 1 progress; proposal 5 still pending. |
| after 10:58 | Codex → Claude | PR 1 up: #90 at `e39869125`. |
| after 10:58 | Codex → Claude | PR 2 progress; `add_frames` interpretation for new road rectangles. |
| after 10:58 | Codex → Claude | PR 2 up: #94 at `cdbf0b81d`; follow-ups #91, #92, #93 filed. |
| after 10:58 | Codex → Claude | Proposal 5 refinement from #93: exclusive publish or refuse. |
| after 10:58 | Codex → Claude | #94 bug found by its own plot check (neighbouring profile visible); fix by one layer per figure. New head `5843f8148`. |
| 2026-11-12 15:29 | Claude → Codex | Proposal 5 agreed with the #93 refinement and four pins; `add_frames` interpretation accepted. |
| 2026-11-12 15:35 | Codex → Claude | Proposal 5 pins confirmed; fixing four bot review threads on #90 and #94. |
| after 15:35 | Codex → Claude | #90 new head `14da7dd36`; #94 new head `cd4834783`. |
| after 15:35 | Codex → Claude | Drawings mode progress. |
| after 15:35 | Codex → Claude | PR 3 up: #96 at `3ef16fca7`. |
| 2026-11-12 19:30 | Claude → Codex | #90 verdict: changes requested, matchlines. |
| 2026-11-12 19:40 | Codex → Claude | Matchline fix planned in the shared path, to be carried up the stack. |
| 2026-11-12 19:45 | Claude → Codex | #94 verdict: approve once the matchline fix is carried in. |
| 2026-11-12 19:53 | Claude → Codex | #96 verdict: approve once the matchline fix is carried in. |

## Design agreements

All five were agreed by message before the dependent code. Proposals 1 to 4 were agreed between 10:22 and 10:58, before PR 1 and PR 2 were written. Proposal 5 was agreed at 15:29; Codex held the drawings code until then.

1. **Frame tiling** (agreed 10:22). Host-free `alignment::frames`. Range ends are kept exactly and are exempt from rounding and the buffer, never from containment. Fixed: next = previous placed matchline + spacing, floored to the increment on absolute stations, then moved back to `floor((joint - buffer) / increment)` while inside a joint buffer; `NO_PROGRESS` when it cannot advance. Positive offset is left of the chord; centre = chord midpoint + offset × left normal; twist = chord angle; size = paper size × scale. Automatic takes the longest admissible rounded candidate. Budgets refuse with `NUMERICAL_LIMIT`; a piece outside its frame refuses with `FRAME_OUTSIDE` and a witness station. Pinned by the reviewer: (a) no frame longer than the spacing and none shorter than `POSITION_TOLERANCE`; (b) the buffer covers interior joints only, both sides, strict, 0 = off, and automatic obeys it; (c) containment is inclusive within `POSITION_TOLERANCE`; (d) containment is tested against the offset frame; (e) spacing below the increment is `NO_PROGRESS`.
2. **Frame and sheet storage** (agreed 10:33, confirmed 10:41). The alignment store's bounded XRECORD mechanism, generalised only in its key and payload helpers. One model-space group anchor (a visible MTEXT on `CADCRAFT-SHEET-GROUPS`) holds the settings, the ordered frame and member identities, the source alignment and the sheet records. Every generated entity stores group, role, frame index and its own handle, so a copy is unowned. A layout is owned through a paper-space marker. Changes asked by the reviewer: (1) the frame axis is read from the stored vertex order, not the long side; (2) stations are re-derived from the stored chord half-length about the frame centre, so an unmoved frame returns its stored stations; (3) a stretched frame is refused. Pins: gaps and overlaps between hand-moved frames are reported, not refused; an erased anchor or a corrupt manifest is refused with nothing changed.
3. **Template contract** (agreed 10:45, confirmed by Codex after 10:58). Content viewports are named by layer: `SHEET-PLAN-1`, `SHEET-PROFILE-1`, `SHEET-PLAN-2`, `SHEET-PROFILE-2`; the role set must equal the arrangement's exactly. The matchline block and attribute must exist in the current drawing, with a line and MTEXT fallback. North arrow inserts sit on `SHEET-NORTH-1/2`. The layout is copied with fresh handles and its blocks and table records. Changes asked by the reviewer: (1) a conflicting layer, linetype or style keeps the current drawing's definition and is reported, and only an unequal block definition is refused; (2) a site frame is the insert's rectangle as seen (uniform scale only), and paper size is that divided by the scale. Pins: the north arrow rotation is set absolutely to the twist; an odd frame count in a two-up arrangement drops the unused slot and reports it; a missing or oversized profile view is refused; the template goes through the capped reader.
4. **Profile view range** (agreed 10:58, confirmed by Codex after 10:58). `view::draw` gains a station range and elevation window; the old call is the whole-range wrapper. `profileviewframes` takes `at`, a fixed height, the profile (ground or design), the centre rule (highest, median, lowest), a datum increment and optional start and end buffers. The buffered range is clamped to the alignment and the clamp reported. Datum = `floor((centre - height / 2) / increment) × increment`. The first vertical grid line is `ceil(start / step) × step`. Change asked by the reviewer: #87 uses one rule in one place with no zero-displacement case; since MOVE has no shared helper, COPY always drops ground and keeps design. Pins: the median is the median of the clipped vertex elevations and the test lists them; a profile leaving the fixed window is clipped and reported per view; views stack downward in frame order and a re-run replaces only owned views; model x is 1:1 with station.
5. **Sheet drawings** (agreed 15:29, confirmed 15:35). `duplicate_objects: true` is required before any work. Each drawing is the current model with its definitions and owned profile figures, keeping only its own layouts. The whole set is staged in a private folder inside the output folder, reopened and verified, then published; the source manifest is assigned last. Updates write `_vN` names and never edit earlier files. From #93: publication is exclusive (no-replace rename or hard link) or it is refused; the fallback that can race is never used for drawings. Pins: (a) an existing unowned target name, versioned or not, is refused in preflight with no skip to the next version; (b) duplicate names within a run are refused in preflight; (c) undo restores the manifest only, files stay, and a re-run is then a collision; (d) without the flag nothing is staged or created.

## Reviews

### #90 at `14da7dd36` (first read at `e39869125`)

Verdict: changes requested, one item. Ran `cargo test -p cadcraft-alignment` (56 passed), `-p cadcraft-engine --test sheet_frames` (6 passed) and the engine `sheet` unit tests (4 passed).

Hand calculations, made without the code:

- Straight 1,000, spacing 250, increment 10: ranges 0/250/500/750/1000, centres (125 + 250i, 0), angle 0; with offset 5 the centres are (125 + 250i, 5).
- Line, arc, line (R = 100, left quarter turn, 200-wide frame, automatic). Arc point at station s ≥ 100: (100 + 100 sin φ, 100 − 100 cos φ), φ = (s − 100) / 100. At 210: (189.121, 54.640), chord 196.86. At 220: (193.204, 63.764), chord 203.45. First matchline 210. Second chord to (200, 200): (10.879, 145.360), length 145.77, so the second frame reaches the end. Tangent directions stay within 90° of each chord, so each piece is inside its frame.
- Buffer: joint 255, buffer 20, candidate 250 → floor(235 / 10) × 10 = 230; next 480.
- Profile window: ground 100 at 0, 120 at 400, 90 at 1000; range 250 to 500 gives 112.5, 120, 115; median 115; datum floor(95 / 5) × 5 = 95; window 95 to 135. Start 1234.5, step 100: first grid line 1300.

Finding: `redraw_matchlines` draws one matchline at both ends of every frame, so the straight case gets eight where the brief expects three.

### #94 at `cd4834783`

Verdict: approve once the #90 fix is carried in. Ran `--test sheets` (21 passed), `--test sheet_frames` (6) and the `sheet` unit tests (4).

Hand check: a plan viewport of paper height h at scale s shows a model height of h × s centred on the frame with twist −(frame angle); a north arrow drawn pointing up is set to −(frame angle); a profile viewport uses the same h × s, twist 0, centred on the figure, and a figure that does not fit is refused.

### #96 at `3ef16fca7`

Verdict: approve once the #90 fix is carried in. Ran `--test sheet_drawings` (7 passed), `--test sheets` (21), `--test sheet_frames` (6) and the `sheet` unit tests (7). All four proposal 5 pins and the #93 refinement are implemented as agreed.

### Not run

The full `cargo xtask ci`, the CLI and control-channel flows, plots, and a filesystem without hard links. The author reports the local gates and CLI flows; hosted Linux and Windows gates were green on #90 and #94 at the reviewed heads and queued on #96.

## Issues filed

- #95: non-blocking notes from #90 (automatic search cost, candidate stepping, two edge refusals, unbounded stored angle, site blocks with a second polyline, error text, stale matchlines after `profileviewframes`, mirrored road frames).
- #98: non-blocking notes from #94 (an erased owned sheet entity or layout leaves the group stuck; absolute template path stored in the drawing; slot read from any layer ending `-2`).
- #99: non-blocking notes from #96 (a hand-edited or deleted published drawing stops later updates; absolute output folder stored; `layouts_per_drawing` range; hard-link message).

## Reference reading

The REVENG files the brief names record the stages, settings vocabulary, ownership model and two observed refusals (no alignment; the two duplication confirmations). They record frame geometry, rounding, collision and rollback as unknown (`U-SG-001` to `U-SG-003`), so the brief's rules decide those. The three PRs match the stage order, the settings and the ownership model, turn the two confirmations into one required flag, and use none of the reference's message text.
