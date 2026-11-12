# Slice 12: plan and profile sheets (Codex codes, Claude reviews)

Same Paseo setup as slices 7 to 11. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice12]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews each of Codex's PRs in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it. **Claude posts its review with a verdict on each PR and pushes its notes PR before the slice is called done.**

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## What this slice is

Plan and profile sheets along an alignment: the first consumer of the slice 11 alignment model. It builds on three things that already exist and adds no second copy of any of them:

- `crates/alignment` (station to point and direction, profiles, `view::draw`) and its drawing store in `crates/engine/src/cmd/alignment_store.rs`;
- layouts, viewports with scale and twist, and plotting (`crates/engine/src/cmd/layout.rs`, `cmd/layout/viewports.rs`);
- the `{{Name}}` placeholder replacement of `figureexport` (slice 10).

The behaviour below is the specification: frames tiled along an alignment with matchlines, then one layout per frame made from a template layout, each with a plan viewport and optionally a profile viewport, and an update that touches only what the command made. Where it is silent, use the behaviour reference, read only: the CIM-Project-Suite-REVENG repository (path in the kickoff), `docs/phase2/features/sheet_generator.md` and `tests/behavioral/cases/BT-P0-SG-*.md`. It is a clean-room behaviour record; port the behaviour, not the Civil 3D API, and where it and this brief differ, the brief wins. Other sources are standard plan and profile sheet practice and the public DXF Reference. Do not read the sources of any GPL, LGPL or AGPL project. Nothing from inside an Autodesk product is read or copied; observing the installed application black-box with synthetic drawings is allowed as `AGENTS.md` describes.

## Scope

Ordinary drawing commands: JSON in, one undo step each, no plan and approval flow. Host-free geometry (frame tiling, fit checks, station ranges) goes in `crates/alignment` or a new small module beside it; the engine commands stay thin.

