# Slice 3 notes: Claude (mission, ruleset, QA rules, `droneqa`)

Agent id `aaf07392-8686-403a-9168-ece6112f9ee7`; Codex `0d8d85c3-3711-4535-bfc4-cde5ae334a84`. Reference: AIGIS3D at `C:\Users\ichbi\dev\AIGIS3D`, read only (the slice doc's path lacks `dev`).

## Message log (UTC, 2026-11-04)

Rows are in exact send/receive order (the order `paseo logs` shows); `paseo logs` carries no timestamps, so times are from my shell clock where I measured them and `—` otherwise. Commit times are from git.

| Time | Direction | Message id | Summary |
|---|---|---|---|
| 10:19 | Claude → Codex | 04114147 | Hello; redaction boundary proposal (item 3); expected parser shape. |
| — | Codex → Claude | c337baca | Crossed: proposes `parse_report(&[u8], &str) -> ParseResult` and an append-only JSON sidecar history with the draft schema. |
| — | Claude → Codex | ace85317 | Agrees items 1 and 2; asks for `sensitive_fields` in the draft record. |
| — | Codex → Claude | 26da0240 | **Agrees item 3** (redaction, `redact(&Value, &[String]) -> Value`, fail-closed `None`); renames parser to `pix4d::parse`. |
| — | Claude → Codex | a94ec918 | Confirms all three items; names my public API (`analyze`, `summarize`, `Mission`, `Ruleset::from_json`). |
| — | Codex → Claude | a982cfff | Confirms `sensitive_fields` in the draft schema; all three contracts agreed. |
| 10:20 | Claude → Codex | e941d8fc | (A) staged merge order; (B) shared-file request: `regex` dependency. |
| — | Codex → Claude | df090d26 | **Agrees (A) and (B)**: real full-match semantics; reject (not reinterpret) unsupported Python syntax. |
| — | Codex → Claude | 5f76ca13 | Asks for the regex dependency as its own commit. |
| (git) 10:24 | — | — | Commit 6760d38 (regex). |
| — | Claude → Codex | e8e33bcb | Dependency commit 6760d38; CI running. |
| — | Codex → Claude | 57ed821d | Its reviewer blocked moving onto 6760d38 pending operator approval. |
| 11:16 | Claude → Codex | 923bc28f | 6760d38 pushed, `cargo xtask ci` 7/7; waiting for the foundation SHA. |
| — | Codex → Claude | 8f0219f1 | Ack; operator approval for the rebase still pending; will send the foundation SHA. |
| 11:19 | Claude → Codex | 9221c4ce | Proposal: `qareview draft` takes the `droneqa` inputs and calls `analyze` itself; states the `valid.txt` parse my fixtures assume. |
| — | Codex → Claude | 230a2ce3 | **Agrees** `qareview draft` inputs (same as `droneqa`, report XOR report_text, draft calls `analyze`); review/commit bind the stored draft; its parser targets my `valid.txt` expectations (unverified). |
| — | Claude → Codex | 407fab21 | Agrees the review/commit binding; all four interfaces settled; holding for the foundation SHA. |
| — | Codex → Claude | 7c25a0cc | Operator approved the rebase; Codex now on 6760d38; foundation SHA after its mapper and CI. |
| — | Codex → Claude | e1d7fe67 | Its parser on `valid.txt` matches my `valid_report()` exactly, with zero diagnostics; full CI rerun pending; the scrubber matches ASCII-only case-insensitively (no `unicode-case` feature). |
| — | Codex → Claude | d0dbabd4 | Shared-file request: `sha2` for workflow SHA-256 binding, in its later workflow commit; describes its fail-closed sidecar locking. |
| — | Claude → Codex | 6431abec | **Agrees** `sha2`; suggests `sha2 = "0.10"` in the droneqa manifest only (already in the lock); I make no further Cargo edits after 6760d38. |
| (git) 14:26 | — | — | Codex commit 2f67906 (parser, redaction). |
| — | Codex → Claude | e43abbd6 | Foundation pushed: 2f67906 on 6760d38, its CI 7/7; `valid.txt` contract verified; asks for a review; agrees `sha2` in the droneqa manifest. |
| (git) 14:30 | — | — | My commit 6750e94 (mission, QA, droneqa). |
| — | Claude → Codex | a925e0a0 | PR up: #10 at 6dd9e4d, review request; review done on 2f67906 (accepted, one non-blocking note, on #10). |
| — | Codex → Claude | 3ebbb207 | Review of #10 (in progress): unbounded recursion in `Mission::from_value` (clone/trim before rejection); serde errors echo rejected values. |
| — | Codex → Claude | 0bb90b40 | Review done on #10 at 6dd9e4d: the two boundary issues above block acceptance; ParseResult decode errors too. |
| — | Codex → Claude | c2dab293 | Asks for a shared `qa::validate_report(&ParseResult) -> Result<(), String>` (negative RMSE, calibrated > total, bounded strings). |
| — | Codex → Claude | b359cd60 | Refines history to JSON Lines (one immutable event per line, SHA-256 chain, caps, lock); asks to confirm. |
| — | Codex → Claude | 76273038 | Reproduces negative report RMSE passing `ACCURACY.RMSE_Z`; addendum on #10. |
| — | Codex → Claude | cf7079c0 | Its uncommitted workflow passes the real CLI chain pix4dimport → droneqa → qareview draft/review/commit/history. |
| (git) — | — | — | My fix commits c84c436 (recursion bound, structural errors) and 5522d42 (`validate_report`). |
| — | Claude → Codex | c7bce2aa | Fixes pushed at 5522d42 (answers on #10); `validate_report` signature; **agrees** JSONL history. |
| — | Codex → Claude | 7048d63b | Sonar secrets scan flags the synthetic `token=…` test literal at `droneqa_check.rs:166`; Codex stops until it is removed. |
| — | Claude → Codex | 201e195c | Cleared: the synthetic marker lost its `token=` prefix; all my files scan clean; head 40d2c19, CI 7/7. |
| — | Codex → Claude | a2715d62 | Review closed on #10 at 40d2c19: all three fixes accepted (verdict on #10); its workflow uses `check_shape` and `validate_report`; its stacked PR is next. |
| — | Codex → Claude | 42b3e205 | PR up: #11 at f69a659, stacked on 40d2c19; review request with the command params. |
| — | Claude → Codex | 75b576c7 | Review done on #11: accepted; independent CLI chain green; two non-blocking findings (stale lock message, `content_type: null`). |
| — | Codex → Claude | ccf6a389 | Both findings fixed locally: the HISTORY_BUSY message explains verified lock removal; `content_type: null` = default. No PID/age lock breaking (reason: it could release an active writer). SHA to follow after CI. |
| — | Codex → Claude | 1afb4fdc | Fixes pushed: #11 head 9c9f3e8, its CI 7/7, scans clean. |
| — | Claude → Codex | 0a571f34 | Closeout: #11 accepted at 9c9f3e8 (comment on #11); my final head is this notes commit. |

Commits, in order: 6760d38 (regex; shared files, after df090d26), Codex's foundation 2f67906 (after 26da0240), then my mission/qa/command commit (after all four agreements and Codex's foundation SHA in e43abbd6).

## Agreed contracts

1. **Parser**: `pix4d::parse(bytes: &[u8], content_type: &str) -> ParseResult`; PDF gives a "not supported yet" diagnostic. `droneqa` takes `report` (a `ParseResult`, as `pix4dimport` returns) or `report_text` + `content_type` (default `text/plain`).
2. **History**: an explicit JSON sidecar path passed to `qareview`, append-only; records carry `schema_version`, `draft_id`, `mission_id`, `ruleset_version`, `analysis` (already redacted), `sensitive_fields`, state and timestamp.
3. **Redaction**: sensitive fields are a list of canonical mission names, default `["pilot"]`, and an unknown name is an error. `analyze` replaces observed/requirement with `[REDACTED]` when a finding's inputs include a sensitive field (`None` means every field, fail closed), then runs both through Codex's credential scrubber. The workflow rejects credential-like input and redacts output and audit details.

## Decisions and why

- **Strict mission/ruleset**: `deny_unknown_fields`, typed integers (so `true` or `-1` is not a count), ranges checked after reading. Blank top-level text is "not documented"; blank list entries are errors, as in pydantic.
- **Attachment patterns**: Rust `regex`, wrapped as `^(?:pattern)$` for Python `re.fullmatch` semantics, including alternation. It has no lookaround or backreferences, so those patterns fail ruleset validation rather than matching differently (tested). Patterns are capped at 1000 bytes and a 1 MiB compiled size.
- **Hostile shapes** (Codex's review, 3ebbb207): `check_shape` walks input iteratively and rejects nesting beyond 16 levels or more than 200 000 values before anything clones, trims, deserializes or drops it, since `catch_unwind` cannot recover from a stack overflow. This covers `Mission::from_value`, `Ruleset::from_value`, `normalize_mission` inputs and `droneqa`'s `report`. The regression test uses 500 000 levels, built and leaked by hand because `json!` and `Drop` recurse too.
- **Errors never echo input** (3ebbb207): serde messages lose their quoted and back-ticked spans. A mission error names the failing field, found by deserializing each field alone. Unknown keys are named only when they're plain identifiers, and list errors give an index (`required_metrics[2]`), not the value.
- **Report validation** (c2dab293, 76273038): `qa::validate_report` applies the reference `QAMetrics`/`ReportEvidence` rules to a caller-supplied `ParseResult`: finite non-negative RMSE and GSD, residuals with a qualified provenance and unit, calibrated ≤ total, and text and diagnostics bounded. `droneqa` rejects an invalid `report` with an error. `analyze` also ignores one (fail closed, reported as no usable report), so a direct caller can't get a PASS from it either. The same validator serves Codex's workflow.
- **Number display**: requirements print thresholds through `serde_json` (`<= 10.0`, `>= 3`), which matches Python's `repr` for the values the rules use, so the reference strings (`<= 0.25`, `<= 0.2499999`) carry over.
- **Scenario ruleset**: as in the reference `fixture_client.load_fixture`, the fixture ruleset with the scenario's keys replaced and its `control` merged. It lives inline in the tests, so no shared fixture file changes.

## Verification

- `cargo xtask ci`: 7/7 on 6760d38 and on my mission/qa/command head (Windows).
- Through the CLI (`cadcraft-cli run --cmd "droneqa {mission, ruleset, report_text: valid.txt}"`): pass → PASS, fail → FAIL, insufficient-data → INSUFFICIENT_DATA.
- `pix4d::parse(valid.txt)` is pinned to the scenario fixture (`real_parser_reads_valid_txt_as_the_scenario_fixture_assumes`).
- Pitfall: `xtask` and fixture-reading test binaries bake in `CARGO_MANIFEST_DIR`; a target dir reused after deleting a worktree fails with `os error 267` / `NotFound` until those packages are cleaned.

## Review of Codex's #11 (f69a659)

Accepted with no blockers (posted on #11). I ran the independent CLI chain, one `cadcraft-cli run --cmd` process per step: `pix4dimport` (`valid.txt`, 0 diagnostics) → `droneqa` (PASS) → `qareview` draft (analysis identical to the preview) → APPROVED → COMMITTED → history `[DRAFT, APPROVED, COMMITTED]`. The pilot name never reaches the history bytes. Adversarial runs gave SOURCE_CHANGED, PAYLOAD_HASH_MISMATCH, SENSITIVE_INPUT, ALREADY_COMMITTED and INVALID_HISTORY (edited line). Non-blocking: a stale lock left by a killed writer gives HISTORY_BUSY forever with a "retry" message; `pix4dimport` rejects `content_type: null`.

## Review of Codex's foundation (2f67906)

Checked `parse` against `parsers.py`: exact `text/plain`, PDF diagnostic, byte limit, duplicate and unknown lines. Checked `redact` against `redaction.py`: key list, the four string scrubs, depth cap, no mutation. No blocking issue. Non-blocking: `scrub` compiles its four regexes on every string; a `LazyLock` would help on large audit histories.
