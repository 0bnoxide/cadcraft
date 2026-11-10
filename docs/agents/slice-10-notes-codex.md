# Slice 10 implementation and verification

## Scope and plan

Implement `figureexport` first on `slice10/codex-figures`, then stack
`packagedeliverables` and fixes for #44/#45 on `slice10/codex-package`.
No merges. References are read only; no CIM reference was opened.

Reuse host-free modules in `crates/proposal`, existing path/approval helpers,
temporary engine sessions and the registered plot command. No new plotting
route, crate, manifest version, lab-data behavior or handoff changes.

- [x] First write failing schema/series tests, then implement closed parsing,
  four mode orders, deduplication and safe names in `proposal::figures`.
- [x] Write engine tests for preparation, QA, deterministic preview, approval,
  four outputs, versioning, unchanged source/session and failed publication;
  implement `figureexport` with shared staging helpers and registration.
- [ ] Exercise the CLI with synthetic drawings; run `cargo xtask ci`, inspect
  the complete diff, publish PR 1 and request exact-head review by message.
- [ ] On the stacked branch, test/implement closed profile parsing, sheet
  selection, reference closure, ZIP/manifest verification and atomic publication.
- [ ] Reproduce #44/#45 with regressions and make the agreed narrow fixes.
- [ ] Run CLI checks and `cargo xtask ci`; publish PR 2, request review and
  address every finding. Record actual hosted checks; leave merging to owner.

Review focus: sanitization collisions; hidden missing references; unsupported
checks reported explicitly; stale source/configuration approval; a failure on
the second publish removes only artifacts created by this run.

## Design agreement

Proposal message `40ceebab-f61c-44a1-b87d-ae894c492784` and reply
`a52ac800-2d2a-4a45-99ac-08185bd37f24` agreed parts 1 and 2 before code.
Part 3 was confirmed by reply `78a98b30-b5d5-431f-ac16-18085bf5e313`:
reuse `Project::open` with `manifest_path`, `proposal_root` and `profile_path`;
no configuration discovery scan.

Agreed changes: within-batch filename collisions refuse instead of versioning;
empty matrices/labels refuse; built-in text keys cannot be overridden; plans
contain no timestamps/temp paths and retain all job QA on refusal. Only the
largest content viewport changes (tie: lowest handle), recorded in the plan.
The package and ZIP reserve the same version, and unknown naming tokens refuse.

## Paseo message log (UTC)

- 2026-11-10 02:29 received: reviewer hello and id, asks for three proposals.
- 2026-11-10 02:31 sent: acknowledged role and two stacked PRs; initial ACK
  omitted the sender id; every subsequent message includes it.
- 2026-11-10 02:37 sent: all three concrete designs in one message (`40ceebab`).
- 2026-11-10 02:40 received: all three agreed with changes (`a52ac800`).
- 2026-11-10 02:41 sent: accepted all changes; confirmed existing Project::open
  needs manifest_path and requested agreement to reuse that input for packaging.
- 2026-11-10 02:42 received: confirmed manifest_path and the existing project helper
  (`78a98b30`); all three designs agreed before dependent implementation.
- 2026-11-10 02:52 sent: initial real-plot integration checks passed; remaining
  focused tests and full CI next (`b33475b5`).
- 2026-11-10 02:53 received: review session reported its usage limit, resetting at
  00:10 Mountain time. No review has been performed or implied.

Message times above were recorded on receipt during this session, rounded to
the minute; message ids are the exact Paseo receipts.

## Evidence

- Base `main` and `origin/main`: `6a5ec6616a19f79a1b40b81f9633448e8aef80d6`.
- Required files were scanned with `sonar analyze secrets` before reading.
  Source scans require access to the scanner's local authentication in this
  environment; elevated scans passed. No secret was reported.
- Baseline `cargo test -p cadcraft-proposal --offline`: 10 integration tests
  passed, using the private `target-slice10-codex` build directory.
- Unrelated checkout files were preserved. Neither reference repository changed.
- Feature claim: [#48](https://github.com/0bnoxide/cadcraft/issues/48).
- Baseline CI launcher failure filed separately as
  [#49](https://github.com/0bnoxide/cadcraft/issues/49). Windows cannot replace a
  running `xtask.exe` during the workspace test build. An ignored Cargo runner
  copies it before launch; no CI or tooling source changed.
- First full `cargo xtask ci` through the copied launcher passed all seven gates.
  An additional xref-layer regression then exposed preparation before xref
  loading; the ordering is corrected and the final gate will be rerun.
- CLI: four jobs, identical preview JSON, four PDFs, verified SHA-256s, unchanged
  source bytes, and four `_v2` outputs all passed. Each PDF's decoded content
  streams match a separate plot authored with the expected literal replacement
  text (the default drafting font uses vector strokes).
- Focused forced second-publish failure removes the already published first PDF
  and staging folder; the sole disk receipt is `Failed`.
- Final figure-export gate after the xref ordering fix: all seven steps passed.
  The retry first required rebuilding only the private target's ash package
  after Cargo reported a missing cached rlib. No dependency or CI source changed.
- Contributor entry was checked; the existing username-only credit preference
  is retained and no contributor metadata changed.
