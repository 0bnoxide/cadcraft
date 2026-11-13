# Slice 11 implementation notes

Base: `cf49a049` (`main`). Tracking: [#68](https://github.com/0bnoxide/cadcraft/issues/68).
Horizontal branch: `slice11/codex-horizontal`.
Run Cargo from an isolated checkout and set `CARGO_TARGET_DIR` to a build directory owned by that checkout.
Second branch: `slice11/codex-vertical`, stacked on the horizontal PR.

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
The CLI displays `send_agent_prompt` as **Send message**: 13 outbound implementation calls and
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
- 2026-11-11 00:04 Denver, sent: corrections pushed at `e97cb46bbb65f3552cf71a9e8d009c399b8cfe79`,
  full latest-head review requested again; receipt `cfd0f244-5dfe-4b8b-a580-24131a4a1c87`.
  Received the same provider quota response, reset reported as 02:10. Review thread evidence:
  [endpoints](https://github.com/0bnoxide/cadcraft/pull/78#discussion_r4534941805),
  [lookup work](https://github.com/0bnoxide/cadcraft/pull/78#discussion_r4534941992).
  The code head is followed only by this publication-log update. Both independent posted
  verdicts, the review notes PR and final hosted checks remain outstanding.

## Horizontal base integration

The vertical branch now incorporates horizontal repair `42a55c7f`: inclusive endpoint
queries, consistent station accumulation, stable shallow-arc projection/exchange, strict
radius-roundoff validation and portable checkout instructions. The profile code and its
reconciled receipt log are preserved. This supersedes the older-base status recorded above;
posted review verdicts, the review notes PR and final hosted checks remain required.

A combined arc/profile regression exposed an exported arc recovering one ULP shorter,
which put its exact terminal design/ground station outside the imported alignment.
Import now preserves a finite declared element length only within four floating-point
epsilons of the finite recovered length. Evaluated endpoints, length attributes,
directions and strict profile bounds are still checked. The 100-case regression covers
both turns, rotated/major/shallow arcs and exact terminal design/ground samples; a PVI
one representable station past the endpoint remains invalid.

## Full horizontal review and repair

- The separately claimed numerical/portable-notes corrections were published as `42a55c7f`.
  Both hosted Linux and Windows gates passed for that head. The designated full review at
  that head requested three further corrections; its independent geometry hand checks matched.
  [Review](https://github.com/0bnoxide/cadcraft/pull/74#pullrequestreview-5856481019),
  [review notes PR #84](https://github.com/0bnoxide/cadcraft/pull/84).
- 2026-11-11 America/Denver message log:
  - 02:15 sent: retry full reviews at live heads, `7249c113-c504-4d71-96bd-760a4521b1e5`.
  - 02:16 sent: preserve the separate vertical-integration claim and hold its final review,
    `319b07cf-6287-472c-83e9-3f8d62eea82a`.
  - 02:21 received: full horizontal review requested changes, `bddbbf28-f9b0-4063-b1c8-fe85cbfe606f`.
  - 02:25 sent: proposed stationary-point refinement, an immutable borrowed validated view,
    and true arc carrier bulges, `aaa95ec4-d205-4fd7-8a03-11c74bf95eb0`.
  - 02:26 received: all three repairs agreed, `8e87fe6d-c699-4d13-b07e-74c274ea26b2`;
    1e-7 station checks at offsets 1/50/-1000, test-only validation counter, bulge tolerance
    1e-9, both-format major-arc reopen, and unchanged storage version 1.
  - 02:42 sent: implementation progress and pending full gate,
    `36e3a177-bd1b-4f16-8026-9c61b6ee1633`.
- The new repair branch starts at `42a55c7f` and will fast-forward the horizontal PR without
  a force push. It does not modify the separately claimed vertical integration.
- Spiral search keeps its global distance bounds, then brackets and bisects `(C-P).T = 0`.
  Only roots and element ends compete until the terminal 1e-8 station intervals; refinement
  uses the same 200000-step budget and stops within 1e-9 station units. Off-curve entry,
  exit and compound regressions fail before the correction and pass at 1e-7 station error.
- `ValidatedAlignment` borrows immutable data and caches cumulative lengths. One-shot methods
  retain their signatures; batch queries use the borrowed view. Report input, carrier checking
  and LandXML export reuse it. A cfg(test)-only thread-local counter confirms one full
  validation for 4096 station queries, length queries and projection on 4096 tight spirals.
- Carrier vertices retain one bulged segment for each arc, including clockwise/counterclockwise
  270-degree arcs; only spirals use chords. The generic chord approximation API remains available.
  Staleness compares bulges at 1e-9 and coordinates at 1e-7. Both-format engine tests verify
  major-arc persistence, tolerated rounding and refusal of a real bulge edit.
- Focused checks: 22 alignment tests and five engine command tests passed. Corrected-binary CLI
  checks pass for drawn line/arc geometry, report/exchange, DXF/DWG reopen, off-curve entry/exit/
  compound queries at offsets 1/50/-1000, and public LandXML XSD validation. Full CI passed
  all seven steps: 4157 tests passed, 28 skipped, plus doc tests, i18n, assets, layers and WASM.
  The vertical sampling path
  will adopt the validated view after its integration includes these new horizontal repairs.
- Other full-review observations are tracked in [#83](https://github.com/0bnoxide/cadcraft/issues/83);
  angle points/MOVE remain tracked in [#70](https://github.com/0bnoxide/cadcraft/issues/70).

## Validated profile integration

- Horizontal repairs were published without a force push at `74917b9cff13e29e6c2b8007331d07496a290067`.
  The designated updated review approved that head subject to its hosted Linux/Windows gates;
  both gates subsequently passed. The review notes PR is [#84](https://github.com/0bnoxide/cadcraft/pull/84).
- The separately claimed profile-base integration was published at `ee590a4d`, preserving the
  original profile changes and the exact terminal-profile station roundoff regression. That
  completed head is the base for the current update. The repair patch is carried by a cherry-pick
  without a merge or published-history rewrite; the profile export, storage and notes conflicts
  retain both sets of changes.
- New message log, 2026-11-11 America/Denver:
  - 03:08 sent: frozen horizontal repair head/local gate evidence and proposed validated-profile
    adapter, `020dbe62-6fc8-4add-95c7-1ce999731868`.
  - 03:09 received: profile API adaptation and cherry-pick plan agreed,
    `f65d224e-9fb8-4355-adea-904fc7ba2592`.
  - 03:11 received: horizontal approval conditional on hosted checks,
    `7c971383-49ce-4750-a39c-9fce060caad3`.
  - 03:26 sent: confirmed both horizontal gates green and combined validation progress,
    `86ecdf14-14fd-450a-a0bb-6b37023e1817`.
- `Profiles::validate_on(&ValidatedAlignment)` reuses checked horizontal bounds; existing
  `validate(&Alignment)` entry points remain wrappers. Storage checking, profile export and
  view layout reuse their checked view. Sampling builds one view and checks its completed
  ground result against it. Its deterministic test failed with two full validations before
  the change and now reports one. Profile tests also retain strict rejection one representable
  station beyond the end. All counters are compiled only for tests.
- The combined 40 focused alignment/profile tests and clippy pass. All seven combined CI
  stages passed: 4176 tests passed, 28 skipped, plus doc tests, i18n, assets, layers and WASM.
  Final CLI runs pass for profile sample/create/view, the endpoint regressions, DXF/DWG reopen,
  horizontal and profile LandXML exchange/XSD validation, off-curve projections and undo.
  The new hosted head and designated full profile review remain pending. No PR is merged.

## Profile review and main integration

- 2026-11-11 03:33:47 America/Denver, sent frozen profile head `57334e6c` and complete combined validation, receipt `613d1eae-12d7-4343-9a7b-7f01a1ff8648`.
- 03:36:25 received the posted full profile code approval, receipt `44f64214-0d9b-4766-8bb0-4ca211938ccf`: 40 crate and eight engine tests passed independently, and profile/TIN hand calculations matched. Remaining integration conditions were base conflicts and hosted gates. Non-blocking findings are filed in [#85](https://github.com/0bnoxide/cadcraft/issues/85).
- 03:36:58 sent the live blockers and requested clarification of the no-merge instruction, receipt `65c7993b-8523-4c69-af73-91ba8d338115`; 03:37:17 received the reviewer's interpretation, receipt `051992fd-58e9-4f9e-874b-25d9c77d6301`. Exact provider receipt times were supplied for the review notes, receipt `6d0208ea-2d15-494e-aaac-837224a77b1e`.
- Paseo CLI tool logs independently confirmed at least 22 implementer and 13 reviewer `[Send message]` events, above the required six/four; a later independent check counted 23/14. The review notes PR [#84](https://github.com/0bnoxide/cadcraft/pull/84) is open and changes only its notes file.
- At the owner's continuation, sent the branch-update plan, receipt `15039f59-6193-4229-a37d-38d1c8b158b5`. The reply reported an expired OAuth session. The owner's coordinator then explicitly required merging `origin/main` at `1a1696c78c02b7ee33b43b781b17e0fd1f01d706`, retargeting #78 to main, and stopping after publication, mergeability and hosted-CI start confirmation. Both designated reviewers are unavailable; final re-review will be requested when one returns.
- The uncommitted earlier base update had an unchanged tree. Its pending merge metadata was cleared without resetting files or changing a branch; the normal merge now incorporates main. The four overlaps retain profile validation/export and the receipt history while preserving main's COPY translation, malformed-dictionary checks, foreign/reactor handling and all later fixes. No rebase, force push or PR merge is performed.
- Final integrated verification passed before publication: unchanged `cargo xtask ci`, all seven stages (fmt, clippy, test, i18n, assets, layers, WASM), 4202 tests passed and 28 skipped; nextest run `5c64a812-5d50-477d-92bd-1560aa729db6`. Main's automatically merged fixes outside profile integration are byte-identical to main. The earlier CLI evidence remains tied to `57334e6c`; it was not rerun for this main-only integration. Hosted checks and re-review are recorded separately on the published PR.
