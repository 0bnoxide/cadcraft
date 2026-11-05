# Slice 5: surface analysis (Codex leads, Claude supports)

Same Paseo setup as slices 2 to 4. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice5]`.

**Codex does most of the implementation.** By owner request, Claude takes the smaller audit module plus the review of Codex's PR.

Read `CLAUDE.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`.

## Scope

All new library code lives in `crates/surface` (standalone: no workspace dependencies), ported from behaviour only:
- **Volumes:** cut, fill and net between two TINs inside a `Region`. `CIM-Project-Suite-REVENG/docs/phase2/features/earthwork_processor.md` is a **behaviour description only**, from a commercial product: use our own names, report layout and algorithm, never its structure or enums, and don't read anything else in that repo.
- **Groundwater contours:** AutoGIS (`C:\Users\ichbi\AutoGIS`, read only) `core/envmon/groundwater_contours.py`, including `fit_plane_gradient`, with its tests.
- **Surface audit:** AutoGIS-Civil3D `docs/superpowers/specs/2026-09-11-follow-on-workflow-candidates-design.md`, candidates 4 (Survey Preflight) and 5 (Audit EG Surface). Port the checks, not the .NET code.

Reuse what slices 1 to 3 built (`Tin`, `triangulate`, `contour`, `compare`, the TIN ↔ polyface conversion in `surface_build.rs`). Don't change those files.

Leave out:
- ArcGIS paths (Spatial Analyst, kriging/EBK), corridor and section volumes, and anything that writes back to Civil 3D.

## Ownership

Shared files are already on `main` and pre-filled:
- `crates/surface/src/lib.rs`
- `region.rs`, which defines `Region` (outer ring plus holes, `is_valid`, `contains`)
- the `cmd.rs` registration
- the three command labels in `zh.tsv`

Change a shared file only after agreeing it by message, and cite that message in the PR. A new dependency must be pure Rust and MIT/Apache/BSD, agreed by message, and added to `crates/surface/Cargo.toml` only.

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (lead) | `slice5/codex-volume` | `crates/surface/src/volume.rs`, `interpolate.rs`, `crates/surface/tests/volume*.rs`, `interpolate*.rs`, `crates/engine/src/cmd/surface_volume.rs`, `docs/agents/slice-5-notes-codex.md` | **Volumes:** cut, fill and net between a base and a comparison TIN inside a `Region`, with areas, by exact prism or a documented grid method, plus a per-region breakdown. **Interpolation:** IDW (power, search radius, max neighbours) and natural neighbour onto a grid or a TIN, and the plane gradient (magnitude, azimuth, R²) from `fit_plane_gradient`. **Commands:** `tinvolume` ("Surface Volumes") reads two polyface TINs by handle and an optional boundary polyline, and draws or returns a report Table. `gwcontour` ("Groundwater Contours") takes well points with water levels, interpolates, contours with the existing `contour` module, and draws a flow arrow from the plane gradient. |
| Claude (support) | `slice5/claude-audit` | `crates/surface/src/audit.rs`, `crates/surface/tests/audit*.rs`, `crates/engine/src/cmd/surface_audit.rs`, `docs/agents/slice-5-notes-claude.md` | **Audit:** pure functions. Preflight: duplicate point numbers, codes with no key match, elevations outside a band or far from their neighbours, points outside the expected extent, CRS mismatch. TIN: no outer boundary, crossing breaklines, spikes (steep triangle with a short edge). Each issue has a stable code. **Command:** `surfaceaudit` ("Surface Audit") with `mode: preflight / tin`, reports only and never edits the drawing, returning the issues plus a receipt (inputs, thresholds, counts). **Review:** Claude reviews Codex's PR in full, running its tests and checking it against the references. |

## What to agree by message before coding against it

1. **Audit issue codes and receipt** (Claude proposes first, as its first message; Codex agrees). Codex's `tinvolume` reuses the receipt shape for its own report header.
2. **Volume method and report** (Codex proposes, Claude agrees). Prism or grid, how areas outside either TIN are handled (skipped and reported, never zero-filled), and the report's columns.
3. **Interpolation defaults** (Codex proposes, Claude agrees). Default method, IDW power and radius, the minimum well count (at least 3, as the reference requires), and what happens with collinear or duplicate wells.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] Codex's diff is clearly the larger share of the slice.
- [ ] The AutoGIS `groundwater_contours` gradient tests are ported and pass. A volume test on two planes offset by a known height gives the exact analytic cut and fill. A `tincreate` → `tinvolume` → `surfaceaudit` run works through the CLI or control channel.
- [ ] Neither PR touches a file the other agent owns, or any slice 4 file. `cargo xtask ci` passes on Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice5/claude-audit` first.
