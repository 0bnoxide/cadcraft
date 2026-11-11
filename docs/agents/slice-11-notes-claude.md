# Slice 11 reviewer notes

Review-only role: design agreement by message, a full review of each code PR, no code. Brief: `slice-11-alignments-profiles.md`.

## Message log

Times are America/Denver, taken from the author's provider log of receipts (sent to the reviewer on 2026-11-11 03:4x); the reviewer's own Paseo log did not yield timestamps. "out" is reviewer to author, timed at the author's receipt. "in" is author to reviewer, timed at the author's send. The short code is the start of the Paseo message id.

| Time | Direction | Summary |
|---|---|---|
| 11-10 21:48:08 | out | fe4dc566: reviewer id; asked for the five proposals, API first. |
| 11-10 21:48:32 | in | 7fe0a83d: acknowledged. |
| 11-10 21:49:42 | in | 0ccaf0db: proposal 1, `crates/alignment` API. |
| 11-10 21:50:16 | out | 0efaade8: proposal 1 agreed with nine pins. |
| 11-10 21:51:05 | in | 5c5d9f7f: pins accepted; proposal 2, clothoid evaluation. |
| 11-10 21:51:26 | out | 6621485a: proposal 2 agreed with four pins. |
| 11-10 21:52:00 | in | 88139b1a: proposal 3, storage. |
| 11-10 21:52:38 | out | da249b04: proposal 3 agreed with five changes; shared-file edits agreed. |
| 11-10 21:53:25 | in | 6b25a552: proposal 4, LandXML. |
| 11-10 21:53:57 | in | 52be26d2: proposal 5, sampling and view. |
| 11-10 21:54:09 | out | 486cbf67: proposal 4 agreed with eight changes. #69 and #70 filed. |
| 11-10 21:54:59 | out | a7757343: proposal 5 agreed with seven changes; PR 1 may start. |
| 11-10 21:5x | in | adaa8113: `tinexport` already has a `unit` parameter (default meter); proposes reusing it. |
| 11-10 21:57:15 | out | 70a8e68c: `unit` parameter agreed. |
| 11-10 21:5x | in | 0d0f37b7: proposal 5 pins accepted; tracking issue #68. |
| 11-10 22:33:27 | in | e4928f8a: schema reached over HTTP; #72 and #73 filed. |
| 11-10 22:40:01 | in | 07a3fb8d: PR 1 up, #74 at 000afb94. |
| 11-10 23:29:23 | in | 5443c715: PR 2 up, #78. |
| 11-10 23:30:20 | in | 134dc57a: review request repeated. |
| 11-11 00:04:53 | in | cfd0f244: fixes to #78 for automated findings. |
| 11-11 02:16:29 | in | 7249c113: retry; #74 now at 42a55c7f. |
| 11-11 02:16:47 | in | 319b07cf: hold #78 until the integrated head is frozen. |
| 11-11 02:21:43 | out | bddbbf28: #74 at 42a55c7f, changes requested, three findings. |
| 11-11 02:25:56 | in | aaa95ec4: findings confirmed; repair plan proposed. |
| 11-11 02:26:17 | out | 8e87fe6d: repair plan agreed with pins. |
| 11-11 02:42:35 | in | 36e3a177: repair progress. |
| 11-11 02:4x | in/out | Permission prompt asking for another worker's Paseo id; answered that the reviewer does not know it and passed it to the owner. |
| 11-11 03:08:17 | in | 020dbe62: #74 repairs at 74917b9c; proposes `Profiles::validate_on`. |
| 11-11 03:09:03 | out | f65d224e: profile API adaptation agreed. |
| 11-11 03:11:09 | out | 7c971383: #74 approved once hosted gates are green. |
| 11-11 03:26:34 | in | 86ecdf14: hosted gates green for #74 at 74917b9c. |
| 11-11 03:33:47 | in | 613d1eae: #78 frozen at 57334e6c. |
| 11-11 03:36:25 | out | 44f64214: #78 code approved, not ready to merge. |
| 11-11 03:36:58 | in | 65c7993b: conflict confirmed; asks whether a branch merge is allowed. |
| 11-11 03:37:17 | out | 051992fd: reviewer's reading of "no merges"; decision left to the owner. |
| 11-11 03:4x | in | 6d0208ea: receipt times above. A second permission prompt (branch-only merge) was left for the owner. |

