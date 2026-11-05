# Slice 4: callouts and labels (Codex leads, Claude supports)

This slice uses the same Paseo setup as slices 2 and 3. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice4]`.

**Codex does most of the implementation.** By owner request, Claude takes a small module plus the review of Codex's PR, to save Claude usage.

Read `CLAUDE.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`.

## Scope

All new code lives in `crates/labels`, ported from behaviour only:
- **Callouts** from AutoGIS (`C:\Users\ichbi\AutoGIS`, read only): `core/envmon/callout_geometry.py`, `callout_templates.py`, `callout_collision.py` and `label_expression_common.py`, with their tests.
- **Batch labelling** in the spirit of Label Genie, from `CIM-Project-Suite-REVENG/docs/phase2/features/label_genie.md`. Treat it as a **behaviour description only**: that spec came from a commercial product. Use our own template format, names and algorithms. Never copy its `.lg` schema, enum names or any structure from it, and don't read anything else in that repo.

Leave out:
- ArcGIS figure specs and arcpy paths.
- Pipe network, profile and section labels, and cross-drawing or XREF labelling.

## Ownership

Shared files are already on `main` and pre-filled:
- `crates/labels/src/lib.rs`
- `geom.rs`, which defines `Pt` and `Rect`
- `crates/labels/Cargo.toml`, with `serde` and `serde_json`
- the `cmd.rs` registration
- the two command labels in `zh.tsv`

Change a shared file only after agreeing it by message, and cite that message in the PR.

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (lead) | `slice4/codex-labels` | `crates/labels/src/callout.rs`, `placement.rs`, `template.rs`, `crates/labels/tests/callout*.rs`, `placement*.rs`, `template*.rs`, `crates/engine/src/cmd/labels_callout.rs`, `crates/engine/src/cmd/labels_batch.rs`, `docs/agents/slice-4-notes-codex.md` | **Callout tables:** rows from a record, cell layout, box size, grid lines, leader attachment and quadrant offsets. **Placement:** collision-aware placement that ranks candidate positions around each anchor, reports overlaps, and stays inside a boundary when one is given. **Batch label templates:** sources are points on given layers or with given feature codes, polyline segments (length and bearing), and TIN spot elevations at polyface vertices or a grid. **Commands:** `callout` ("Place Callouts") draws a Table or MText box with a leader. `labelbatch` ("Batch Labels") creates MText or MLeader labels, tags them so a rerun updates rather than duplicates them, and can delete its own labels. |
| Claude (support) | `slice4/claude-format` | `crates/labels/src/format.rs`, `crates/labels/tests/format*.rs`, `docs/agents/slice-4-notes-claude.md` | **Content formatter:** format parts (literal, property lookup, number with precision, rounding, suffix and zero suppression, counter, newline), plus the label-field derivation from `label_expression_common.py`. Pure functions, no drawing access. **Review:** Claude reviews Codex's PR in full, running its tests and checking it against the references. |

## What to agree by message before coding against it

1. **Formatter API** (Claude proposes first, as its first message; Codex agrees). Settle the part types, the signature, and how missing properties and non-finite numbers render. Codex's callout cells and batch labels both use it. Claude should push a first working `format.rs` early, so Codex isn't blocked.
2. **Text size measurement** (Codex proposes, Claude agrees). Placement needs each label's rectangle before it is drawn. Agree how the engine commands measure MText or Table extents, and what the `labels` crate receives.
3. **Label ownership tag** (Codex proposes, Claude agrees). How a label records which template and source made it, so reruns update instead of duplicating, for example XData or an extension dictionary.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] Codex's diff is clearly the larger share of the slice.
- [ ] The AutoGIS callout tests are ported and pass. A `labelbatch` rerun on the same drawing updates its labels instead of duplicating them.
- [ ] Neither PR touches a file the other agent owns, `cargo xtask ci` passes on Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice4/claude-format` first.
