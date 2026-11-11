# Slice 11 reviewer notes

Review-only role: design agreement by message, a full review of each code PR, no code. Brief: `slice-11-alignments-profiles.md`.

## Message log

Times are local (UTC-7), 2026-11-10 unless a date is given.

| Time | Direction | Summary |
|---|---|---|
| 21:46 | out | Sent reviewer id; asked for the five design proposals, `crates/alignment` API first. |
| 21:47 | in | Author acknowledged; reading the brief, proposals to follow. |
| 21:5x | in | Proposal 1 (API): element types, conventions, tolerances, caps, error type. |
| 21:5x | out | Proposal 1 agreed with nine pins (below). Asked for proposal 2. |
| ~22:00 | in | Pins for 1 accepted. Proposal 2 (clothoid): piecewise Taylor integration, bounded projection search. |
| ~22:00 | out | Proposal 2 agreed with four pins. Asked for proposal 3. |
| ~22:10 | in | Pins for 2 accepted. Proposal 3 (storage): extension dictionary XRECORD with chunked JSON, staleness check. |
| ~22:10 | out | Proposal 3 mechanism agreed with five changes; agreed the `engine/Cargo.toml` and `xtask/src/layers.rs` lines. Asked for proposal 4. |

| ~22:20 | in | Pins for 3 accepted. Proposal 4 (LandXML mapping, units, refusal codes). |
| ~22:20 | out | Proposal 4 agreed with eight changes. Filed #69 and #70. Asked for proposal 5. |
| ~22:30 | in | Proposal 5 (ground sampling, vertical design, profile view). |
| ~22:30 | out | Proposal 5 agreed with seven changes. All five agreed; PR 1 may start. |
| 22:3x | in | Unit question answered: reuse `tinexport`'s `unit` parameter (default meter). |
| 22:3x | out | Agreed the `unit` parameter. |
| 22:33 on | in | Proposal 5 pins accepted; schema reached over HTTP; issues #68, #72, #73 filed by the author. |
| 11-11 | in | PR 1 up (#74, head 000afb94); PR 2 up (#78); later heads 42a55c7f (#74) and 455bfd0f (#78); #78 to be re-based by the owner, review held until the head is frozen. |
| 11-11 | out | Review of #74 at 42a55c7f posted: changes requested, three must-fix findings. |
| 11-11 | in | Author confirmed the three findings and proposed repairs: stationary-point refinement, a validated view, bulge vertices for arcs. |
| 11-11 | out | Repair plan agreed with pins (1e-7 station tests, test-only counter, bulge tolerance 1e-9). |
| 11-11 | in | The author asked for another worker's Paseo id through a permission prompt. Answered that the reviewer does not know it; passed the question to the owner. |
| 11-11 | in | #74 repairs published at 74917b9c; proposed `Profiles::validate_on(&ValidatedAlignment)` for #78. |
| 11-11 | out | Agreed the profile API adaptation. Updated verdict on #74 posted: approved once hosted gates are green. |
| 11-11 | in | Hosted Linux and Windows gates green for #74 at 74917b9c. |

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

### #78 Profiles

Held until the author sends the frozen head after the owner's re-base onto the updated #74.
