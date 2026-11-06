# Slice 7: New Proposal (Codex codes, Claude reviews)

Same Paseo setup as slices 2 to 6. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice7]`.

**Codex writes all the code.** By owner request, Claude stays available for orchestration: it settles the designs with Codex by message, reviews Codex's PR in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it.

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
| Codex (all code) | `slice7/codex-proposal` | `crates/proposal/src/manifest.rs`, `planner.rs`, `paths.rs`, `approval.rs`, `crates/proposal/tests/*.rs`, `crates/engine/src/cmd/proposal.rs`, `docs/agents/slice-7-notes-codex.md` | **Manifest:** our own versioned JSON format for base roots by year, folders, model and sheet templates, sheet profiles (orientation, size, layout, title block, placeholders, plot settings), property mappings and xrefs, parsed fail-closed (unknown fields, duplicates, bad paths and wrong versions are issues, never defaults). **Planner:** the ordered, dependency-linked action list from inputs and a manifest, with deterministic ids and JSON. **Paths:** safe name components, safe relative artifact paths, case-insensitive collisions and length limits, as pure functions. **Approval and receipt:** plan hashing (SHA-256 of the plan JSON), the stale-approval check, and the receipt with its outcome (Cancelled, Refused, Failed or Succeeded). **Command:** `newproposal` ("New Proposal") with `action: plan` (returns the plan, its hash and any issues, writes nothing) and `action: create` (needs the approved hash, stages everything under a temporary folder, verifies each artifact, moves the folder into place, writes the receipt last, and cleans up on failure). Interactive prompts for the inputs. |
| Claude (review) | `slice7/claude-review` | `docs/agents/slice-7-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews Codex's PR in full, runs its tests, checks it against the references, and posts findings on the PR. Writes no code; a fix goes back to Codex by message. |

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Path and approval API.** Function signatures, issue codes and the receipt fields.
2. **Manifest format.** The JSON schema, version 1, and a small example manifest for tests.
3. **Create and verify.** Staging and the final move, what verification checks per artifact (exists, opens, layout and title block present, xref resolves), cleanup on failure, and where the receipt goes when the move fails.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branch. Claude's branch changes only its notes file.
- [ ] The reference planner and manifest tests are ported (adapted to our format) and pass. A `newproposal` plan → create run through the CLI or control channel makes a project whose drawings open in CADCraft, and a second create into the same target is refused. A create with a changed plan and the old hash is refused as stale.
- [ ] Each PR touches only its own files. `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, and both merge without anyone fixing a conflict by hand. Merge `slice7/codex-proposal` first, then Claude's notes.