1. **View frames.** Input: an alignment (handle), a station range (default the whole alignment), the plan viewport's size on paper, a scale, and a matchline rounding increment.
   - Frames tile the range in order. Each frame covers stations `[s_i, s_i+1]`; the frame is a rectangle of the viewport's size times the scale, centred on the midpoint of the chord from `s_i` to `s_i+1` and turned to that chord's direction.
   - Interior matchline stations are multiples of the rounding increment. A frame's station length is the longest multiple of the increment whose piece of alignment (with a small margin) lies inside the frame. If not even one increment fits, refuse with a code naming the station. The last frame ends at the range end.
   - Drawn in model space: each frame as a closed polyline, and a matchline across the frame at each interior station, labelled with the station in the alignment's station format.
   - The frame group (alignment handle, range, size, scale, increment, and each frame's stations, centre and direction) is stored on the frame objects with the alignment store's mechanism. A frame moved or edited by hand no longer matches its stored definition and is refused with a clear code; rerunning the command replaces the group in one undo step.
2. **Profile view by station range.** `profileview` (slice 11) draws the whole alignment. Add a station range so a sheet can show only its frame's stations, with the elevation range taken from the profiles inside that range. Fix #85 item 5 here: vertical grid lines fall on round stations, not on steps from the start station.
3. **Sheets.** Input: a frame group, a template layout in the current drawing, a layout name pattern, and the arrangement (`plan` or `plan_profile`).
   - The template layout holds the title block and one viewport, or two for `plan_profile`. Which viewport is the plan and which is the profile is named by the template (Codex proposes how), never guessed from size or position. A template that doesn't match the arrangement is refused before any change.
   - One layout per frame, copied from the template. The plan viewport's view is set to the frame: its centre, the frame's direction as twist, and the scale. For `plan_profile`, a profile view is drawn for the frame's station range (item 2) and the profile viewport shows it at the same horizontal scale.
   - Placeholders in the new layout's TEXT, MTEXT and block attributes are replaced through the `figureexport` code: `{{SheetNumber}}`, `{{SheetCount}}`, `{{StationStart}}`, `{{StationEnd}}`, `{{Alignment}}`. An unknown placeholder is left as it is and reported.
   - Each made layout and profile view carries its ownership (frame group and frame index) with the same store.
   - Refuse, before changing anything: a drawing with no alignment, a stale frame group, a missing template, a name that collides with a layout the command does not own.
4. **Update.** After the frames are rerun, the update brings the owned sheets in line: re-sets views, redraws owned profile views, re-replaces placeholders, adds layouts for new frames and deletes owned layouts for frames that are gone. It never changes a layout or object it does not own. One undo step.
5. **Follow-ups ridden along:** #87 (COPY keeps a stale existing-ground profile) and #85 item 5 (in item 2). Close #87 from the PR; comment on #85 that item 5 is done and leave the rest open.

Commands (Codex proposes final ids and labels; these are the expected set):

| Command | Does |
|---|---|
| `viewframecreate` | Item 1. Makes or replaces a frame group with its matchlines. |
| `profileview` | Item 2. Gains an optional station range. |
| `sheetcreate` | Item 3. Makes the sheet layouts from a frame group and a template. |
| `sheetupdate` | Item 4. Updates the owned sheets of a frame group. |

Out of this slice: sheets in separate drawings, sheet set (`.dst`) files, plotting (the existing PUBLISH and Plot already plot layouts), arrangements other than `plan` and `plan_profile`, frames along site geometry without an alignment, sliding or resizing frames by hand, north arrows and match-line blocks from a library, pipe networks in profile views, and every other open alignment issue (#69, #70, #73 and its PR #82, #75, #83).

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice12/codex-frames`, then `slice12/codex-sheets` stacked on it | `crates/alignment`; new engine command files with their registration in `cmd.rs`; `profile_cmd.rs` and `alignment_store.rs`; `figureexport.rs` only to share the placeholder code; `zh.tsv` rows for new labels; `docs/agents/slice-12-notes-codex.md` | PR 1: items 1, 2 and #87. PR 2: items 3 and 4. |
| Claude (review) | `slice12/claude-review` | `docs/agents/slice-12-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews each PR in full, runs its tests, checks the frame geometry against independent hand calculations, posts its review with a verdict on the PR, and opens its notes PR. |

No new dependency is expected. If one is proposed, its licence must be in the `AGENTS.md` allowlist and Claude must agree it by message first.

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Frame tiling.** The fit test and its margin, how the rounding increment and the range ends interact, and the refusal codes.
2. **Frame and sheet storage.** What is stored on which object, how staleness and ownership are checked, and the DXF and DWG round-trip evidence.
3. **Template contract.** How a template names its plan and profile viewports, and how a layout is copied with its viewports and title block.
4. **Profile viewport.** How the per-frame profile view is placed in model space, its elevation range rule, and how the profile viewport's view and scale are set.

## Owner decisions

The recommended answers are written into the scope above; the owner confirms or changes them before the agents start.

1. Sheets are layouts in the current drawing. No separate sheet drawings and no sheet set file.
2. Two arrangements: plan only, and plan with profile.
3. Frames tile at the longest station length that fits, with matchlines on multiples of a rounding increment. A frame edited by hand is refused; rerun the command instead.
4. The brief is the specification; the REVENG Sheet Generator record fills gaps (the owner confirmed on 2026-11-12 that REVENG is clean-room, so no licence check applies).
5. Plotting stays with the existing PUBLISH and Plot.
6. Only #87 and #85 item 5 ride along.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 4 from Claude and at least 6 from Codex.
- [ ] All four items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branches. Claude's branch changes only its notes file. Each Codex PR carries a posted review with a verdict, and the notes PR is open, before anyone merges.
- [ ] Known-answer tests, with the expected values computed independently of the code under test and the working shown in a comment:
  - a straight 1,000-unit alignment, frames 250 units long in model space, increment 10: four frames, matchlines at 250, 500 and 750, centres and twist exact;
  - a line, arc, line alignment: each frame's stations, centre and direction, and every frame contains its piece of alignment;
  - an arc too tight for one increment is refused with the code naming its station;
  - a start station of 1234.5 gives profile grid lines on round stations.
- [ ] Hostile input does not panic: zero, negative and non-finite sizes, scales and increments, an increment far smaller than the alignment (frame count is bounded), and a corrupted stored frame group. Each is refused with an error.
- [ ] Through the CLI or control channel:
  - `viewframecreate`, then `sheetcreate` with a two-viewport template, makes one layout per frame with the views, twists and replaced placeholders expected, and undo removes them all;
  - frames and sheets survive save and reopen as DXF and as DWG, and a moved frame is refused;
  - rerunning `viewframecreate` with a different size, then `sheetupdate`, adds or deletes owned layouts and leaves an unowned layout untouched;
  - a name that collides with an unowned layout is refused and nothing changes;
  - COPY of an alignment with both profiles drops the ground profile on the copy (#87).
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, including the wasm and layers gates, and each PR merges without anyone fixing a conflict by hand. Merge Codex's PRs in order, then Claude's notes.
