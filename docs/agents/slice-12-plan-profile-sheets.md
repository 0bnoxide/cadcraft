# Slice 12: plan and profile sheets (Codex codes, Claude reviews)

Same Paseo setup as slices 7 to 11. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice12]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews each of Codex's PRs in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it. **Claude posts its review with a verdict on each PR and pushes its notes PR before the slice is called done.**

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## What this slice is

The Sheet Generator: plan and profile sheets along an alignment, and site sheets from placed view frames. It is the first consumer of the slice 11 alignment model and builds on what already exists, adding no second copy of any of it:

- `crates/alignment` (station to point and direction, profiles, `view::draw`) and its drawing store in `crates/engine/src/cmd/alignment_store.rs`;
- layouts, viewports with scale and twist, and DWG/DXF writing (`crates/engine/src/cmd/layout.rs`, `cmd/layout/viewports.rs`);
- the `{{Name}}` placeholder replacement of `figureexport` and the staged, no-replace publish of slices 9 and 10 (`crates/proposal`).

**Behaviour reference, read only:** the CIM-Project-Suite-REVENG repository (path in the kickoff): `docs/phase2/features/sheet_generator.md`, `docs/phase2/formats/sheet_generator_external_stores.md`, the Sheet Generator rows of `docs/phase2/implementation_requirements.md` and `docs/phase2/unknowns.md`, and `tests/behavioral/cases/BT-P0-SG-*.md`. Match its stages, inputs, settings, ownership and refusal behaviour. Where it records a behaviour as unknown (exact rounding, collision prompts, cross-store rollback), this brief says what to do. Port behaviour, not the Civil 3D API, and copy none of its quoted message text: write our own wording. Other sources are standard plan and profile sheet practice and the public DXF Reference. Do not read the sources of any GPL, LGPL or AGPL project. Nothing from inside an Autodesk product is read or copied.

## Scope

Drawing commands: JSON in, one undo step for every change to the current drawing, no plan and approval flow. Programmatic calls never prompt; where the reference asks the user to confirm, the JSON form needs an explicit flag and is refused without it. Host-free geometry (frame tiling, station ranges, profile view ranges) goes in `crates/alignment` or a small new module beside it; the engine commands stay thin. The stages run in the reference's order: view frames, profile views, adjust, create sheets, update sheets.

