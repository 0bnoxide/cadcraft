# Slice 9: proposal follow-ons (Codex codes, Claude reviews)

Same Paseo setup as slices 7 and 8. Both agents run as Paseo agents on the same host and coordinate by **messaging each other directly** with `send_agent_prompt`. The messaging protocol in `docs/agents/slice-2-tin.md` applies unchanged, with the prefix `[slice9]`.

**Codex writes all the code.** Claude settles the designs with Codex by message, reviews Codex's PR in full, and writes no production or test code. If Claude finds something to change, it asks Codex to change it.

Read `AGENTS.md` first: never crash, run `cargo xtask ci` before every push, keep it clean-room. Work on branches and open PRs; never push to `main`. Per the house rules, PRs, commits and comments carry no agent attribution.

If you find an issue that is not fixed in your current PR, file it as a GitHub issue on 0bnoxide/cadcraft (`gh issue create`) and link it from the PR or your notes.

**Work in ponytail mode** (owner request) for the whole slice: question whether each change needs to exist, reuse existing code before writing new code, prefer the standard library and existing dependencies, and make the smallest complete change once the problem is understood. Add no options nobody asked for. This never skips validation, error handling, tests or the never-crash rules.

## Scope

Two new workflows on top of slice 7's `newproposal`, plus four open follow-up issues. The reference is the AutoGIS-Civil3D design `docs/superpowers/specs/2026-09-11-follow-on-workflow-candidates-design.md` (read only), candidates 1 and 3. Port the behaviour and guardrails, not the Civil 3D API. Both workflows keep slice 7's shape: inputs, validate, deterministic plan, preview, approval bound to the plan hash, staged execution, reopen-and-verify, receipt.

1. **Single-drawing staging (shared by items 2 and 3).** These are the first workflows that change a drawing an earlier run created. Owner decision (see "Owner decisions" below): edit a staged copy beside the target, reopen and verify it, then move the prior file to `_superseded/<stem>.<UTC timestamp>.<ext>` in the same folder and move the staged copy into place. The approval binds to the SHA-256 of the target as planned, so a drawing changed after planning makes the approval stale. A failure before the swap leaves the target untouched; a failure during the swap restores the prior file. The receipt records the prior and new hashes and the retained path. Build this once, beside slice 7's project publish, and reuse it from both commands. No general workflow framework: share only what the two commands actually use.
2. **Intake Source Files** (`intakesource`). Inputs: a proposal root created by `newproposal`, and one or more source files, each with a role. Copy each source into the manifest's source location, keep its original filename, and record its SHA-256 in the proposal's existing `SourceRegister` support record. Then, by format:
   - a DWG or DXF source gets one relative-path **overlay xref** into the `BaseModel` drawing, using the same conventions as slice 7's xref graph;
   - a raster image source (PNG, JPEG, TIFF) gets one relative-path **image attachment** in `BaseModel` (`EntityKind::Image`), a separate planned action from an xref;
   - every other format is copied and registered only, and its register row says so.
   Refuse a reference name equal to the host drawing's name, a source path that is a link, and a name with characters or a length the existing `paths` rules reject. Never clean, reproject, explode or convert source content. Verify: each copy's hash equals the original's, every attachment reopens with the planned relative path and transform, and the register holds exactly the planned rows.
3. **Configure Drawing** (`configuredrawing`). Inputs: a target drawing inside a proposal root and the manifest's drawing-settings block. The plan is a diff, with one action per setting that would change and its current and planned values. A drawing already in conformance gives an empty plan and no write. Settings in scope are only what CADCraft drawings hold: units and precision header values, the manifest's layers (create missing ones, set colour, linetype and lineweight on named ones), named text and dimension styles, and the annotation scale list. Never touch model or paper-space entities, rename or purge layers, or change a setting the manifest does not name. A coordinate-system setting is out: the fork has no PROJ. Verify by reopening and re-reading every changed setting.
4. **Manifest version 2.** Items 2 and 3 need a source-role list, a source location and a drawing-settings block, none of which manifest version 1 has. Add them as manifest version 2. Version 1 manifests keep working for `newproposal` unchanged; `intakesource` and `configuredrawing` refuse a version 1 manifest with a clear issue code. Source roles (owner decision): `survey`, `record_drawing`, `gis_export`, `imagery`, `other`. Default source location: `Support/Sources/<role>/`.
5. **Follow-ups.** Close each from the PR:
   - **#30:** add the forced final-move failure test for `newproposal` (and cover the new swap the same way), and move `crates/proposal/tests/fixture.rs` to `tests/common/mod.rs`.
   - **#32:** use one no-replace rename helper for both the project publish and the new swap. `rustix` is already in `Cargo.lock` (MIT OR Apache-2.0) and has `renameat_with` with `RenameFlags::NOREPLACE`. Use it as a direct dependency where the platform supports it, with no `unsafe`. Where it doesn't (or on Windows, whose rename already refuses an existing target), keep today's check and the documented comment.
   - **#34:** keep the bounded per-point checkpoint summary in `qareview` draft evidence, bound to the draft and source hashes, as the issue describes.
   - **#36:** bound the `pointconvert` short-row diagnostics before each push, as the issue describes.

