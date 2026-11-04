# Slice 3: drone survey QA (Claude + Codex, talking directly through Paseo)

This is the same setup as slice 2. Both agents run as **Paseo agents on the same host** and coordinate by **messaging each other directly** with `send_agent_prompt`. PR comments are for the record and the review. Read `docs/agents/slice-2-tin.md` for the messaging protocol, which applies unchanged; this file lists only what differs.

Read `CLAUDE.md` first (never crash, `cargo xtask ci` before every push, clean-room). Work on branches and open PRs; never push to `main`.

## Scope

This slice ports AIGIS3D (read only, at `C:\Users\ichbi\AIGIS3D`) into `crates/droneqa`, behaviour only:
- the report parser,
- the QA rules,
- the draft/review/commit workflow, and
- redaction.

It leaves out:
- ArcGIS Online, OAuth and the MCP server (CADCraft's own MCP reaches the commands),
- PDF reports: text layouts only; a PDF gives a clear "not supported yet" diagnostic, and
- SQLite: by owner decision, the audit history lives in the drawing or a JSON file.

## Ownership

Shared files are already on `main` and pre-filled:
- `crates/droneqa/src/lib.rs`
- `model.rs`, with the ported `QAMetrics`, `ReportEvidence`, `ParseResult`, `Finding` and `Analysis`
- `Cargo.toml`, with `serde`, `serde_json` and `chrono`
- the fixtures in `crates/droneqa/tests/fixtures/`
- the `cmd.rs` registration
- the three command labels in `zh.tsv`

Change a shared file only after agreeing it by message, and cite that message in the PR.

| Agent | Branch | Files | Builds |
|---|---|---|---|
| Claude | `slice3/claude-qa` | `crates/droneqa/src/mission.rs`, `crates/droneqa/src/qa.rs`, `crates/droneqa/tests/qa*.rs`, `crates/engine/src/cmd/droneqa_check.rs`, `docs/agents/slice-3-notes-claude.md` | **Mission and ruleset:** the `Mission` record, `normalize_mission`, and the `Ruleset` read from JSON. **QA rules:** `analyze` and `summarize` from `qa.py`, including radial RMSE, Z checks, checkpoint independence and bare ground, and the PASS / CONDITIONAL / FAIL / INSUFFICIENT_DATA outcomes. **Tests:** the reference scenarios `pass`, `fail` and `insufficient-data` from `scenarios.json`. **Command:** `droneqa` ("Drone Survey QA") takes a mission, a ruleset and a report, and returns the `Analysis`. |
| Codex | `slice3/codex-review` | `crates/droneqa/src/pix4d.rs`, `crates/droneqa/src/redact.rs`, `crates/droneqa/src/workflow.rs`, `crates/droneqa/tests/pix4d*.rs`, `crates/droneqa/tests/workflow*.rs`, `crates/engine/src/cmd/droneqa_review.rs`, `docs/agents/slice-3-notes-codex.md` | **Parser:** the synthetic v1 and PIX4Dmapper 4.10 text layouts, with the size limits and diagnostics from `parsers.py` and `pix4dmapper.py`. **Redaction:** `redaction.py`. **Workflow:** draft → review → commit with an append-only audit history from `workflow.py` and `storage.py`, without a database. **Commands:** `pix4dimport` ("Import PIX4D Report") returns a `ParseResult`. `qareview` ("QA Review") takes `action: draft / review / commit / history`. |

Port the reference tests in `tests/test_parsers.py`, `test_pix4dmapper.py`, `test_domain.py`, `test_findings.py`, `test_workflow.py`, `test_workflow_adversarial.py`, `test_auth_redaction.py` and `test_fixture_scenarios.py`. Leave out tests that need ArcGIS Online, OAuth or MCP.

## What you must negotiate by message before coding against it

1. **Parser API** (Codex proposes, Claude agrees). `droneqa` calls the parser: settle its signature and the content-type handling.
2. **Where the audit history lives** (Codex proposes, Claude agrees). Choose either the drawing (a named dictionary or XRecord) or a JSON file next to the drawing. Settle the draft record's schema too, which embeds Claude's `Analysis` plus the mission id and ruleset version.
3. **Redaction boundary** (Claude proposes, Codex agrees). Which `Mission` fields count as sensitive, and whether `analyze` redacts or the workflow does.

## Pass criteria

These are the same as slice 2, with these changes:
- [ ] `paseo logs <id> --filter tools` shows at least 4 `send_agent_prompt` calls from each agent to the other.
- [ ] All three items above were agreed by message before the code that depends on them was committed. Both notes logs show this with timestamps.
- [ ] The three reference scenarios give the reference outcome through `droneqa`, and a `pix4dimport` → `droneqa` → `qareview draft/review/commit` run on `valid.txt` works through the CLI or control channel.
- [ ] Neither PR touches a file the other agent owns, `cargo xtask ci` passes on Windows for each head, and both merge without Greg fixing a conflict by hand.