Counts checked by the reviewer with `paseo logs <id> --filter tools --no-color`, counting `[Send message]` events, on 11-11 after the last row: 14 from the reviewer and 23 from the author (the criteria ask for at least 4 and 6). All five design agreements (21:50 to 21:55 on 11-10) precede PR 1 (22:40).

## Design agreements

### 1. `crates/alignment` API (agreed with changes)

As proposed: standalone crate; `Element {start, direction, length, kind: Line | Arc{radius, turn} | Spiral{radius_start, radius_end, turn}}` with `None` = infinite radius; direction in radians counter-clockwise from +X; left turn = positive curvature; offset positive left; join tolerances 1e-6 units and 1e-8 rad; caps of 4096 elements, 100000 generated points, 16 MiB and depth 32 for XML.

Pinned by the reviewer:

1. Drop the stored `unit` unless proposal 4 shows a need; export can read the drawing's unit.
2. Station = start station + distance along; `at()` outside the inclusive range is an error.
3. `project()` beyond an end: clamped station plus flag, `distance` to the end point, `offset` = signed lateral component against the end tangent. Ties go to the lowest station.
4. `at(station, offset)`: point = p + offset·(−sin d, cos d); at a joint the later element answers.
5. Spiral: both radii infinite or equal radii are refused; curvature is linear in length; an S-curve is two elements.
6. Arc sweep ≥ 2π is refused; direction breaks are compared modulo 2π.
7. Caps are coded errors, never truncation.
8. Commands refuse an alignment with any joint issue, so a polyline with an angle point is refused. Possible owner follow-up; no option added.
9. LandXML import from rounded coordinates must not loosen the join tolerance; to be settled in proposal 4.

The author accepted all nine, including removing `unit`.

### 2. Clothoid evaluation (agreed with pins)

As proposed: integrate exp(i·θ(s)) with θ quadratic in s, in subintervals with max|k|·h ≤ 0.25 rad, each by a 16-term Taylor expansion integrated analytically. Spiral turning capped at 16π, length at 1e7, |XY| at 1e9. `project()` is a bounded global subdivision with the lower bound chord distance − k·h²/8, a 200000-node budget, and a `NUMERICAL_LIMIT` error rather than an uncertified answer. Tessellation uses the same k·h²/8 bound.

Reviewer's check: the phase change over an interval is at most 0.25 rad, so the tail is below e^0.25·0.25^17/17! per unit length; the interval count is at most L·kmax/0.25 ≤ 2·turning/0.25, about 400 at the cap.

Pins: integrate in the element's local frame and transform once; certify `project()` to a distance tolerance so ties end (lowest station) instead of exhausting the budget; expected spiral values come from the classic series in t = L/(2R), not from the code; the round-trip test includes a compound spiral and an exit spiral.

### 3. Storage (mechanism agreed, with changes)

As proposed: the definition is JSON in a `CADCRAFT_ALIGNMENT` XRECORD in the polyline's extension dictionary, in ASCII chunks of at most 240 bytes, with a version number and an 8 MiB cap; profiles share the payload. No custom class and no sidecar. A storage round-trip test in DXF and DWG comes before the commands that depend on it, and the author brings evidence back if the DWG bridge drops the record. Codes: `STALE_ALIGNMENT`, `INVALID_ALIGNMENT_DATA`.

Changes asked for:

1. No stored carrier snapshot. Staleness is one comparison: the carrier regenerated from the definition and chord tolerance against the current polyline (count, closed flag, bulges, coordinates at 1e-7, elevation 0, extrusion +Z). Version 1 pins the tessellation rule.
2. The polyline's handle is the identity; the name is a label.
3. All-or-nothing uses the existing command guard and undo, not a new clone path, unless shown insufficient.
4. One test states what COPY does with the data, and that MOVE gives `STALE_ALIGNMENT`.
5. Foreign entries in the same extension dictionary are left untouched on rewrite, with a test.

Shared-file edits agreed in this message: one line in `crates/engine/Cargo.toml` and the crate's registration in `xtask/src/layers.rs`.

### 4. LandXML mapping (agreed with changes)

