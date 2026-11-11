# Slice 11 implementation notes

Base: `cf49a049` (`main`). Tracking: [#68](https://github.com/0bnoxide/cadcraft/issues/68).
Worktree: `.claude/worktrees/slice11-codex-horizontal`; branch `slice11/codex-horizontal`.
Private build directory: worktree `target/slice11-codex`, set on every Cargo invocation.
Second branch: `slice11/codex-vertical`, stacked on the horizontal PR. No merges authorized.

## Design and execution plan

1. Host-free geometry in `crates/alignment`: validated elements, joint diagnostics, station formatting,
   true-curve evaluation and global nearest projection, bounded chord approximation; independent answers
   and hostile-input tests in `tests/horizontal.rs`.
2. Prove standard extension-dictionary/XRECORD persistence in DXF and DWG, including a second reopen
   and a payload larger than xdata allows. Then add carrier regeneration/staleness validation in
   `alignment_store.rs`; preserve other applications' dictionary entries.
3. Bounded namespace-aware LandXML parser/writer in the new crate; synthetic exchange and refusal
   tests. Register four thin horizontal commands and their Chinese label rows. Test command/undo,
   polyline arc conversion, exchange, MOVE/grip rejection and both-format reopen.
4. Run focused tests, diff review, and `cargo xtask ci`, open horizontal PR and request full review.
5. On the stacked branch add symmetric parabolic design profiles, true-curve triangle crossings,
   bounded ground sampling and host-free view marks; register three profile commands, extend exchange
   and storage tests, run the gates and request review. Address findings with regression tests.

No new external dependency, CI workflow edits, reference repository or product sources. Public
LandXML 1.2 schema and standard route geometry only. `crates/handoff` remains untouched.

## Agreed contracts

- XY = easting/northing; radians CCW from +X; left offset positive. at() outside the inclusive
  station range errors. project() clamps beyond endpoints and returns a flag and lateral component.
  At a joint the later element answers; nearest ties choose lowest station.
- Joint limits: 1e-6 units / 1e-8 radians; commands refuse every reported gap or direction break.
  No angle-point option. Elements capped at 4096; generated points at 100000. XY coordinates are
  bounded to 1e9 and element length to 1e7. Start stations accept any finite value for which the
  finite station extent remains representable; arithmetic precision/overflow returns an error.
- Spirals have unequal endpoint curvatures of one sign, including a tangent at either end; total
  turning <=16*pi. Integrate locally with the 16-term exponential polynomial, phase <=.25 radians
  per interval. Projection uses true-curve candidates and certified chord lower bounds, not one
  local minimum. Distance ties within 1e-9; numerical exhaustion refuses the query.
- Storage version1: exact definition + chord tolerance (+ profiles later), no carrier snapshot.
  An entity extension dictionary owns `CADCRAFT_ALIGNMENT` XRECORD with ASCII JSON in <=240-byte
  strings; payload <=8MiB. Regenerate the carrier and compare at 1e-7. MOVE and grip edits are stale.
- Exchange reads N E [Z], uses schema compass directions, verifies stated geometry with explicit
  mismatch errors; unsupported geometry refuses the whole affected alignment. Station equations
  generate named warnings. PR1 imports horizontal geometry with a warning for profiles; PR2 handles
  PVI/ParaCurve and separate ProfSurf segments. Station format is display-only, not exported.
- Reuse tinexport's `unit` argument/default meter on import/export; no INSUNITS mutation or stored
  unit. Scale imported lengths, coordinates, stations and elevations into those drawing units.
- Ground samples include actual triangle edge crossings, tangent hits and element ends, then use
  the plane-gradient curvature bound for elevation chord error. Gaps are separate segments.
  Profile grade = rise/run. Views draw a new grid/ground/design figure in one undo step, deleting
  nothing. Layers: PROFILE-GRID, PROFILE-TEXT, PROFILE-GROUND, PROFILE-DESIGN.

## Paseo message log

Messages use `[slice11]` only; own id `c2f6c537-b8b1-4a44-a5d3-039989a37c27`, reviewer
`c247b28f-eab2-4645-85eb-cf13e141f31d`. All five agreements were received before the first
implementation commit. Times below were reconciled from the provider records behind Paseo's
`logs <id> --filter tools` output; sent times identify the tool call, received times its record.
The CLI displays `send_agent_prompt` as **Send message**: 12 outbound implementation calls and
7 outbound review/design calls are present, exceeding the required six and four.

| Recorded 2026-11-10 America/Denver | Direction | Message / receipt |
|---|---|---|
| 21:48:02 | received | Reviewer hello/id, `fe4dc566` |
| 21:48:25 | sent | Setup acknowledgement, `7fe0a83d` |
| 21:49:36 | sent | API proposal, `0ccaf0db` |
| 21:50:11 | received | API agreement with nine pins, `0efaade8` |
| 21:50:59 | sent | Accepted API pins, clothoid proposal, `5c5d9f7f` |
| 21:51:22 | received | Clothoid agreement with four pins, `6621485a` |
| 21:51:54 | sent | Accepted clothoid pins, storage proposal, `88139b1a` |
| 21:52:27 | received | Storage agreement with five pins and integration file permission, `da249b04` |
| 21:53:15 | sent | Accepted storage pins, LandXML proposal, `6b25a552` |
| 21:53:53 | sent | Profile sampling/view proposal, `52be26d2` |
| 21:53:59 | received | LandXML agreement with eight changes, `486cbf67` |
| 21:54:33 | received | Sampling/view agreement with seven changes, `a7757343` |
| 21:54:52 | sent | Accepted mapping pins, existing unit parameter evidence, `adaa8113` |
| 21:57:01 | received | Unit parameter/default agreed, `70a8e68c` |
| 21:57:38 | sent, receipt time | All pins accepted, implementation plan and tracking issue, `0d0f37b7` |
| after 21:57 | received | Reviewer's provider quota exhausted; reset reported as 02:10 America/Denver |
| 22:33:24 | sent | Public XSD retrieved/validated, ProfSurf gap mapping, CI/follow-ups, `e4928f8a` |
| 22:39:53 | sent | Full horizontal PR review requested, `07a3fb8d` |
| 23:29 | sent | Full vertical PR review requested, `5443c715-9016-4380-bb7c-c4e7ab920113` |
| 23:29 | received | Provider quota response again; full reviews remain pending |
| 23:30 | sent | Notes-only follow-up and latest-head review handoff, `134dc57a-b25f-4fdf-98a0-9d31d43a5e7b` |
| 23:30 | received | Provider quota response again; reset still reported as 02:10 Denver |

## Evidence and open checks

- Horizontal geometry: six known-answer/hostile tests pass locally on Windows, including the
  line/spiral/arc/spiral/line route, independent Fresnel series, compound/exit inversion, arc-centre
  ties, arbitrary finite start stations and direction normalization. Five LandXML tests pass (round trip, unit
  scaling, per-alignment refusals/warnings, malformed declarations, nesting/count/nonfinite limits).
- Five engine tests pass: selected polyline with an arc, report/exchange, full stored-definition
  DXF/DWG reopen, one-step undo, failed-create preservation, COPY/MOVE behavior, width/grip staleness,
  preserving another application's dictionary entry, and a 40KB XRECORD through two reopen cycles
  in each format. Each API/command/storage arc was observed failing before implementation.
- `cargo clippy -p cadcraft-alignment --all-targets -- -D warnings` and the same focused engine
  gate passed. Standard `cargo xtask ci` hit the known running-executable lock
  [#41](https://github.com/0bnoxide/cadcraft/issues/41). A private Cargo target runner copies only
  the outer driver so the same seven gates can run. Build concurrency is two (the initial default
  exhausted linker memory). The sandboxed run reached 3225 passes before the existing MCP token
  rotation test was denied credential-file access. Host-access nextest run
  `35d73bf1-6f25-4dc2-8b61-e47c8d73260f`: 4144 passed, 28 skipped; doc tests, i18n, assets and
  layers and WASM passed. Full seven-gate `cargo xtask ci` passed with host access and the private
  outer-driver runner. WASM emitted three existing dead-code warnings in the UI rebuild module.
- Through `cadcraft-cli run`: draw a polyline with a quarter-circle arc, `alignmentcreate`, report
  element table/station/offset, export/import LandXML, save and reopen both DXF and DWG: passed.
  The assertion-based smoke script and its synthetic outputs stay in the ignored private target.
- The existing Session guard restores panics, but does not roll back an ordinary Err after partial
  mutation (`session/execute.rs`). Commands must stage changes on a Drawing clone and replace once,
  following `subsurface_draw.rs`, where a later fallible addition could otherwise leave partial work.
- The original [public LandXML 1.2 XSD](http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd)
  was retrieved over HTTP after HTTPS reset connections and the ICSM mirror timed out. Checked
  Line/Curve/Spiral, Profile/ProfAlign/ProfSurf, PVI/ParaCurve, StaEquation and angle/direction
  definitions. The installed lxml validator accepts the CLI line/arc and clothoid exports against that XSD.
  ProfSurf defines gaps as multiple PntList2D children; PR2 uses this exact standard mapping.
  No third-party implementation source was opened or used.
- Hosted Linux/Windows CI passed at the published horizontal head. Native UI and independent full
  PR review remain open; the reviewer reported a provider quota limit until 02:10 Denver.
- Follow-ups outside this PR: [rounded input coordinates #69](https://github.com/0bnoxide/cadcraft/issues/69),
  [angle points and MOVE #70](https://github.com/0bnoxide/cadcraft/issues/70),
  [COPY exact definitions #72](https://github.com/0bnoxide/cadcraft/issues/72), and
  [unusual clothoids without a usable LandXML PI #73](https://github.com/0bnoxide/cadcraft/issues/73).

## Horizontal publication and vertical implementation

- Horizontal PR: [#74](https://github.com/0bnoxide/cadcraft/pull/74), head
  `000afb94b93f81e7626b0e34a7a302db61c95481`, base `cf49a049`. Hosted Linux and Windows gates
  passed. Review request sent by Paseo at 22:39:53 Denver, receipt
  `07a3fb8d-45b9-4476-bd90-a71966dc4ff8`; the provider again reported quota exhaustion.
- Vertical branch: `slice11/codex-vertical`, created from the exact horizontal head in the same
  assigned worktree; the build directories remain private to this worker. No merge or rebase.
- `profile.rs` supplies PVI validation, symmetric parabolas, grade ratios, extrema and bounded
  profile tessellation. `sampling.rs` intersects the true elements with triangle edges after
  bounding-box rejection; station intervals identify a triangle and use its plane-gradient /
  curvature bound. Isolated contacts are point segments. Crossing deduplication is exact so it
  cannot bridge a narrow uncovered gap.
- The new profile commands use the shared TIN conversion, stage updates, persist profiles on the
  carrier and draw a new figure without deletion. Ground/design/grid/text have separate layers;
  ground is green, design red and grid gray. Ground gaps remain separate entities.
- `landxml::import` now returns `Route { definition, profiles }` bundles. The geometry-only
  `export` remains available; `export_routes` includes profiles. The engine stores optional
  profiles in version-1 data, accepting horizontal-only files with the field absent. Ground
  gaps are separate PntList2D children of one ProfSurf, as the XSD specifies.
- This first model stores one ground and one design profile. Multiple named profiles are refused
  explicitly rather than discarded; [#75](https://github.com/0bnoxide/cadcraft/issues/75) tracks collections.
- Eleven new host-free tests cover crest/sag values at five stations and extrema, overlap/abutment,
  later-tangent grade, nonfinite/range refusals, standard profile exchange/refusal, exact line/arc/
  spiral crossings, the radius-1 z=x arc example, gaps including a 5e-10 gap, isolated contacts,
  view placement/exaggeration and 200000 triangles with 50 elements (about 0.04s for the test group
  in the local optimized test build). The command test covers sample/create/view, undo, both-format
  storage and profile exchange. View and command tests were observed failing before implementation.
- A final crossing regression proved an edge-end tolerance could admit a root outside the finite
  edge. The segment parameter now stays strictly in [0,1]; the test failed before the change.
- Automated horizontal review found inclusive endpoint subtraction roundoff and nonportable notes.
  The owner has a separate claim on both corrections; the vertical branch leaves that work untouched.
  Follow-ups: [endpoint #76](https://github.com/0bnoxide/cadcraft/issues/76),
  [portable notes #77](https://github.com/0bnoxide/cadcraft/issues/77). Recheck the horizontal head
  before integration; this branch remains based on its original published head.
- CLI profile smoke: TIN -> sample -> design -> view, DWG reopen, LandXML reimport/view and public
  XSD validation pass on the final code. Profile PNG inspected with the headless renderer: the
  red design crest, green ground line, grid and all station/elevation labels are visible.
- Initial vertical `cargo xtask ci`: all seven gates passed with the same private outer driver,
  two build/test workers and host access. Nextest `3b6a2a36-0dcf-4d2f-9c84-fb9e50ef2d72`:
  4157 passed, 28 skipped, followed by passing doc tests, i18n, assets, layers and WASM.
  Hosted vertical checks and independent full reviews remain pending at this note revision.
- Vertical PR: [#78](https://github.com/0bnoxide/cadcraft/pull/78), code head
  `104107a0959a57c421a4161fe9280a407b598bd5`, base `slice11/codex-horizontal` at `000afb94`.
  Linux and Windows hosted gates started. Full review requested with code head and evidence;
  the reviewer was told to resolve the latest PR heads after this notes-only follow-up. Both
  posted verdicts, the review notes PR, any resulting corrections and final hosted gates remain
  required. The worktree is retained for those corrections. Nothing has merged.

## Vertical review corrections

- [Exact tessellation endpoints](https://github.com/0bnoxide/cadcraft/pull/78#discussion_r4534731387):
  reproduced the valid 102.51-unit design failing with OUT_OF_RANGE. The same arithmetic also
  failed ground sampling on a radius-100 arc with five subdivisions. Both regressions failed
  before the correction. A shared helper now retains each exact interval endpoint.
  A nonzero-origin two-element ground regression also failed because `(start + L1) + L2`
  differed from `start + (L1 + L2)`; cumulative distance is now formed before adding the origin,
  matching the horizontal station bounds and keeping shared endpoints identical.
- [Triangle-location work](https://github.com/0bnoxide/cadcraft/pull/78#discussion_r4534731389):
  replaced the repeated full triangle scan with a median-split bounding-box index, built once.
  Construction depth is capped at 20. Every visited lookup node and tested triangle spends
  from the same 5000000 crossing/location budget; exhaustion returns NUMERICAL_LIMIT.
  Lowest original triangle index still wins shared-edge ties. A 16000-triangle strip regression
  performs 8000 lookups within 1000000 work units; the instrumented old linear scan exhausted
  that allowance. Zero-budget refusal and shared-edge tie behavior are tested. The existing
  200000-triangle/50-element sample still passes (focused test group about 0.07s locally).
- Focused alignment tests and clippy pass after both corrections. Superseded full CI runs were
  stopped as the ground regressions exposed related occurrences. The replacement gate passed
  all seven steps on the complete correction: nextest `8188e458-8177-496f-aa26-334bcc2f3e08`,
  4160 passed / 28 skipped, followed by doc tests, i18n, assets, layers and WASM.
- One Paseo continuation (`52c56f37`, maximum one run) is scheduled for 2026-11-11 02:15
  America/Denver to retry the required full reviews after the reported 02:10 quota reset.
- CLI checks on the corrected binary pass for the 102.51-unit design creation/view and the
  radius-100 ground sampling/view. The full profile smoke also passes again, including DWG
  reopen, LandXML reimport, public XSD validation and undo. A single-contact PntList2D also
  validates against the public XSD.
