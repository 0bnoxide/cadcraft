# Slice 2 — Codex contours and comparison

Agent: `b7f698a7-a42e-4d95-a4af-6a21772f3d8c` on Paseo host `srv_VmtxFq0sR2n7`.
Branch: `slice2/codex-contours`, based on main `e2f08f0`.
Worktree: `.claude/worktrees/slice2-codex-contours`.
Every Cargo invocation sets `CARGO_TARGET_DIR` to this worktree's `target` directory.

## Scope and implementation plan

Only `surface/src/{contour,compare}.rs`, `surface/tests/{contour,compare}*.rs`,
`engine/src/cmd/surface_contour.rs` and this note are owned by Codex.
No shared-file changes, reference-repository writes or merges.

1. Test and implement marching triangles with topology-keyed chaining, open/closed
   paths, index levels, flat/vertex/level-edge handling and bounded workloads.
2. Once the outside rule is agreed, test and implement barycentric elevations and
   target-minus-reference statistics, preserving sample order and outside counts.
3. After the conversion contract is agreed, integrate and test `tincontour` and
   reporting-only `tincompare`, then run `cargo xtask ci`, commit and open a PR.
4. Request Claude's review through Paseo; test/review Claude's PR; answer findings
   and exchange review/fix messages. Greg alone merges.

The standalone surface crate has no dependencies on other workspace crates.
No new dependencies or assets are needed. Local `plan/` is absent.
AutoGIS references are under `autogis/core/`; observed behavior only is used.

## Evidence

- Initial `cargo test -p cadcraft-surface`: passes (empty scaffold, 0 tests).
- Sonar secrets scans passed before reading workspace and reference files.
- Existing unused egui patch warnings appear during Cargo dependency resolution.

## Paseo message log (UTC)

- 2026-11-04 before 08:54:02 received — Claude `2eefe075-7879-44ee-8b16-0b7e8faf6d39`
  proposed conversion API, outside exclusion and contour layer names.
- 2026-11-04 before 08:54:02 sent — Codex agreed handles, outside exclusion and layer names;
  requested fallible strict conversion and clarification of XY tolerance.
  Message `2e005518-14d3-430e-97ed-554c62f77d83`.
- 2026-11-04 before 08:54:02 received — Claude accepted strict fallible conversion
  and XY-distance tolerance; final agreement `5048efaf-ac8a-4107-954b-1f05484dba07`.
- 2026-11-04 08:54:02 sent — Codex confirmed final agreement, asked how an
  independent main-based PR can compile without Claude's conversion helpers;
  message `b8c75f85-6d46-4323-9b2c-7b9e19028185`.
- 2026-11-04 before 08:55:48 received — Claude proposed stacked PR on his
  conversion-only prerequisite; message `b77fe814-3aab-47f4-8bcd-8c3398f734c2`.
- 2026-11-04 08:55:48 sent — Codex accepted stack, requiring rebase onto Claude's
  final reviewed head to prevent a diff that removes later prerequisite work;
  message `05710c4c-e1d4-4ba2-8171-af023c36bb33`. Greg merges and retargets.
- 2026-11-04 before 09:01:28 received — Claude published conversion prerequisite
  `6b2db119a20cc21b37755b1a35b774685db56a52`, warning of temporary unused helper;
  message `dbdb31ec-4b5c-4b1f-9df2-4c3dcb238f8e`.
- 2026-11-04 before 09:01:28 received — Claude requested existing `sha2` and
  `crc32fast` as engine dev-dependencies for the required handoff-package test;
  message `d4d80855-a66b-45ac-b71a-f9a831cfb77f`.
- 2026-11-04 before 09:01:28 sent — Codex approved those two dev-dependency and
  engine lock-entry additions, leaving both files to Claude; message
  `3735d7f1-9615-44f0-827e-601bfbfe2677`.
- 2026-11-04 before 09:06:08 sent — Codex reported 3 passing engine tests and
  the format-check finding in Claude's conversion file; message
  `2e52716d-7744-4583-8cab-34922d588187`.
- 2026-11-04 before 09:13:20 received — Claude requested review of PR #7 at
  `99d1291ede1494bad3526e90dc5b4ff269c56038`; message
  `27ecf5fe-fc85-4c67-a412-52d8295d2eb5`.
- 2026-11-04 before 09:13:20 sent — Codex acknowledged PR and rebased onto
  its head with no merge; message `61339e9d-08a4-4a85-902d-5bb739fe001a`.
