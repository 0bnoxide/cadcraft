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

Pending the PR.
