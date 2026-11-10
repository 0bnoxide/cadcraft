# Slice 9 implementation notes

Branch: `slice9/codex-follow-ons`, based on `main` at `3853d064`.
Private build directory: `target` inside this worktree; the CI driver uses `target/ci-driver`.

## Agreed design and execution plan

The brief is `slice-9-proposal-follow-ons.md`. The read-only behavioral reference is
AutoGIS-Civil3D's follow-on candidate design, candidates 1 and 3. No reverse-engineering
repository is read. All implementation stays on this branch; no merges.

- [x] First pin manifest v2 validation and v1 compatibility with failing tests. Add the
  five source roles, default `Support/Sources`, and a closed drawing-settings schema.
- [x] Pin no-replace file moves, failed project publication, failed swaps and restoration.
  Share the move/swap code beside the existing project publisher. Move test fixtures
  to `tests/common/mod.rs`. Retain prior drawings in `_superseded` with UTC timestamps.
- [x] Implement intake planning, exact approval, staged copy/register/host publication,
  reopen verification, and rollback. Hash source files by streaming. Publish copies,
  then register, then host; roll back register and new copies if the host cannot swap.
- [x] Implement settings diffs and named-field patches, same-format verification,
  no-op behavior, and entity preservation. No coordinate-system changes.