- 2026-11-04 before 09:13:20 sent — Codex found centroid boundary leakage and
  the handwritten calendar conversion; approved existing chrono dependency in
  shared surface manifest/lock; message `50edd1f9-990b-4c5e-b634-8d2301e58750`.
- 2026-11-04 after 09:13:20 sent — Codex completed review, posted two findings
  on PR #7 and requested fixes; message `e74c9912-9e0a-41e4-a58c-7ccfc76e955f`.
- 2026-11-04 09:19:56 sent — Codex reported full CI's only failure was the
  deliberate boundary regression: 3,033 passed, one failed, 488 unrun due to
  fail-fast; message `9e167723-6469-4ad6-b5c3-4ca0c17215c8`.
- 2026-11-04 before 09:22:50 received — Claude published `c804107`, fixing the
  original boundary case and chrono finding; message
  `69848468-6bdf-4ab8-9bca-6de68eca65ed`.
- 2026-11-04 before 09:25:04 sent — Codex accepted the original fixes but found
  a residual notch leak at boundary-vertex crossings, posted a concrete regression
  on PR #7 and suggested rejecting triangles enclosing boundary vertices;
  message `ee198f5d-39cc-4564-b442-18f4b1e2ebd0`.
- 2026-11-04 before 09:27:38 received — Claude published `0083b450`, with
  boundary-segment clipping against triangle interiors and regressions for the
  vertex-touch case and boundary chords; message
  `b76ec64b-96d6-42d9-a8e0-72988a42eb9c`.
- 2026-11-04 after 09:31:58 sent — Codex posted final review closeout on PR #7,
  accepted both fixes and documented conservative clipping; reported removal of
  the extra untranslated menu paths and a new full CI run; message
  `c3912e69-d634-48c4-aa7f-46b4e2689859`.
- 2026-11-04 09:36:01 sent — Codex published PR #9 at `1003e6b`, based on
  `0083b450`, requested Claude's review and reported full CI and CLI evidence;
  message `bcc9337a-3dec-47e4-8850-fe8d64dc3aa4`.
- 2026-11-04 09:48:39 received — Claude accepted #9 at `1003e6b`, no blocking
  findings; verified contract, geometry, tests and an independent full CLI run;
  three non-blocking notes require a fix or reason; message
  `fcd0ee06-9b6c-40ab-8116-5e9aedb4a48d`.
- 2026-11-04 09:51:10 sent — Codex answered each note with a reason on #9 and
  through Paseo, retained the reviewed production code and announced a final
  notes-only commit after fresh CI; message `7c1fc373-7297-470d-bc31-0356bcb0cfef`.
- 2026-11-04 after 09:51:10 received — Claude accepted all answers, nothing
  open; final #7 head `cd78026` is `0083b450` plus notes only; merge order is #7
  with a merge commit, then Greg retargets #9 to main; message
  `d94c96f1-b070-4048-aa64-b1c04d6ae1a0`.
- 2026-11-04 after 09:51:10 outgoing closeout — final notes-only push notification
  (`Fixes pushed`, reviewed production files unchanged), prepared for transmission
  immediately after this log commit is pushed; no reply requested.

Times labelled "before" are observed UTC bounds, not fabricated delivery times.

## Agreed contract (before dependent implementation)

All helpers in Claude-owned `surface_build.rs`, `pub(crate)`:
`tin_to_polyface(&Tin) -> crate::Result<PolyfaceMesh>`,
`polyface_to_tin(&PolyfaceMesh) -> crate::Result<Tin>`,
`tin_from_handle(&Drawing, Handle) -> crate::Result<Tin>`.
Strict finite/index/degenerate-face validation; quads split and CCW orientation.
JSON `surface` / `other` hex handles. Outside points excluded from statistics
and counted in `outside`; no extrapolation. Edge tolerance is XY distance
`1e-9` drawing units. Contour layers `C-TOPO-MINR` / `C-TOPO-MAJR`, default colour.

## Implementation and checks

- Surface APIs: `contour::contours`, `compare::{elevations,compare_points,compare_vertices}`.
- Contour nodes use shared vertex/edge IDs; no coordinate quantization or bridging.
  Flat triangles and isolated points yield no line; level edges are deduplicated;
  saddle vertices split paths instead of arbitrarily joining branches.
- Decimal boundary regression failed first for 0.3/0.6 at interval 0.1, then passed
  after bounded floating-point snapping of level ratios and vertex elevations.
- Comparison differences are target minus reference; no-overlap statistics are
  absent (`null` in JSON), never misleading zeros or NaN. Near-edge samples clamp
  to the actual closest segment, including Euclidean distance at corners.
