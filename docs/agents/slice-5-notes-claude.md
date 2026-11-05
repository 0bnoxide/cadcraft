# Slice 5 notes: Claude (support)

Agent id `e5408ba6-2a47-4a94-8b4d-4948cc5002d3`. Codex: `0f92e804-b093-459d-a84e-af7815fe197a`. Branch `slice5/claude-audit`.

## Message log (UTC)

| Time | Dir | Summary |
|---|---|---|
| 2026-11-05 ~07:30 | Codex → Claude | Hello and id; asks for the audit codes and receipt first. |
| 2026-11-05 ~07:31 | Claude → Codex | Item 1 proposal (6201272a): `PF-*` / `TIN-*` codes, `Severity`, `Issue {code, severity, message, at, refs}`, `Receipt {check, version, inputs, thresholds, counts}` with sorted maps; `tinvolume` reuses `Receipt` with `check: "volume"`. |
| 2026-11-05 ~07:33 | Codex → Claude | **Agrees item 1** (4530efb7). Proposes item 2 (midpoint grid volumes, skipped area reported, cut/fill/net, per-region rows) and item 3 (IDW power 2, all wells, 12 neighbours, at least 3 distinct wells, collinear rejected for natural neighbour and gradient, no extrapolation). |
| 2026-11-05 ~07:34 | Claude → Codex | **Agrees items 2 and 3** (2d5392b2), asking for a hard grid-cell cap with an error and a sloped-planes analytic test. |
| 2026-11-05 ~07:36 | Codex → Claude | Accepts both (3945dca6). |

## Decisions

- Neighbour outliers use the Delaunay neighbours of each point (spade, already a dependency) and their median elevation; no k or radius to tune, and no O(n²) scan.
- Spikes use the **plan** length of the shortest edge; the slope is the triangle plane's angle from horizontal.
- Breakline crossings include T junctions; a shared endpoint is not a crossing. Collinear overlaps are not reported (marked `shortcut:`). Above 100 000 segments the check is skipped and the receipt says so.
- `surfaceaudit` takes boundary and breaklines as coordinates, like `tincreate`, since a polyface TIN does not carry them.
- Not ported: "out of date against its definition", definition items pointing at missing files, data shortcuts, and the boundary-through-vertex case (Civil 3D storage states with no CADCraft equivalent).
