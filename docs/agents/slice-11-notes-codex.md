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
implementation commit. Receipt identifiers below preserve ordering; exact transport timestamps
still need reconciliation from the message receipts.

| Recorded 2026-11-10 America/Denver | Direction | Message / receipt |
|---|---|---|
| 21:49 | received | Reviewer hello/id, `fe4dc566` |
| 21:49 | sent | Setup acknowledgement, `7fe0a83d` |
| 21:50 | sent | API proposal, `0ccaf0db` |
| 21:50 | received | API agreement with nine pins, `0efaade8` |
| 21:51 | sent | Accepted API pins, clothoid proposal, `5c5d9f7f` |
| 21:51 | received | Clothoid agreement with four pins, `6621485a` |
| 21:52 | sent | Accepted clothoid pins, storage proposal, `88139b1a` |
| 21:52 | received | Storage agreement with five pins and integration file permission, `da249b04` |
| 21:53 | sent | Accepted storage pins, LandXML proposal, `6b25a552` |
| 21:54 | sent | Profile sampling/view proposal, `52be26d2` |
| 21:54 | received | LandXML agreement with eight changes, `486cbf67` |
| 21:55 | sent | Accepted mapping pins, existing unit parameter evidence, `adaa8113` |
| 21:56 | received | Sampling/view agreement with seven changes, `a7757343` |
| 21:57 | sent | All pins accepted, implementation plan and tracking issue, `0d0f37b7` |
| 21:57 | received | Unit parameter/default agreed, `70a8e68c` |
| 21:58 | received | Reviewer's provider quota exhausted; reset reported as 02:10 America/Denver |
| 22:33 | sent | Public XSD retrieved/validated, ProfSurf gap mapping, CI/follow-ups, `e4928f8a` |

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
- Hosted Linux/Windows CI, native UI and independent PR review have not yet run. Nothing is
  represented as ready or merged; the reviewer reported a provider quota limit until 02:10 Denver.
- Follow-ups outside this PR: [rounded input coordinates #69](https://github.com/0bnoxide/cadcraft/issues/69),
  [COPY exact definitions #72](https://github.com/0bnoxide/cadcraft/issues/72), and
  [unusual clothoids without a usable LandXML PI #73](https://github.com/0bnoxide/cadcraft/issues/73).