- Engine tests failed first with `UnknownCommand`, then all 3 passed after
  implementation; undo restores the full drawing, comparison leaves it unchanged.
- Current limitation: comparison uses linear face search capped at 10 million
  face/sample visits; large surveys return a clear error rather than hanging.
- No contributor-name edit: user explicitly chose to retain `@0bnoxide` credit.
- Full CI is pending final prerequisite: early fmt check flags Claude's literal,
  and engine clippy flags his temporarily unused `tin_to_polyface` helper.
- Those preliminary prerequisite issues were fixed in `99d1291`. Its surface
  and six engine surface-build tests pass, including the contract-v1 validator.
- Cross-review added a failing build-to-contour boundary regression: the retained
  triangle draws `(0,10,0)` beyond the boundary's y=4 edge. Claude owns the fix.
- Large unrelated elevations cannot set the snapping tolerance for low vertices;
  the regression failed before moving tolerance calculation to each vertex.
- `cargo xtask wasm` passed all 14 selected crates. Baseline UI WASM dead-code
  warnings remain; native strict lint is checked by the full CI gate.
- Full engine unit suite passed (443 tests at conversion prerequisite), and
  expanded contour command suite passed (4 tests, including existing layer settings).
- Full `cargo xtask ci` started on the stacked head; no commits or pushes of Codex
  code are made until it passes.
- First full CI on prerequisite `99d1291`: fmt and strict workspace clippy pass;
  nextest stops at the boundary regression (3,033 pass, 1 fail, 488 unrun, 28 skipped).
  Later gates did not run in that invocation. Final CI will be rerun after the fix.
- At `c804107`, the square-boundary regression passes; a concave notch touching
  the triangle diagonal at polygon vertices still leaks a contour through the
  excluded area. The new test fails first and is held for Claude's follow-up fix.
- Actual CLI at `c804107` with Codex working files: synthetic CSV (explicit column
  mapping) → `rtkimport` (4 points) → `tincreate` (4 vertices, 2 triangles) →
  `tincontour` (5 3D polylines: 3 index, 2 minor) → `tinexport` (4 points, 2 faces).
  DXF written, 23,212 bytes. Outputs live only under ignored `target/`.
- The DXF reopens through CLI `info`: 4 points, 1 polyface mesh, 5 3D polylines,
  both agreed layers with default white colour, no skipped entities or read notes.
- Rebased onto `0083b450` without merging; reviewed the final fix and reran the
  surface suite: all 27 tests pass, including both previously failing boundary
  regressions. Conservative clipping is documented: partial triangles are dropped,
  not split to fill right up to the boundary. Optional out-of-range timestamp is omitted.
- Second full CI: 3,260 tests pass, Chinese catalog test fails on the extra menu
  paths `Survey` / `Surfaces`; direct rerun confirms exactly those missing strings.
  Removed the unrequested menu hierarchy to match existing survey commands.
  Both commands remain in the shared registry with their pretranslated labels.

## Final implementation gate

2026-11-04 09:34 UTC: `cargo xtask ci` passes all seven steps (fmt, strict
workspace clippy, 3,525 nextest tests plus doctests, i18n, assets, layers, wasm).
The code is stacked on Claude's reviewed `0083b450` prerequisite; only the six
Codex-owned files differ from that base. No merge or reference-repository write.

PR: https://github.com/0bnoxide/cadcraft/pull/9 (base `slice2/claude-tin`).
Initial code head: `1003e6b047e1a91662a65af1c0f6076380373d78`.
Claude independently accepted that exact code head. All three non-blocking notes
are answered on #9 and accepted by Claude:

1. Hex strings remain the negotiated and documented handle contract. Numeric
   handle compatibility with `tinexport` is optional additional API scope.
2. Work limits remain to bound linear scans. Large surveys need an index or
   traversal improvement, rather than blindly raising the limits. No drone-scale
   performance qualification is claimed.
3. Every target vertex, including unused vertices, is intentionally sampled,
   documented and regression-tested. Explicit XYZ points provide subset comparison.

No source change follows the review. The automated GitHub Codex review also
reported no findings at `1003e6b`. The final commit changes this log only.
2026-11-04 09:54 UTC: fresh `cargo xtask ci` again passes all seven gates,
with 3,525 tests passing and the existing 28 skipped tests.
Greg owns both merges: #7 first using a merge commit, then retarget #9 to `main`.
No native GUI session was exercised. Synthetic CLI outputs and build caches are
removed with this agent's private target directory after the final validation.
