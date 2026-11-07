# Slice 8 implementation notes

Designs agreed through Paseo on 2026-11-07 before implementation.

- P1: checkpoint CSV uses required m|ft, converting to feet for existing grading. Supersede binds a prior ID, uses existing commit checks, refuses self/cross-mission/already-superseded/mismatched links, and appends one atomic audit event.
- P2: drawing overrides keyed by template/source; set replaces, clear removes, list flags dormant entries. Locked placement persists; quadrant-only unlocked overrides preserve automatic anchor/offset.
- P3: from,to map headers normalized; codes exact. Comma/tab input, unchanged comments, short rows copied and reported, tabs inside fields refused for tab output. Preserve other fields, refuse all output collisions.

## Message log (UTC receipt dates; precise timestamps in Paseo logs)

2026-11-07 received Claude hello (8c6d6f49).
2026-11-07 sent hello/worktree context (24df733c).
2026-11-07 sent P1 (545513a9), P2 (7a00ef2e), P3 (45abbb2c).
2026-11-07 received P1 agreement/amendments (453566c5), P2 (3becdbf7), P3 (772842a4).
2026-11-07 sent acceptance of all amendments (db8188e0).

## Execution

Base main b2d05144. Private CARGO_TARGET_DIR is target-slice8-codex inside this worktree; set explicitly for every cargo invocation.
Implement and verify #29, checkpoint input, supersede, overrides, converter; run cargo xtask ci, review diff, open PR, request full review through Paseo, fix findings. No merge.

## Implementation plan

1. Add the metric-template/inch-layout regression in proposal.rs, run it red, remove header mutation and resolve page-unit fallback using the existing page setup, then rerun.
2. Add checkpoint parser tests adapted from the reference, implement bounded CSV parsing into ParseResult, expose through droneqa and workflow context, verify grading and units.
3. Add supersede adversarial tests, extend the existing shared commit path and hash-chain replay with one supersession event, verify old histories still load.
4. Add placement override tests and persisted template/source records; wire set/clear/list and both rerun paths. Verify drawing round trip and dormant entries.
5. Add converter preservation/collision tests; reuse subsurface::sheet for maps and existing survey parsing where suitable; wire pointconvert and labels.
6. Run command demonstrations, cargo xtask ci, inspect diff, commit/push and open PR; request Claude review and address every finding. Never merge.

## Design follow-ups and message log

All exchanges below were completed before 2026-11-07T02:27:24Z and before the first implementation commit. These are observation-time bounds, not fabricated send timestamps; exact delivery times remain in the Paseo message records identified below.

- Sent #29 shared-reader proposal (6ca95b29); received rejection and proposal-local scaling correction (e0e73ab0); sent acceptance (1ec36e3b). Shared paper reader remains unchanged.
- Sent progress/carrier exploration (cbf34459), Point-carrier proposal (5b1fa32c); received carrier requirements (6b530aa6, 93b09f2d).
- Sent empty-block alternative after inspecting TIN point consumers (902df3fd); received agreement plus purge protection requirement (822f0c3a); sent acceptance (cec539be).
- Sent implementation/test progress and mission-identity correction (a15eb1ae).
- Sent DXF/DWG native-format clarification and legacy compatibility policy (77232e03); received confirmation and request for distinct legacy refusal (09b4260c).

Final override storage is an empty, uninserted CADCRAFT_OVERRIDE_* block with a JSON description capped at 1920 bytes on read/write. This supersedes the initial marker/xdata proposal. No survey Point is created. tincreate directly reads Points, so avoiding Point metadata prevents contamination without changing survey consumers. PURGE protects the reserved block prefix.

Supersession uses a hash of canonical mission GlobalID, plus the reviewed prior link. Legacy records without this key load/review/commit normally but cannot be superseded (LEGACY_SUPERSESSION_TARGET).

## Validation so far

- Regression observed failing, then passing: metric MEASUREMENT with inch plot units; checkpoint units/conversion; supersede action; cross-GlobalID supersession; override command/rerun; purge persistence; pointconvert preservation/collision.
- DXF and DWG override description round trips passed. Rerun-after-reopen and maximum valid payload cases added.
- First cargo xtask ci: fmt/clippy passed; test stopped at three MCP token tests with host filesystem Access Denied. 3121 tests passed, 3 failed, 28 skipped, 827 not run. Direct token test reproduced denial in sandbox and passed with host access. Full CI is rerunning with host access.
- No new dependencies, no workflow edits, no contributor identity edits, no merges.

2026-11-07T02:35:27Z validation update: full host CI passed all seven steps, with 3953 tests passed and 28 skipped. Three existing WASM dead-code warnings remain warnings; Clippy passed with -D warnings.

CLI demonstrations passed checkpoint grading, reviewed supersession, cross-mission refusal, override set plus repeated labelbatch, pointconvert unknown codes and collision refusal, and newproposal from a metric template with inch plot units. The first synthetic proposal demo was refused for missing required profile placeholders; corrected the demonstration manifest, then creation succeeded and the exported DXF retained MEASUREMENT 1.

Final diff review strengthened override coverage to leaders and found original-source leader starts ignoring manual anchors (regression failed at 1 vs 10). Both batch and standalone callout paths now use the overridden request anchor. Full host CI rerunning before commit/push.

Sent progress message ee04d691, received CI/native-format follow-ups as recorded above. Source reference repos remained read only; only the permitted Point File Converter dossier was read from the commercial-reference repo.

Final review also reproduced and fixed overridden leader starts in both callout paths; the stronger leader regression checks the actual anchor. The following CI run passed tests but assets correctly rejected synthetic CLI-demo DXFs left under the private build folder. Removed only that generated demo folder after verifying its resolved absolute path, then reran full CI. This was test-artifact cleanup, not a source-code failure.

Hosted workflow coverage was verified from the existing ci.yml: Linux runs cargo xtask ci; hosted Windows runs workspace nextest tests only, by the repository's existing explicit optimization. Local Windows validation runs the full seven gates. No workflow edits.

Final checkpoint CSV review added quoted-field coverage (quoted headers, numeric fields and an ID containing a comma). The regression failed before the parser fix and passed afterward. Full CI after demo cleanup passed; full CI is running again on this last parser change before publication.
2026-11-07T02:44:40.4276470Z - Final pre-push cargo xtask ci passed all seven gates on Windows with host access (3954 tests passed, 28 skipped). No source changes after this run.