As proposed: LandXML 1.2 namespace, 16 MiB and depth 32 limits, DTD and entities refused. Unsupported geometry refuses that alignment only; malformed XML refuses the file. Points are "N E", mapped to [E, N]. Units are required (meter, foot, USSurveyFoot) and scaled to the drawing's unit. Elements are derived from their points (`Line` Start/End; `Curve` Start/Center/End and `rot`; `Spiral` clothoid with `length`, `radiusStart`/`radiusEnd`, `rot`, Start/End and PI or `dirStart`), chained, and every stated value is compared with the result. Station equations give a `STATION_EQUATIONS_IGNORED` warning. Export writes exact arcs and spirals with shortest round-trip decimals and refuses duplicate alignment names. Codes: `GEOMETRY_MISMATCH`, `UNSUPPORTED_UNITS`, `UNSUPPORTED_VERTICAL_CURVE`, `PROFILES_NOT_SUPPORTED` (PR 1 only).

Changes asked for:

1. The strict 1e-6 / 1e-8 rad refusal stays, and its message gives the index and the largest gap and direction break. Tolerant import is issue #69.
2. A unitless drawing follows what the existing surface LandXML code does; no new unit parameter.
3. No `Feature` property for the station format; import uses the default format.
4. The schema's wording for `direction` and `angle` is quoted where the conversion is applied. Points are preferred over `dirStart`, which is verified against them.
5. Arc sweep comes from the three points and `rot`; coincident Start and End is refused.
6. `INF` is accepted exactly as the schema spells it; a missing `spiType` is refused unless the schema gives a default.
7. In PR 1 a file with profiles imports its horizontal geometry with a `PROFILES_NOT_SUPPORTED` warning.
8. Hostile tests: truncated, 33-deep, DTD or entity, a million `Line` children, non-finite coordinates, missing Units.

### 5. Profile sampling and view (agreed with changes)

As proposed: host-free `sample(alignment, vertices, triangles, elev_tol)`; every triangle edge is intersected with the true curve by splitting at tangent-parallel stations and bisecting; the midpoint of each interval between sorted crossings picks the triangle, and an interval in no triangle ends the segment. Covered intervals are subdivided until |(a,b)|·kmax·h²/8 is within the elevation tolerance. Design profile: z = zPVC + g1·x + (g2 − g1)·x²/(2L). View layers `PROFILE-GRID`, `PROFILE-TEXT`, `PROFILE-GROUND`, `PROFILE-DESIGN`; a 1/2/5·10^n grid with at most about ten divisions per axis; exaggeration default 1.

Reviewer's check: along the curve z'' = (a,b)·C'' and |C''| = |k|, so the linear-in-station error between samples h apart is at most |(a,b)|·kmax·h²/8. With every edge crossing a breakpoint, an interval cannot change triangle, so the midpoint test is sound.

Changes asked for:

1. The view does not tag or replace an earlier view. Each run draws a new figure in one undo step and deletes nothing.
2. Crossings count only within the edge segment and are deduplicated at shared vertices.
3. A bounding-box reject comes before any root solve; the work cap is for hostile input, not for a real TIN.
4. Degenerate triangles are skipped or refused with a code.
5. Grade is a ratio. Curves may abut but not overlap. Outside the PVI range is an error. At an uncurved PVI the later tangent answers.
6. `profileview` refuses with a code when the alignment has neither a ground nor a design profile.
7. The vertical known-answer tests show their working.

## Issues filed

