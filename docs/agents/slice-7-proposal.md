# Slice 7: New Proposal (Codex leads, Claude supports)

Same Paseo setup as slices 2 to 6. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice7]`.

**Codex does most of the implementation, measured in lines.** Slice 6 missed that target because the support work grew. Here Claude's modules are deliberately small and fixed: if Claude's share starts to grow, Claude stops and hands the extra work to Codex by message instead.

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

**Work in ponytail mode** (owner request): read `C:\Users\ichbi\AutoGIS\.claude\skills\ponytail\SKILL.md` before writing code and follow it for the whole slice, or invoke the `ponytail` skill where it is installed. Smallest complete change, reuse before writing, no unrequested options. It never skips validation, error handling or the never-crash rules.

## Scope

All new library code lives in `crates/proposal` (standalone: no workspace dependencies), ported from behaviour only, from AutoGIS-Civil3D (`C:\Users\ichbi\AutoGIS-Civil3D`, read only): `src/AutoGIS.Civil3D.Proposal/*`, the preview, approval, executor and verifier in `src/AutoGIS.Civil3D.Adapter/*`, and `tests/AutoGIS.Civil3D.Proposal.Tests/*`. Port behaviour and the tests' intent, not the C# structure.

New Proposal creates a project folder from a standards manifest: folders, model drawings from a DXF/DWG template, sheet drawings with a layout and a title block whose attributes come from the inputs, xrefs between drawings, and support records (project configuration, empty registers and logs, the plan, and a receipt written last). It plans first, shows the plan, and only creates files after the user approves that exact plan.

Changes from the reference:
- **No sheet sets.** CADCraft has no sheet set manager. Write a sheet index as a support record (our own JSON) instead, and fill title-block attributes directly.
- **Templates are DXF or DWG** read through the existing `io` crate, not `.dwt`. Tests use small synthetic templates they write themselves; never commit files produced by AutoCAD.
- **Paths are portable.** Keep the reference's Windows-safe name rules (reserved names, forbidden characters, length limits) on every platform, so a project made on Linux opens on Windows.

Leave out: Civil 3D data shortcuts (create the folder only), the WinForms form (the interactive prompt sequence replaces it), and the MCP server (CADCraft's MCP reaches the command).

## Ownership

Shared files are already on `main` and pre-filled:
- `crates/proposal/src/lib.rs` and `Cargo.toml` (`serde`, `serde_json`, `sha2`)
- `issue.rs`, which defines `Inputs` and `Issue`
- the `cmd.rs` registration, the workspace and engine `Cargo.toml` entries, `Cargo.lock` and `xtask/src/layers.rs`
- the command label in `zh.tsv`

Change a shared file only after agreeing it by message, and cite that message in the PR.

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (lead) | `slice7/codex-proposal` | `crates/proposal/src/manifest.rs`, `planner.rs`, `crates/proposal/tests/manifest*.rs`, `planner*.rs`, `crates/engine/src/cmd/proposal.rs`, `docs/agents/slice-7-notes-codex.md` | **Manifest:** our own versioned JSON format for base roots by year, folders, model and sheet templates, sheet profiles (orientation, size, layout, title block, placeholders, plot settings), property mappings and xrefs, parsed fail-closed (unknown fields, duplicates, bad paths and wrong versions are issues, never defaults). **Planner:** the ordered, dependency-linked action list from inputs and a manifest, with deterministic ids and JSON. **Command:** `newproposal` ("New Proposal") with `action: plan` (returns the plan, its hash and any issues, writes nothing) and `action: create` (needs the approved hash, stages everything under a temporary folder, verifies each artifact, moves the folder into place, writes the receipt last, and cleans up on failure). Interactive prompts for the inputs. |
| Claude (support) | `slice7/claude-paths` | `crates/proposal/src/paths.rs`, `approval.rs`, `crates/proposal/tests/paths*.rs`, `approval*.rs`, `docs/agents/slice-7-notes-claude.md` | **Paths:** safe name components, safe relative artifact paths, case-insensitive collision detection and length limits, as pure functions on strings. **Approval and receipt:** plan hashing (SHA-256 of the plan JSON), the stale-approval check, and the `Receipt` record with its outcome (Cancelled, Refused, Failed or Succeeded). No filesystem access and no command. **Review:** Claude reviews Codex's PR in full, running its tests and checking it against the references. |

## What to agree by message before coding against it

1. **Path and approval API** (Claude proposes first, as its first message; Codex agrees). Function signatures, issue codes and the receipt fields. Claude pushes a first working `paths.rs` early, so Codex isn't blocked.
2. **Manifest format** (Codex proposes, Claude agrees). The JSON schema, version 1, and a small example manifest for tests.
3. **Create and verify** (Codex proposes, Claude agrees). Staging and the final move, what verification checks per artifact (exists, opens, layout and title block present, xref resolves), cleanup on failure, and where the receipt goes when the move fails.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] Codex's diff is at least twice Claude's in added lines. If it isn't, the PR says why.
- [ ] The reference planner and manifest tests are ported (adapted to our format) and pass. A `newproposal` plan → create run through the CLI or control channel makes a project whose drawings open in CADCraft, and a second create into the same target is refused. A create with a changed plan and the old hash is refused as stale.
- [ ] Neither PR touches a file the other agent owns. `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice7/claude-paths` first.
