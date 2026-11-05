# Slice 4 Codex notes

- Branch: slice4/codex-labels; base main 9e9d054451232f12bd0bf984b886944ea7703fe1.
- Worktree: .claude/worktrees/slice4-codex-labels.
- Every Cargo invocation uses CARGO_TARGET_DIR=<worktree>/target-slice4-codex.
- Operator: credit @0bnoxide only; no people.toml changes; no merges/main pushes.
- Sonar host Keychain access required; elevated scans pass. plan/STATUS.md absent.

## Paseo message log (UTC)
- 2026-11-05 initial OUT d5772f6d: hello/identity, requested formatter proposal.
- 2026-11-05 IN 148291be: formatter API proposal.
- 2026-11-05 OUT 04de3a6d: agreed formatter; requested bounded output/width and invalid increment rule.
- 2026-11-05 OUT 5f48bb86: scanner restored; full operator constraints confirmed.
- 2026-11-05 IN e29e3e55: bounds adopted, format_number helper, 11 tests passing reported by Claude.
- 2026-11-05 OUT: final formatter confirmation; proposed font-layout measurement and versioned XData ownership contracts. Awaiting agreement before dependent implementation.
- 2026-11-05T06:04Z IN 770dc753: measurement agreed; ownership agreed with DXF string limits and APPID conditions. OUT 6957b06e accepted UTF-8 chunks and id cap. IN 4c87b1d6 agreed one-line shared APPID registration (OUT 6edf6393 request).
- 2026-11-05T06:04Z: placement regression tests failed on missing API before implementation; both now pass. Crowded boundary and hostile dimensions covered.
- 2026-11-05T06:14Z OUT 7aca5f07 / PR comment: formatter review at 9ba1d554 found allocation before truncation; 11 tests passed. IN 30140d6f: ef04d14a fixes bounded append, CI passed per Claude. OUT 8da09647: 12 tests pass, finding resolved; PR comment records approval. Feature branch fast-forwarded only to agreed ef04d14a (agreement ba8492f7).
- 2026-11-05T06:19Z: engine callout and batch rerun tests pass; long Unicode tag DXF save/load verified. Segment length/bearing, TIN vertex/grid elevations and feature-code property filters pass. Full CI compiling in dedicated target directory. Point codes use explicit properties keyed by point handle because existing imported POINTs contain geometry only.
- 2026-11-05T06:25Z: Codex implements the larger share in callout/placement/template and two commands. Shared change only: APPID array registration, approved by Claude message 4c87b1d6-a2c0-45bf-805e-32e703babc95. Formatter is inherited base ef04d14a, not edited by Codex. No CI/config/localization/credit file changes.
- References read as behavior only: AutoGIS callout_geometry.py, callout_templates.py, callout_collision.py, label_expression_common.py and test_rules_and_callouts.py. Only label_genie.md read in CIM repository; own JSON template names and structures. No reference repository writes.
- Limits: 2048 labels/obstacles, 4096 cells, 256 rows, 64 columns, 256 parts, 64-byte template ids; oversized or impossible boundary placement errors rather than placing outside. Labels operate in model space. Feature codes are supplied as properties keyed by point handle. Unknown ownership versions are untouched. Grid source keys use surface handle and integer grid indices; edits to source topology can require deleting/rebuilding that template.
- 2026-11-05T06:21:46.4998329Z verification: initial sandbox CI fmt/clippy passed; 3 MCP tests failed with Windows code 5 PermissionDenied. Host-access cargo xtask ci rerun is executing tests. All publication waits for full success.

### Additional Paseo ledger (all exchanges before the implementation commit)
- OUT 28b54a94: final formatter confirmation and measurement/tag proposals.
- IN f7b9a96f: Claude PR #13 at 9ba1d554, CI reported green.
- OUT a2f3a867: request exact formatter base; IN ba8492f7: base agreed.
- OUT dff4199e: placement progress, requested early formatter head.
- OUT 4e27faf0: formatter fix inspected; engine rerun and roundtrip progress.
- OUT c510a3fc: full CI first-build progress, no action required from Claude.
- IN agent completion notifications: Claude repeatedly reported waiting for its CI or Codex PR; no design changes in these notifications.

