# Slice 2 notes: Claude (TIN build, LandXML export)

Agent id `2eefe075-7879-44ee-8b16-0b7e8faf6d39`; Codex `b7f698a7-a42e-4d95-a4af-6a21772f3d8c`.

## Message log (UTC, 2026-11-04)

| Time | Direction | Message id | Summary |
|---|---|---|---|
| 08:52 | Claude → Codex | 8994c47a | Hello and contract: TIN ↔ polyface API, outside-point rule, layers C-TOPO-MINR/MAJR. |
| ~08:53 | Codex → Claude | 2e005518 | Agrees to most; asks for `tin_to_polyface` → `Result`, strict `polyface_to_tin`, XY-distance 1e-9 edge tolerance. |
| 08:53 | Claude → Codex | 5048efaf | Accepts all three; final contract (strict conversion, contract-v1 degeneracy test). |
| ~08:54 | Codex → Claude | b8c75f85 | Accepts; asks how its PR compiles before my conversion is on `main`. |
| 08:55 | Claude → Codex | b77fe814 | Proposes stacked PR: Codex's PR based on `slice2/claude-tin` after my conversion commit. |
| ~08:56 | Codex → Claude | 05710c4c | Agrees; will rebase onto my final reviewed head; Greg retargets to `main` after my merge. |
| 08:57 | — | — | First commit touching the conversion: 82fba1a (after the agreement above). |
| 08:59 | Claude → Codex | dbdb31ec | Conversion pushed at 6b2db119; dead-code note until the commands land. |
| ~09:05 | Claude → Codex | d4d80855 | Shared-file request: `sha2` and `crc32fast` as engine dev-dependencies for the handoff test. |
| ~09:07 | Codex → Claude | 3735d7f1 | Approves the dev-dependencies; has rebased onto 6b2db119; will rebase onto my final head. |
| ~09:09 | Codex → Claude | 2e52716d | Its command tests pass on my prerequisite; flags rustfmt in `surface_build.rs` (already fixed by my `cargo fmt`). |
| 09:11 | Claude → Codex | 27ecf5fe | PR up: #7 at 99d1291; review request. |
| ~09:12 | Codex → Claude | 61339e9d | Rebased onto 99d1291; reviewing. |
| ~09:15 | Codex → Claude | 50edd1f9 | Must-fix: hand-rolled calendar (AGENTS.md chrono rule); centroid clipping leaks outside the boundary. Approves `chrono` in the surface manifest. |
| ~09:20 | Codex → Claude | e74c9912 | Review done (findings on #7); asks for "fixes pushed" with the new SHA. |
| ~09:25 | Codex → Claude | 9e167723 | Its stack CI is green apart from the boundary reproducer, which waits on my fix. |
| ~09:30 | Claude → Codex | 69848468 | Fixes pushed at c804107; every finding answered on #7; the reproducer input now gives a build error. |
| ~09:40 | Codex → Claude | ee198f5d | Accepts c804107; new hole: a concave notch touching a triangle side at boundary vertices; suggests a vertex-inside test. |
| ~09:45 | Claude → Codex | b76ec64b | Fixes pushed at 0083b45: clipped-segment interior test (covers the notch and touch-point chords); proposed as my final head. |
| ~09:55 | Codex → Claude | c3912e69 | Final review: #7 accepted at 0083b45 (closeout on the PR); its CLI CSV→TIN→contours→LandXML run passes; its PR is coming. |
| ~10:05 | Codex → Claude | bcc9337a | PR up: #9 at 1003e6b, based on `slice2/claude-tin` @ 0083b45; review request. |
| ~10:20 | Claude → Codex | fcd0ee06 | Review done on #9: accepted, no blocking findings; 3 non-blocking notes on the PR. |
| ~10:25 | Codex → Claude | 7c1fc373 | All three notes answered on #9 with reasons (hex-only per the contract; caps stay until an index; unused vertices intentional). No code change. |

## Decisions and why

- **Duplicate XY**: breaklines go in first, so a breakline's elevation beats a point's; otherwise the first point wins. Exact XY matches only. Near-duplicates make slivers, which the contract-v1 degeneracy filter drops.
- **Crossing breaklines**: skipped and counted (`skipped_segments`), never forced. spade's `add_constraint` panics on crossings, so the code uses `try_add_constraint`.
- **Boundary**: keeps whole triangles only: every corner inside or on the boundary (1e-9), and no boundary edge passing through its interior. Each boundary edge is clipped to the triangle and the clipped piece's midpoint tested. That covers proper crossings, boundary vertices inside, and chords between two touch points, which a vertex-inside test alone would miss (Codex's notch case, ee198f5d). Nothing is cut, and no boundary edges are inserted, since a boundary has no elevations. Centroid-only clipping leaked outside the boundary (Codex's review), so it was replaced. A bug found on the way: `f64::signum(0.0)` is `1.0`, so the orientation sign is computed by hand.
- **Dates**: through `chrono` (AGENTS.md), added to `cadcraft-surface` as agreed in 50edd1f9.
- **Unused vertices** after clipping are removed, so LandXML point ids are dense.
- **Handoff test**: a hand-built stored ZIP. The manifest uses a known vertical datum (EPSG 5703), so the expected status is `Valid`, not `ValidWithWarnings`.

## Agreed contract (before any conversion code was written)

- `pub(crate) fn tin_to_polyface(&Tin) -> crate::Result<PolyfaceMesh>`; errors on non-finite, out-of-range or repeated indices.
- `pub(crate) fn polyface_to_tin(&PolyfaceMesh) -> crate::Result<Tin>`; strict, quads split (a,b,c)+(a,c,d), degenerate faces (contract-v1 test) are errors, output counter-clockwise.
- `pub(crate) fn tin_from_handle(&Drawing, Handle) -> crate::Result<Tin>`.
- `tincompare`: points outside the TIN are left out of the statistics and counted as `outside`; no extrapolation; on-edge tolerance is XY distance 1e-9.
- `tincontour` layers: `C-TOPO-MINR` (minor) and `C-TOPO-MAJR` (index), default colour.
- Deviation from the slice doc (agreed in 05710c4c): Codex's PR is based on `slice2/claude-tin`, retargeted to `main` by Greg after mine merges.

## End-to-end run (pass criterion)

On Codex's head 1003e6b, with `cadcraft-cli run` (release) and a 6-point PNEZD CSV:
`rtkimport` (6 points) → `tincreate {"max_edge":30}` (6 triangles) → `tincontour {"surface":"106","interval":0.5,"index_every":4}` (9 contours: 2 index, 7 minor) → `tincompare` (count 2, outside 1) → `tinexport {"epsg":32613,...}` (6 points, 6 faces). The export round trip is `tincreate_then_tinexport_round_trips_through_a_valid_handoff_package` (the handoff validator returns `Valid`).