1. **Road view frames** (reference `VIEWSHAPES`). Input: an alignment (handle), a station range (default the whole alignment), the plan viewport's size on paper, a scale, and these reference settings:
   - **spacing:** a fixed distance between matchlines, or automatic (the longest that keeps the frame's piece of alignment inside the frame);
   - **station rounding:** interior matchline stations are rounded down to a multiple of a given increment;
   - **vertex buffer:** a matchline is moved back so that it is not within the buffer of an element joint;
   - **offset:** the frame is shifted left or right of the alignment by a given distance.
   Each frame is a rectangle of the viewport's size times the scale, centred on the chord from its start to its end station (plus the offset) and turned to that chord. A piece of alignment that leaves its frame is refused with a code naming the station, never clipped silently. Frames are drawn in model space as closed polylines; matchlines are inserts of a matchline block named in the input (a plain line and label when none is named), with the station written to a named attribute.
2. **Site view frames** (reference `SITEVIEWSHAPES`). The user places view-frame block inserts in model space (the reference ships these as site view blocks); the command turns the selected inserts into a frame group, each frame taking the insert's position, rotation and the block's rectangle times the scale. No alignment is needed.
3. **Profile views per frame** (reference `PROFILEVIEWSHAPES` and `ADJUSTVIEWS`). For each road frame, a profile view whose station range matches the frame's, extended by start and end buffers. Elevation rule, as the reference's settings: a fixed view height, with the datum chosen to centre a selected profile (ground or design) on its highest, median or lowest elevation in the range, and rounded to a given increment. `profileview` (slice 11) gains the station range and elevation window this needs. Fix #85 item 5 here: vertical grid lines fall on round stations, not on steps from the start station.
4. **Create sheets** (reference `CREATELAYOUTS`, `CREATESITELAYOUTS`). Input: a frame group, a template drawing (DWG or DXF) and the layout in it to copy, the arrangement, name templates, numbering, and the storage mode.
   - **Arrangements:** plan and profile, plan and plan, profile and profile, plan only, profile only (site groups: plan only, and plan and plan). Two-up arrangements take consecutive frames. The template names its viewports by role; which viewport is which is never guessed from size or position. A template that doesn't match the arrangement is refused before any change.
   - Each plan viewport's view is set to its frame (centre, twist, scale); each profile viewport shows its frame's profile view at the same horizontal scale. A north arrow block named in the input is rotated to the plan viewport's twist.
   - **Names and numbers:** a layout name template and a drawing name template with `{n}` and the placeholders below, numbering start and increment, and a number style (plain or zero-padded to a width).
   - **Storage:** `current` puts the layouts in the current drawing. `drawings` writes new sheet drawings with a given number of layouts each, in a given output folder, as DWG. Each sheet drawing carries copies of what its viewports show, so the reference's two confirmations about duplicating civil objects become one required flag, `duplicate_objects: true`, without which `drawings` is refused. Sheet drawings are staged and published with the slices 9 and 10 no-replace move: an existing file is never overwritten, the run reports every file it wrote, and a failure leaves no partial set.
   - Placeholders in each sheet's TEXT, MTEXT and block attributes are replaced through the `figureexport` code: `{{SheetNumber}}`, `{{SheetCount}}`, `{{StationStart}}`, `{{StationEnd}}`, `{{Alignment}}`. An unknown placeholder is left and reported.
   - Ownership (frame group, frame index, and for `drawings` the file and layout) is stored with the alignment store's mechanism on the frames, layouts and profile views it makes.
   - Refuse before any change: a drawing with no alignment for a road group (own wording, same condition as the reference), a missing template or layout, a missing named block or attribute, a name that collides with a layout or file the command does not own.
5. **Update sheets** (reference `UPDATELAYOUTS`, `UPDATESITELAYOUTS`). Frames may be moved or rotated by hand, as in the reference; the update reads each owned frame's current geometry (and, for road frames, re-derives its stations), redraws owned profile views and matchlines, re-sets views and re-replaces placeholders, adds sheets for new frames and removes owned sheets for frames that are gone. It never changes a layout, drawing or object it does not own. For `drawings` it writes new versions with the no-replace move and reports what it superseded.
6. **Follow-ups ridden along:** #87 (COPY keeps a stale existing-ground profile) and #85 item 5 (in item 3). Close #87 from the PR; comment on #85 that item 5 is done and leave the rest open.

Commands (Codex proposes final ids and labels; these are the expected set):

| Command | Reference | Does |
|---|---|---|
| `viewframecreate` | `VIEWSHAPES` | Item 1. |
| `siteviewframes` | `SITEVIEWSHAPES` | Item 2. |
| `profileviewframes` | `PROFILEVIEWSHAPES`, `ADJUSTVIEWS` | Item 3. |
| `sheetcreate` | `CREATELAYOUTS`, `CREATESITELAYOUTS` | Item 4, road or site by the group. |
| `sheetupdate` | `UPDATELAYOUTS`, `UPDATESITELAYOUTS` | Item 5. |

Not matched, with the reason:

- **Sheet set (`.dst`) files.** An Autodesk Sheet Set Manager database with no public specification, written in the reference only through Autodesk's COM API (its own `IR-DATA-003`). The reference's plan puts it last for the same reason. A plain JSON sheet index in the output folder is out too unless the owner asks for one.
- **`MOVEONPROFILEVIEWS`, `UPDATEPROFILEVIEWPARTS`, pipe and pressure profiles in views.** There is no pipe network until slice 14; these go there.
- **Data references and xref-overlay alignments.** The fork has no data references; a road group needs an alignment in the current drawing.
- **Plotting.** The reference doesn't plot either; the existing PUBLISH and Plot plot the sheets.
- **Remembered form settings.** No dialogs; inputs are JSON.

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice12/codex-frames`, then `slice12/codex-sheets`, then `slice12/codex-drawings`, each stacked on the one before | `crates/alignment`; new engine command files with their registration in `cmd.rs`; `profile_cmd.rs` and `alignment_store.rs`; `figureexport.rs` and `crates/proposal` only to share the placeholder and publish code; `zh.tsv` rows for new labels; `docs/agents/slice-12-notes-codex.md` | PR 1: items 1 to 3 and #87. PR 2: items 4 and 5 with `storage: current`. PR 3: `storage: drawings`. |
| Claude (review) | `slice12/claude-review` | `docs/agents/slice-12-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews each PR in full, runs its tests, checks the frame geometry against independent hand calculations and the behaviour against the reference, posts its review with a verdict on the PR, and opens its notes PR. |

No new dependency is expected. If one is proposed, its licence must be in the `AGENTS.md` allowlist and Claude must agree it by message first.

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Frame tiling.** Fixed and automatic spacing, rounding, the vertex buffer, the offset, how they interact at the range ends, and the refusal codes.
2. **Frame and sheet storage.** What is stored on which object, how ownership is checked, how a hand-moved frame is re-read, and the DXF and DWG round-trip evidence.
3. **Template contract.** How a template names viewport roles, the matchline, north arrow and site view blocks with their attributes, and how a layout is copied from another drawing.
4. **Profile view range.** The buffers, the fixed-height datum rule, rounding, and placement in model space.
5. **Sheet drawings.** What each sheet drawing contains, the name and collision rules, staging and publish, and the run report.

## Owner decisions

The recommended answers are written into the scope above; the owner confirms or changes them before the agents start.

1. Both storage modes: layouts in the current drawing, and new sheet drawings with a set number of layouts each.
2. All five arrangements, plus the two site ones.
3. Fixed or automatic spacing, rounding, vertex buffer and offset, as the reference's settings. Frames can be moved by hand; the update re-reads them.
4. REVENG is the behaviour reference (owner, 2026-11-12: it is clean-room, so no licence check applies). This brief settles what it leaves unknown.
5. Not matched: sheet set files, pipe parts in profile views (slice 14), data references, plotting.
6. Only #87 and #85 item 5 ride along.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 5 from Claude and at least 8 from Codex.
- [ ] All five items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branches. Claude's branch changes only its notes file. Each Codex PR carries a posted review with a verdict, and the notes PR is open, before anyone merges.
- [ ] Known-answer tests, with the expected values computed independently of the code under test and the working shown in a comment:
  - a straight 1,000-unit alignment, fixed spacing 250, increment 10: four frames, matchlines at 250, 500 and 750, centres and twist exact; the same with a 5-unit offset;
  - a line, arc, line alignment with automatic spacing: each frame's stations, centre and direction, and every frame contains its piece of alignment;
  - a matchline that would fall within the vertex buffer of a joint is moved back by the expected amount;
  - a profile view for stations 250 to 500 with a fixed height centred on the median ground elevation, and its rounded datum;
  - a start station of 1234.5 gives profile grid lines on round stations.
- [ ] Hostile input does not panic: zero, negative and non-finite sizes, scales, spacings and increments, an increment far smaller than the alignment (frame count is bounded), a corrupted stored frame group, and a hostile template drawing. Each is refused with an error.
- [ ] Through the CLI or control channel:
  - road frames, profile views, then `sheetcreate` in plan and profile with `storage: current` gives one layout per frame with the views, twists, north arrow and replaced placeholders expected, and undo removes them all;
  - plan and plan takes consecutive frames; a site group from three placed view-frame inserts gives three plan-only sheets;
  - `storage: drawings` with two layouts per drawing writes the expected files, refuses without `duplicate_objects`, and a second run overwrites nothing;
  - frames and sheets survive save and reopen as DXF and as DWG; a frame moved by hand is followed by `sheetupdate`;
  - changing the frames, then `sheetupdate`, adds or removes owned sheets and leaves an unowned layout untouched;
  - a name that collides with an unowned layout or file is refused and nothing changes;
  - a drawing without an alignment is refused for a road group, and the input file is unchanged;
  - COPY of an alignment with both profiles drops the ground profile on the copy (#87).
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, including the wasm and layers gates, and each PR merges without anyone fixing a conflict by hand. Merge Codex's PRs in order, then Claude's notes.