## Ownership

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Codex (all code) | `slice9/codex-follow-ons` | `crates/proposal` sources and tests, `crates/engine/src/cmd/proposal.rs` and new `intakesource.rs` and `configuredrawing.rs` with their registration in `cmd.rs`, `crates/droneqa` for #34, `crates/engine/src/cmd/pointconvert.rs` for #36, `zh.tsv` rows for new labels, `Cargo.toml`/`Cargo.lock` only for `rustix`, `docs/agents/slice-9-notes-codex.md` | Items 1 to 5. New commands: `intakesource` ("Intake Source Files") and `configuredrawing` ("Configure Drawing"), each with the same plan, preview and approve flow as `newproposal`. |
| Claude (review) | `slice9/claude-review` | `docs/agents/slice-9-notes-claude.md` only | **Design:** agrees or changes each of Codex's proposals below by message. **Review:** reviews Codex's PR in full, runs its tests, checks it against the reference design, and posts findings on the PR. |

Codex may split this into two stacked PRs (items 1, 2, 4 and the #30/#32 work first; item 3, #34 and #36 second) if one PR gets too large to review. Say so by message first.

## What to agree by message before coding against it

Codex proposes each one; Claude agrees or asks for changes.

1. **Staging and swap.** The staged-copy location, the `_superseded` naming, the failure and restore paths, how approval binds to the target hash, and the no-replace rename helper (#32).
2. **Manifest version 2 and intake.** The new manifest fields and their validation, the register row format, the xref and image naming rules, and the format-to-action table.
3. **Configure Drawing.** The settings block schema, how each setting is read and compared, and the action and issue codes.

## Owner decisions

These were open owner inputs in the reference design. The recommended answers are written into the scope above; the owner confirms or changes them before the agents start.

1. Staging and rollback for editing an existing drawing: staged copy, verify, retain the prior file in `_superseded/`.
2. Source roles and location: `survey`, `record_drawing`, `gis_export`, `imagery`, `other`, under `Support/Sources/<role>/`.
3. Autodesk Docs / Desktop Connector hosting: not applicable, so no special handling.

## Pass criteria

- [ ] `paseo logs <id> --filter tools` shows `send_agent_prompt` calls both ways: at least 3 from Claude and at least 4 from Codex.
- [ ] All three items above were agreed by message before the dependent code was committed. Both notes logs show this with timestamps.
- [ ] All code is on the Codex branch (or its stacked PRs). Claude's branch changes only its notes file.
- [ ] Through the CLI or control channel, on a project made by `newproposal` from a version 2 manifest:
  - `intakesource` with a DXF, a PNG and a CSV source attaches the DXF as an overlay xref and the PNG as an image in `BaseModel`, registers all three, and leaves the prior `BaseModel` in `_superseded/`;
  - a second run with the drawing changed after planning is refused as stale;
  - `configuredrawing` on a conforming drawing plans nothing and writes nothing;
  - `configuredrawing` on a drawing with a wrong unit and a missing layer fixes only those, and a reopen shows them;
  - a version 1 manifest is refused by both new commands and still works for `newproposal`.
- [ ] Forced failure tests show the target unchanged (or restored) and no success receipt after a failed swap or publish.
- [ ] Issues #30, #32, #34 and #36 are closed by the PR(s), or #32 is closed with the reason it can't be done on a platform.
- [ ] `cargo xtask ci` passes in GitHub CI on Linux and Windows for each head, and each PR merges without anyone fixing a conflict by hand. Merge Codex's PR(s) first, then Claude's notes.
