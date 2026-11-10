# Slice 11: alignments and profiles (Codex codes, Claude reviews)

Same Paseo setup as slices 7 to 10. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice11]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews each of Codex's PRs in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it. **Claude posts its review on each PR and pushes its notes PR before the slice is called done**; slice 10 merged without either.

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## What this slice is

The first piece of the civil object model. It is **new design work, not a port**: no reference repository holds this behaviour. The sources are public only:

- the LandXML 1.2 schema (`Alignments`, `Alignment`, `CoordGeom` with `Line`, `Curve`, `Spiral`; `Profile`, `ProfAlign` with `PVI`, `ParaCurve`; `ProfSurf` with `PntList2D`), and
- standard route geometry: tangents, circular arcs, clothoid transition spirals, and symmetric parabolic vertical curves.

Clean-room rules apply in full. Do not read CIM-Project-Suite-REVENG. Do not read the sources of any GPL, LGPL or AGPL project. Nothing from inside an Autodesk product is read or copied; observing the installed application black-box with synthetic drawings is allowed as `AGENTS.md` describes.

Later slices (plan and profile sheets, grading, pipes, corridors) build on these types, so the host-free API matters more than the commands.

## Scope

These are ordinary drawing commands, like `tincreate`: JSON in, undoable, no plan and approval flow. Everything host-free goes in a new `crates/alignment` crate that depends on no other workspace crate except `geom` if it helps (check `cargo xtask layers`). The engine commands stay thin.

1. **Horizontal alignment.** An ordered list of elements: line, circular arc, clothoid spiral (entry, exit, and between two arcs). Each element knows its start point, start direction, length and, where it applies, radius or the radii at each end and the turn direction. Required functions, all returning errors for out-of-range input:
   - station to point and direction;
   - point to station and offset (nearest element, with the offset signed left or right, and a clear answer for a point beyond either end);
   - total length, and a polyline approximation within a given chord tolerance.
   Elements must join: a gap or a direction break above a small tolerance is reported per joint, never silently repaired.
2. **Stationing.** A start station (any finite value, negative allowed) and a station format for display: `0+000.000` or `0+00.00`. Station equations are out of this slice (see Owner decisions).
3. **Vertical alignment (design profile).** An ordered list of PVIs (station, elevation), each with an optional symmetric parabolic curve length. Required functions: elevation and grade at a station; the high or low point of each curve; validation that curves don't overlap and that stations increase and lie within the horizontal alignment.
4. **Existing-ground profile.** Sample a TIN (a polyface mesh selected by handle, through the existing TIN conversion in `surface_build.rs`) along an alignment: every triangle-edge crossing of the true curve plus the element ends. On a line that is exact for the TIN. On an arc or a spiral, elevation inside one triangle is not linear in station, so add intermediate samples until a straight line between neighbouring samples stays within a given elevation tolerance of the TIN. Gaps where the alignment leaves the TIN are kept as gaps, never bridged.
5. **Storage in the drawing.** The drawing is the store. An alignment is drawn as one polyline (spirals as chords within the tolerance) and carries its exact definition as data attached to that object; the exact definition is authoritative. The data must survive DXF and DWG save and reopen. If the polyline's geometry no longer matches its definition (someone moved a grip), commands refuse it with a clear code and do not guess. Profiles attach to their alignment the same way. Propose the mechanism after reading how existing commands persist data (`callout_override.rs`, entity xdata, `foreign.rs`) and what `docs/gaps.md` row FF1 says DWG loses; prove the round trip with tests in both formats.
6. **LandXML.** Import and export `Alignments` with `CoordGeom` and `Profile`/`ProfAlign`, reusing the workspace's `quick-xml` and the bounded, hostile-input parsing style of `crates/handoff/src/landxml.rs`. Do not touch `crates/handoff`: contract v1 is frozen. Export writes clothoid spirals with their LandXML attributes; import refuses, per alignment and with a code, any spiral type other than clothoid, any vertical curve other than a symmetric parabola, and any element it can't represent exactly. A file with station equations imports with a warning naming them. An exported file must reimport to the same definition within 1e-6 drawing units.
7. **Profile view.** Draw a profile for an alignment at a picked point: a grid with station and elevation labels, the existing-ground line, the design line with its curves, and a vertical exaggeration. Look at `crates/subsurface/src/profile.rs` and its drawing code first and share what fits. No label styles, bands or dynamic update: rerunning the command redraws it.

Commands (Codex proposes final ids and labels; these are the expected set):