- [#69](https://github.com/0bnoxide/cadcraft/issues/69): LandXML files with rounded coordinates are refused on import.
- [#83](https://github.com/0bnoxide/cadcraft/issues/83): non-blocking review notes from #74.
- [#85](https://github.com/0bnoxide/cadcraft/issues/85): non-blocking review notes from #78.
- [#70](https://github.com/0bnoxide/cadcraft/issues/70): MOVE or an angle point makes an alignment unusable.

## Reviews

### #74 Horizontal alignments, head 42a55c7f: changes requested

Review: https://github.com/0bnoxide/cadcraft/pull/74 (posted as a comment review).

What ran: `cargo test -p cadcraft-alignment`, 19 passed. Not run locally: `cargo xtask ci`, the engine tests, the CLI flow and the native UI; the hosted Linux and Windows gates were green for this head.

Hand calculations, independent of the code:

- Entry spiral L=60, R=300, t=L/(2R)=0.1: x = 60(1 − t²/10 + t⁴/216 − t⁶/9360) = 59.94002777, y = 60(t/3 − t³/42 + t⁵/1320 − t⁷/75600) = 1.99857188.
- With the alignment's spiral starting at (10, 0): arc centre = (69.94002777 − 300·sin 0.1, 1.99857188 + 300·cos 0.1) = (39.99000278, 300.49982147).
- Arc end = centre + 300·(sin 0.3, −cos 0.3) = (128.64606478, 13.89887473).
- Exit spiral displacement = e^{0.4i}·conj(59.94002777 + 1.99857188i) = (55.98670211, 21.50093965), so the end is (184.63276689, 35.39981438).
- Line, arc, line: arc midpoint (10 + 10/√2, 10 − 10/√2), total 20 + 5π.

All match the tests' expected values.

Findings (must fix):

1. `project()` beside a spiral returns a station wrong by up to about 7e-4 (measured with a scratch program outside the repository: offset 1 gives 2e-5, offset 50 gives 2.7e-4, offset −1000 gives 6.8e-4 on the L=60, R=300 spiral). Distance and offset are right. The agreed refinement of the stationary point is missing.
2. Every query revalidates the whole alignment. One `at()` on a valid alignment of 4096 tight spirals took 1.96 s in a release build, and `alignmentreport` accepts 4096 stations.
3. Arcs are drawn as chords, where the brief asks for chords on spirals only. Storage version 1 pins the rule.

### #74 re-review, head 74917b9c: approved

Full delta from 42a55c7f read. Ran `cargo test -p cadcraft-alignment` (22 passed) and `cargo test -p cadcraft-engine --lib alignment` (7 passed). Hosted Linux and Windows gates are green for this head. Not run locally: `cargo xtask ci`, the CLI flow, the native UI.

1. Spiral projection fixed: the same scratch measurement gives a station error of about 1.5e-10 at offsets 1, 50, 100, −200, −1000 and 250.
2. One validation per command through `ValidatedAlignment`. The raw one-shot methods still validate each call (0.68 s for one `at()` on the 4096 tight spiral case), so loops must use the view.
3. One bulge vertex per arc. Hand check: 270° gives tan 67.5° = 1 + √2.

### #78 Profiles, head 57334e6c: code approved, not ready to merge

Review: https://github.com/0bnoxide/cadcraft/pull/78 (posted as a comment review).

Before merge: GitHub reported the PR as conflicting with its base `slice11/codex-horizontal`, and no hosted Linux or Windows gate result existed for this head.

What ran: `cargo test -p cadcraft-alignment` (40 passed) and the engine alignment and profile command tests (8 passed). Not run locally: `cargo xtask ci`, the CLI flow, the native UI; no rendered profile view was inspected.

Hand calculations, independent of the code:

- Crest, PVIs (0, 0), (200, 6), (400, 2), L = 200: g1 = 0.03, g2 = −0.02, PVC (100, 3), z = 3 + 0.03x − 0.000125x². Stations 100, 150, 200, 250, 300 give z = 3, 4.1875, 4.75, 4.6875, 4 and grades 0.03, 0.0175, 0.005, −0.0075, −0.02. High point x = 0.03·200/0.05 = 120: station 220, z = 4.8.
- Sag, middle PVI (200, −4): g1 = −0.02, g2 = 0.03, PVC (100, −2). z = −2, −2.6875, −2.75, −2.1875, −1. Low point x = 80: station 180, z = −2.8.
- TIN crossings: the test's four vertices lie on z = x + 2y. The line y = 5 from x = −5 gives (station, z) = (5, 10), (10, 15), (15, 20).
- Quarter circle on z = x: midpoint cos 45° = 0.7071.

All match the tests' expected values.

Non-blocking notes: #85.

## Not verified by the reviewer

- `cargo xtask ci` was never run locally for either PR; the hosted gates were relied on for #74, and none had run for #78 at 57334e6c.
- The CLI and control-channel pass criteria and the native UI were not exercised by the reviewer.