### Commands and behavior
- callout: anchor [x,y], record object, rows of cells, each cell an array of formatter parts; optional height/padding/boundary/kind=table|mtext.
- labelbatch: template {id, source, content, height, leader?, boundary?}; optional properties {point_handle:{feature_code,...}}. Sources: points (layers/codes), segments (layers), surface_vertices (handle), surface_grid (handle/spacing). Delete using {delete:template_id}.
- Coordinates in drawing units; segment bearing is clockwise from north in degrees. 3D polylines use spatial length; lightweight polyline arcs use arc length and chord bearing.
- Tests port the relevant standalone behavior (title spans, box/grid sizing, nearest edge attachment, collision detection/avoidance, locked overrides, crowded fallback). ArcGIS figure generation/overrides storage remain excluded.
- 2026-11-05T06:22:40.4172867Z host CI: 3618 tests passed, 28 skipped; fmt/clippy/test/i18n/assets/layers reached successfully, WASM check running. No native AutoCAD/UI qualification claimed.
- 2026-11-05T06:23:19.8471133Z final host verification: cargo xtask ci exited 0; all 7 steps passed (fmt, clippy, test, i18n, assets, layers, wasm), 3618 tests passed and 28 skipped. Log retained locally as slice4-ci-host.log (not committed).
- 2026-11-05T06:24:14.8240995Z OUT f93aee1d: PR #15 published at 89368c75abd386977f2caddd91daad88763e7d8e; requested Claude exact-head full review and tests. GitHub verified OPEN to main at exact head; review pending. Original checkout preserved (pre-existing untracked CLAUDE_TEST.txt only).
- 2026-11-05T06:24:14.8240995Z OUT 6d56bcfb: reported 3618 passing tests, 28 skipped and final WASM stage; IN e3316571 Claude acknowledged waiting to review PR. Target directory retained until review/fixes finish, then remove own target only.
- 2026-11-05T06:30:56.1216362Z IN a40c4a24: Claude requested changes at 89368c75 (boundary clamp crash, missing behavior cases, stale sources, quadratic lookup). OUT f56baed4: reproduced panic red, fixed placement and expanding tests. IN b3fec696: reminded title+analyte table test. Automatic GitHub review also found model/paper mismatch, existing callout obstacles, and LwPolyline elevation loss; all verified in code.
- Review revision: safe ordered bounds and edge clipping remove f64 clamp panic; explicit buffer/obstacle hit indices; title/analytical rows and same-point/crowded tests; existing tables/MText become callout obstacles; model-only batch guard; lightweight-polyline elevation preserved; owner index built once, unproduced source labels removed only for current known template/version/role.
- 2026-11-05T06:34:16.8448421Z OUT 8aa6bcb0: requested fixes implemented, title+analyte structure included; 7 placement and 9 engine label tests passed. Full host cargo xtask ci then exited 0: all 7 gates passed, 3625 tests passed, 28 skipped. Review-fix log: slice4-ci-review.log (local only).
- 2026-11-05T06:34:49.7627668Z OUT 14d13a59: fixes pushed at 6d87230dfd9992a6d2b002c049f17026f019dcd8; requested Claude exact-head re-review. Posted detailed answers to every finding on PR #15 (issuecomment-6651486592). GitHub verified OPEN at matching head. Final re-review pending.
- 2026-11-05T06:36:22.0549666Z IN bd777931: Claude approved implementation exact head 6d87230d; all four findings resolved; 21 labels + 9 engine label tests pass independently. Claude will push notes-only #13 update. Greg should merge #13 first, then #15; no feature rebase required for a distinct notes file. Final notes-only Codex commit does not change approved code.
- 2026-11-05T06:36:48.2988293Z OUT 3d52ced4: acknowledged exact-head approval; final notes-only commit + own target cleanup; agreed to leave Greg merge order #13 then #15. Live PR #15 OPEN/CLEAN at 6d87230d, no hosted status checks returned; local CI evidence remains separate from hosted CI. Approval URL: https://github.com/0bnoxide/cadcraft/pull/15#issuecomment-6651501348.
- 2026-11-05T06:38:26.8827016Z final notes-only verification: cargo xtask ci exited 0, all 7 gates passed; 3625 tests passed, 28 skipped. Approved implementation is unchanged from 6d87230d. Own build target cleanup follows publication; CI logs stay local.
- 2026-11-05T06:39:22.6274888Z IN 33aa4463: Claude final #13 head 1115030 is notes-only and all 7 CI gates passed. OUT final acknowledgment through Paseo: approved implementation remains 6d87230d, final notes only, own target cleanup, Greg merges #13 then #15; no reply needed. No code changes since the last full successful CI.
