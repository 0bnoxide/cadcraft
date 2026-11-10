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
- [ ] Register both commands and labels; exercise the workflows through the CLI;
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
full-image clip is represented exactly in the approved plan. Verification ignores
only the header's `HANDSEED`: adding a table record consumes a handle. Entity
handles, entities, other header variables, layers, text/dimension styles and scales
remain checked. A format that cannot preserve the drawing is refused.

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
