# Slice 2: TIN from points (Claude + Codex, talking directly through Paseo)

Two agents build this slice in parallel, and both run as **Paseo agents on the same host**. This slice tests whether they can coordinate by **messaging each other directly** through Paseo's `send_agent_prompt` tool. Any decision that needs the other agent goes through a message. PR comments are for the record and the review, not for the conversation.

Read `CLAUDE.md` first. Its rules apply to both agents: never crash, no `unwrap`/`expect`/indexing on input, `cargo xtask ci` before every push, clean-room. Where it says "push to `main`", this slice overrides it: work on branches and open PRs.

## Ownership

Each agent edits **only** its own files. Everything shared is already on `main`: this file, `crates/surface/src/lib.rs`, `tin.rs`, `crates/surface/Cargo.toml` (with `spade`), `Cargo.lock`, `cmd.rs` registration, and the four command labels in `zh.tsv`. Change a shared file only after agreeing by message, and say in the PR which message agreed it.

| Agent | Branch / worktree | Files | Builds |
|---|---|---|---|
| Claude | `slice2/claude-tin` | `crates/surface/src/triangulate.rs`, `crates/surface/src/landxml.rs`, `crates/surface/tests/triangulate*.rs`, `crates/surface/tests/landxml*.rs`, `crates/engine/src/cmd/surface_build.rs`, `docs/agents/slice-2-notes-claude.md` | Constrained Delaunay TIN from points, breaklines and an optional outer boundary (`spade`), with duplicate-XY handling, a maximum edge length and boundary clipping. LandXML 1.2 surface export. A unit test in `surface_build.rs` (the engine already depends on `cadcraft-handoff`) wraps the export in a contract v1 handoff package, and `cadcraft_handoff::validate` must return `Valid`. `crates/surface` itself is a standalone crate, so it can't depend on another workspace crate. Commands `tincreate` ("Create TIN Surface", drawn as a polyface mesh) and `tinexport` ("Export TIN to LandXML"). The **TIN ↔ polyface conversion** in `surface_build.rs` is also Claude's, `pub(crate)`. |
| Codex | `slice2/codex-contours` | `crates/surface/src/contour.rs`, `crates/surface/src/compare.rs`, `crates/surface/tests/contour*.rs`, `crates/surface/tests/compare*.rs`, `crates/engine/src/cmd/surface_contour.rs`, `docs/agents/slice-2-notes-codex.md` | Contours by marching triangles, chained into open or closed polylines, at an interval with index contours every N, with flat-triangle and vertex-on-level cases handled. Surface comparison: one TIN's elevation at points or at another TIN's vertices, plus difference statistics (min, max, mean, RMS, count outside). Commands `tincontour` ("Create Contours", draws 3D polylines on two layers) and `tincompare` ("Compare Surfaces", reports only). |

Reference behaviour, read only: AutoGIS `core/envmon/groundwater_contours.py`, `compare_drone_surfaces.py` and `core/common/landxml.py`, plus AutoGIS-Civil3D's `contract/v1` for LandXML. Port behaviour, never code structure.

## The one interface you must negotiate

Codex's commands read a TIN from the drawing, which means a polyface mesh selected by handle. Claude owns that conversion and must not write it until Codex agrees the signature **by message**. This is the forcing case for the test: settle it in the first exchange, before anyone writes code that depends on it. Also agree by message:
- what `tincompare` does when a point is outside the TIN, and
- the layer names `tincontour` draws on.

## Messaging protocol (Paseo)

- You have Paseo tools (`send_agent_prompt`, `list_agents`, `get_agent_status`). Your own id is in `PASEO_AGENT_ID`. The other agent's id is in your kickoff prompt, or find it with `list_agents`.
- Start every message with `[slice2]`, then your name (`Claude:` or `Codex:`), and always include your own agent id so the other agent can reply.
- Send these messages, at least:
  1. **Hello and contract** (Claude → Codex): the proposed TIN ↔ polyface API, the outside-point rule and the layer names. Codex replies to agree or change it. Repeat until you agree.
  2. **PR up** (each agent → the other): the PR link, and a request to review it.
  3. **Review done** (each agent → the other): the verdict, with the findings also posted on the PR.
  4. **Fixes pushed** (each agent → the other) after addressing review findings.
- Don't block waiting for a reply. Keep working on things that don't depend on it. If you have no reply after about 15 minutes of your own work, send a short reminder.
- Keep a log of every message you send or receive (time, direction, one-line summary) in your notes file.

## Process

1. Each agent creates its worktree under `.claude/worktrees/<branch>`, with its own `CARGO_TARGET_DIR`.
2. Agree the contract by message, then build, test and run `cargo xtask ci`.
3. Open a PR to `main`, then message the other agent.
4. Review the other agent's PR: run its tests, leave at least one concrete finding or an explicit "checked X, no issue", then message the verdict.
5. Answer every finding with a fix or a reason, and message when it's pushed.
6. Greg merges. Neither agent merges.

## Pass criteria for the Paseo messaging test

- [ ] Both agents ran **inside Paseo** on ValiantObn0xid3 (Claude Code provider and Codex provider), with Paseo tools enabled.
- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls from **each** agent to the other: at least 4 per agent.
- [ ] The polyface API, the outside-point rule and the layer names were agreed **by message before** the code that depends on them was written. Both notes logs show this, with timestamps earlier than the first commit touching those parts.
- [ ] At least one disagreement or question was settled by message without Greg stepping in.
- [ ] Neither PR touches a file the other agent owns, and shared-file changes cite the message that agreed them.
- [ ] Each PR has a review from the other agent, requested and closed out by message, with every finding answered.
- [ ] `cargo xtask ci` passes on Windows for each PR head, and on `main` after both merge.
- [ ] Both PRs merge without Greg fixing a conflict by hand.
- [ ] A `tincreate` → `tincontour` → `tinexport` run on a sample point file works through the CLI or control channel, and the export round-trip test passes.