| Command | Does |
|---|---|
| `alignmentcreate` | Makes an alignment from a selected polyline of lines and arcs, or from a JSON element list (the only way to give spirals), with a name and start station. |
| `alignmentimport`, `alignmentexport` | LandXML in and out, alignments with their profiles. |
| `alignmentreport` | Reports the element table, and the station and offset of given points or the point at given stations and offsets. Draws nothing. |
| `profilesample` | Existing-ground profile from a TIN. |
| `profilecreate` | Design profile from a PVI list. |
| `profileview` | Draws the profile view. |

Out of this slice: station equations, superelevation, cross sections, corridors, alignment labels and label styles, offset alignments, design-criteria checks, asymmetric and circular vertical curves, spirals other than clothoid, coordinate systems (the fork has no PROJ), and interactive grip editing of alignment geometry.

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice11/codex-horizontal`, then `slice11/codex-vertical` stacked on it | New `crates/alignment` with its tests; new engine command files with their registration in `cmd.rs`; `zh.tsv` rows for new labels; `Cargo.toml`/`Cargo.lock` only for the new crate; `docs/agents/slice-11-notes-codex.md` | PR 1: items 1, 2, 5 and the horizontal half of 6, with `alignmentcreate`, `alignmentimport`, `alignmentexport`, `alignmentreport`. PR 2: items 3, 4, 7 and the profile half of 6, with the three profile commands. |
| Claude (review) | `slice11/claude-review` | `docs/agents/slice-11-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews each PR in full, runs its tests, checks the geometry against independent hand calculations, posts findings on the PR, and opens its notes PR. |

No new dependency is expected. If one is proposed, its licence must be in the `AGENTS.md` allowlist and Claude must agree it by message first.

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **The `crates/alignment` API.** Element types and their fields, the direction and sign conventions (angles, left and right, turn direction), tolerances, and the error type. This is the interface later slices depend on, so settle it first.
2. **Clothoid evaluation.** The method (series or other), its accuracy target and the range of spiral angles it holds over, and how point-to-station is solved on a spiral.
3. **Storage.** How the definition is attached to the polyline, how staleness is detected, and the DXF and DWG round-trip evidence.
4. **LandXML mapping.** Each supported element and attribute, the refusal codes, and how units and the station start are read and written.
5. **Profile sampling and view.** The crossing algorithm and its gap handling, and the profile view's layers, grid rule and exaggeration.

## Owner decisions

The recommended answers are written into the scope above; the owner confirms or changes them before the agents start.

1. Clothoid is the only spiral type. Other types are refused on import with a code.
2. Vertical curves are symmetric parabolas only.
3. Station equations are out of this slice. A LandXML file that has them imports with a warning and plain stationing.
4. An alignment is a polyline plus its exact definition stored on that object, and the definition wins. A polyline edited by hand is refused, not re-read.
5. The profile view is a plain redrawn figure: no label styles, bands or live update.
6. No reference repository is used. The sources are the public LandXML 1.2 schema and standard route geometry.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 4 from Claude and at least 6 from Codex.
- [ ] All five items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branches. Claude's branch changes only its notes file. Each Codex PR carries a posted review with a verdict, and the notes PR is open, before anyone merges.
- [ ] Known-answer tests, with the expected values computed independently of the code under test and the working shown in a comment:
  - a line, arc, line alignment: points at five stations, and total length;
  - a line, spiral, arc, spiral, line alignment: the spiral end points and the arc centre, to 1e-6;
  - station to point to station returns the same station within 1e-6 on every element type, including points on a spiral;
  - a crest and a sag curve: elevation and grade at five stations each, and the high and low points.
- [ ] Hostile input does not panic: zero and negative lengths and radii, non-finite numbers, a million-element list, deeply nested and truncated LandXML. Each is refused with an error, and allocations and loop counts are bounded.
- [ ] Through the CLI or control channel:
  - `alignmentcreate` from a drawn polyline with one arc, then `alignmentreport`, gives the expected element table and the station and offset of a given point;
  - `alignmentexport` then `alignmentimport` into a new drawing gives the same definition within 1e-6;
  - the alignment survives save and reopen as DXF and as DWG, and a grip-edited polyline is refused;
  - `profilesample` over a TIN made by `tincreate` returns a point at every triangle-edge crossing, and a gap where the alignment leaves the TIN;
  - on a quarter-circle of radius 1 lying inside one triangle of the plane z = x, the sampled ground line at the arc's midpoint is within the tolerance of 0.7071, not the 0.5 that end-to-end interpolation gives;
  - `profilecreate` then `profileview` draws the grid, the ground line and the design line, and undo removes them.
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, including the wasm and layers gates, and each PR merges without anyone fixing a conflict by hand. Merge Codex's PRs in order, then Claude's notes.
