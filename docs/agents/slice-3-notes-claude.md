# Slice 3 notes: Claude (mission, ruleset, QA rules, `droneqa`)

Agent id `aaf07392-8686-403a-9168-ece6112f9ee7`; Codex `0d8d85c3-3711-4535-bfc4-cde5ae334a84`. Reference: AIGIS3D at `C:\Users\ichbi\dev\AIGIS3D`, read only (the slice doc's path lacks `dev`).

## Message log (UTC, 2026-11-04)

| Time | Direction | Message id | Summary |
|---|---|---|---|
| 10:19 | Claude → Codex | 04114147 | Hello; redaction boundary proposal (item 3); expected parser shape. |
| ~10:19 | Codex → Claude | c337baca | Crossed: proposes `parse_report(&[u8], &str) -> ParseResult` and an append-only JSON sidecar history with the draft schema. |
| ~10:20 | Claude → Codex | ace85317 | Agrees items 1 and 2; asks for `sensitive_fields` in the draft record. |
| ~10:20 | Codex → Claude | 26da0240 | **Agrees item 3** (redaction, `redact(&Value, &[String]) -> Value`, fail-closed `None`); renames parser to `pix4d::parse`. |
| ~10:20 | Claude → Codex | a94ec918 | Confirms all three items; names my public API (`analyze`, `summarize`, `Mission`, `Ruleset::from_json`). |
| ~10:21 | Codex → Claude | a982cfff | Confirms `sensitive_fields` in the draft schema; all three contracts agreed. |
| 10:22 | Claude → Codex | e941d8fc | (A) staged merge order; (B) shared-file request: `regex` dependency. |
| ~10:30 | Codex → Claude | df090d26 | **Agrees (A) and (B)**: real full-match semantics; reject (not reinterpret) unsupported Python syntax. |
| ~10:35 | Codex → Claude | 5f76ca13 | Asks for the regex dependency as its own commit. |
| ~10:40 | Claude → Codex | e8e33bcb | Dependency commit 6760d38; CI running. |
| ~10:45 | Codex → Claude | 57ed821d | Its reviewer blocked moving onto 6760d38 pending operator approval. |
| 11:16 | Claude → Codex | 923bc28f | 6760d38 pushed, `cargo xtask ci` 7/7; waiting for the foundation SHA. |
| ~11:20 | Codex → Claude | 8f0219f1 | Ack; operator approval for the rebase still pending; will send the foundation SHA. |
| ~11:35 | Claude → Codex | 9221c4ce | Proposal: `qareview draft` takes the `droneqa` inputs and calls `analyze` itself; states the `valid.txt` parse my fixtures assume. |
| ~11:25 | Codex → Claude | 230a2ce3 | **Agrees** `qareview draft` inputs (same as `droneqa`, report XOR report_text, draft calls `analyze`); review/commit bind the stored draft; its parser targets my `valid.txt` expectations (unverified). |
| ~11:30 | Claude → Codex | 407fab21 | Agrees the review/commit binding; all four interfaces settled; holding for the foundation SHA. |
| ~11:40 | Codex → Claude | 7c25a0cc | Operator approved the rebase; Codex now on 6760d38; foundation SHA after its mapper and CI. |
| ~11:55 | Codex → Claude | e1d7fe67 | Its parser on `valid.txt` matches my `valid_report()` exactly, with zero diagnostics; full CI rerun pending; the scrubber matches ASCII-only case-insensitively (no `unicode-case` feature). |
| ~12:05 | Codex → Claude | d0dbabd4 | Shared-file request: `sha2` for workflow SHA-256 binding, in its later workflow commit; describes its fail-closed sidecar locking. |
| ~12:06 | Claude → Codex | 6431abec | **Agrees** `sha2`; suggests `sha2 = "0.10"` in the droneqa manifest only (already in the lock); I make no further Cargo edits after 6760d38. |

First commit touching shared files: 6760d38 (after df090d26). No commit calling `redact` or `pix4d::parse` until Codex's foundation commit exists.

## Agreed contracts

1. **Parser**: `pix4d::parse(bytes: &[u8], content_type: &str) -> ParseResult`; PDF gives a "not supported yet" diagnostic. `droneqa` takes `report` (a `ParseResult`, as `pix4dimport` returns) or `report_text` + `content_type` (default `text/plain`).
2. **History**: an explicit JSON sidecar path passed to `qareview`, append-only; records carry `schema_version`, `draft_id`, `mission_id`, `ruleset_version`, `analysis` (already redacted), `sensitive_fields`, state and timestamp.
3. **Redaction**: sensitive fields are a list of canonical mission names, default `["pilot"]`, and an unknown name is an error. `analyze` replaces observed/requirement with `[REDACTED]` when a finding's inputs include a sensitive field (`None` means every field, fail closed), then runs both through Codex's credential scrubber. The workflow rejects credential-like input and redacts output and audit details.

## Decisions and why

- **Strict mission/ruleset**: `deny_unknown_fields`, typed integers (so `true` or `-1` is not a count), ranges checked after reading. Blank top-level text is "not documented"; blank list entries are errors, as in pydantic.
- **Attachment patterns**: Rust `regex`, wrapped as `^(?:pattern)$` for Python `re.fullmatch` semantics, including alternation. It has no lookaround or backreferences, so those patterns fail ruleset validation rather than matching differently (tested). Patterns are capped at 1000 bytes and a 1 MiB compiled size.
- **Number display**: requirements print thresholds through `serde_json` (`<= 10.0`, `>= 3`), which matches Python's `repr` for the values the rules use, so the reference strings (`<= 0.25`, `<= 0.2499999`) carry over.
- **Scenario ruleset**: as in the reference `fixture_client.load_fixture`, the fixture ruleset with the scenario's keys replaced and its `control` merged. It lives inline in the tests, so no shared fixture file changes.

## Pending when Codex's foundation lands

- Rebase onto it; restore `scrub` (the local stub is never committed: `grep LOCAL-STUB` must be empty).
- Add a `qa*.rs` test pinning `pix4d::parse(valid.txt)` to the `valid_report()` fixture (no diagnostics, independence `None`).
- `cargo check -p cadcraft-engine`, then `cargo xtask ci`; replace the `~` times above with exact ones from `paseo logs`.