- [x] Add bounded per-point checkpoint evidence and short-row diagnostics with
  regressions, legacy-history compatibility and privacy checks (#34, #36).
- [x] Register both commands and labels; exercise the workflows through the CLI;
  run `cargo xtask ci`, inspect the diff, open the PR and request independent review.
  Address findings and rerun checks for the final head.

Stages are exclusive sibling files. Approval covers the target, manifest, project
configuration, register and source hashes. Immediately before mutation the target
is checked again. A failed second move restores the prior file; a failed restoration
reports its retained location and never claims success. File moves use rustix
NOREPLACE where supported, otherwise exclusive hard-link/unlink, with a documented
check/rename fallback only for unsupported filesystems. Links/reparse paths inside
the project, stages and retention paths are refused.

Manifest input is used to locate the configurable ProjectConfiguration record;
the embedded manifest is authoritative and disagreement is `manifest_mismatch`.
Source names remain unchanged; collisions with the host, other sources or existing
reference names are refused. Formats: DXF/DWG overlay; PNG/JPEG/TIFF image; all others
copy/register only. Image sizing reuses IMAGEATTACH defaults, with any unsupported
format fallback recorded in the row. No transform option is added.

Settings headers: INSUNITS, MEASUREMENT, LUNITS, LUPREC, AUNITS, AUPREC.
Layer patches: name, color, linetype, lineweight. Text patches: name, font, height,
width_factor, oblique, annotative. Dimension patches: name, text_style, scale,
arrow_size, text_height, decimals, angular_decimals, annotative. Scale patches:
name, paper_units, drawing_units. Omitted properties and unnamed records stay as-is.
Missing dependencies are refused; no automatic linetype/style creation.

## Paseo message log (UTC)

Own id: `d7f23169-a6ff-49a8-bed1-2f2f5d0629f0`.
Reviewer id: `e38f9e72-412b-4ed1-8275-4b7f9ac21812`.

Initial times below are approximate; all precede implementation.

| Time | Direction | Summary |
|---|---|---|
| 2026-11-09 23:55 | received | Reviewer supplied id and requested three proposals. |
| 2026-11-09 23:56 | sent | Acknowledged; required scanner authentication blocked reads. |
| 2026-11-09 23:59 | received | Reviewer setup complete, waiting on proposals. |
| 2026-11-10 00:00 | sent | Scanner resolved; brief scans passed, reading without code changes. |
| 2026-11-10 00:01 | received | Reviewer logged scanner resolution and awaited proposals. |
| 2026-11-10 00:07 | sent | Three proposals: staging/swap, manifest/intake, settings diffs. |
| 2026-11-10 00:10 | received | All agreed with exclusive Windows moves, transactional register, streaming hashes, closed styles, no-op and lossless-write requirements. |
| 2026-11-10 00:11 | sent | Accepted changes, clarified manifest locator and closed style fields; proposed image sizing; one PR. |
| 2026-11-10 00:13 | received | Manifest locator, style fields and follow-ups agreed; reuse IMAGEATTACH sizing. |
| 2026-11-10 00:17 | sent | Accepted image amendment; existing callable IMAGEATTACH path reused, with recorded decode fallback. |
| 2026-11-10 00:32 | sent | Reported Windows regression results, canonical image clip, narrow HANDSEED exclusion, private target relocation. |
| 2026-11-10 00:36 | sent | Reported runner lock, linked issue #41, described separate-driver workaround. |
| 2026-11-10 00:43 | sent | Reported seven gates and valid-PNG CLI acceptance; raw CSV fingerprint strengthened and final repeat running. |
| 2026-11-10 00:47 | sent | PR #43 up at 243dbc4c; requested full independent review of that exact head. |
| 2026-11-10 00:49 | sent | Reproduced Unicode host-name collision; one-line case_fold fix and regression prepared. |
| 2026-11-10 00:52 | received | Changes requested: source-parent redirects, protected targets, complete preservation, valid image test, published resolution. Issues #44/#45 filed separately. |
| 2026-11-10 00:54 | sent | Accepted five fixes; proposed protected source/retention paths and complete Drawing comparison. |
| 2026-11-10 00:55 | received | Fix plan agreed; normalization limited to allocation state and Foreign.targets; report any additional mismatch. |
| 2026-11-10 00:59 | sent | Full comparison found only newly derived EXTMIN/EXTMAX; proposed existing extents helper and explicit plan actions; included canonical style reference fix. |
| 2026-11-10 01:00 | received | Extents helper and explicit checked actions agreed, without a wider exemption. |
| 2026-11-10 01:02 | sent | All 13 focused workflow tests pass; full CI and expanded CLI check running before the review-fix push. |

## Validation

Baseline: `cargo test -p cadcraft-proposal -p cadcraft-droneqa` passed on Windows.
The mandatory local secrets scanner passed each file before reading. Its cache
needed sandbox escalation; no repository content is uploaded for secrets scanning.

The real CLI acceptance run created a v2 proposal and intook a DXF overlay, an
original synthetic 2x1 PNG (IMAGEATTACH sizing, one drawing unit wide and half a
unit high), and a CSV. Copy SHA-256 values matched. Configure Drawing changed only
the requested units and missing layer, then a second run returned `NoChanges`
without modifying drawing bytes. Two prior drawing versions were retained.

The image is round-tripped through the host format during planning so its implicit
full-image clip is represented exactly in the approved plan. Verification compares
the complete `Drawing`, including saved metadata and foreign DXF/DWG payload.
Only `HANDSEED`, the in-memory `handseed` and `Foreign.targets` lookup bookkeeping
are normalized. Intake's expected EXTMIN/EXTMAX come from `Drawing::extents`, appear
as explicit plan actions, and remain checked. A format that cannot preserve the
drawing is refused before approval.

Checkpoint evidence is capped at 1000 points, 256 bytes per id and 250000 serialized
bytes; its CSV input is capped at 1 MB. The raw CSV fingerprint and per-point
summary bind to the source hash; redacted evidence binds to the immutable draft
hash. Optional absent evidence preserves legacy serialized draft hashes.
Short-row diagnostics are checked before each push (1024 rows and bounded path
storage). Source hashing uses bounded streaming reads, never a whole-file buffer.

Unmodified `cargo xtask ci` hits a Windows running-executable lock when nextest
rebuilds the driver: tracked separately as [#41](https://github.com/0bnoxide/cadcraft/issues/41).
The unchanged seven gates passed via
`cargo run --package xtask --target-dir target/ci-driver -- ci`, with the private
`CARGO_TARGET_DIR` still pointing at `target` for its child Cargo commands:
4070 tests passed, 28 skipped; fmt, clippy, i18n, assets, layers and wasm passed.
The wasm check reports three existing dead-code warnings in canvas rebuild code.
No CI or configuration files were changed. A final repeat covers the strengthened
raw-CSV fingerprint and bounded source-hash read.

## Review fixes

The first full review requested five changes, all addressed in the follow-up:

1. Check the source entry itself for links, then canonicalize; parent redirects are
   accepted. A Unix symlink-parent regression exercises this separately from the
   refused symlink-file case.
2. Refuse configuration anywhere under the source location or an `_superseded`
   component, with `protected_target`. Working drawings elsewhere remain eligible.
3. Compare the full drawing after only the normalization above; preflight both the
   unchanged and desired drawings. A corrupt writer dropping a block description
   is refused before approval and leaves the original bytes intact.
4. Exercise a real non-square PNG and the decoder-failure fallback in automated
   tests. The canonical plan keeps pixel size, basis, insertion and relative path.
5. After publication resolve attachment paths to the planned copies and verify
   hashes again. A missing-copy injection proves host/register rollback.

The Unicode host guard uses the same portable `case_fold` as other collision
checks. Dimension text-style references use the existing style's canonical name,
including before diffing, so a lowercase reference does not cause perpetual edits.

The original PR head's hosted Linux and Windows gates both passed. Final hosted
checks and independent re-review will use the new exact head after these fixes.
Non-blocking follow-ups are [#44](https://github.com/0bnoxide/cadcraft/issues/44)
(stale lock recovery) and [#45](https://github.com/0bnoxide/cadcraft/issues/45)
(cleanup and durable failed receipts); no unrelated fixes are included here.

Review-fix validation: all seven gates pass with **4073 tests passed, 28 skipped**.
The expanded real CLI run covers units, a layer, named text and dimension styles
(including a lowercase reference resolved to `Notes`), and an annotation scale;
all five planned setting changes survive reopen, then the next run is a byte-
preserving no-op. It also checks resolved source hashes, a valid non-square image,
explicit extent actions, and refusal of source/retained configuration targets.
Final independent re-review and hosted checks are requested for the pushed head.
