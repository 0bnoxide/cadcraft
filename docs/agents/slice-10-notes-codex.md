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
- [x] Exercise the CLI with synthetic drawings; run `cargo xtask ci`, inspect
  the complete diff, publish PR 1 and request exact-head review by message.
- [x] On the stacked branch, test/implement closed profile parsing, sheet
  selection, reference closure, ZIP/manifest verification and atomic publication.
- [x] Reproduce #44/#45 with regressions and make the agreed narrow fixes.
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
- 2026-11-10 03:17 sent: PR #50 exact head fbc4835894934675015feb6dc1a4667180ecfd49,
  full review requested; local CI and CLI evidence summarized (`3d63568b`).
- 2026-11-10 03:17 received: review session again reported the usage limit.

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

## Packaging and follow-ups

Stacked branch: `slice10/codex-package`, based on PR #50 head fbc48358.
Feature claim: [#51](https://github.com/0bnoxide/cadcraft/issues/51).

The profile defaults to all sheets, `{number}_{title}.pdf`, no traveling drawings
and no extra files. The only naming tokens are number, title and role. Extras
use figures/ (figure_pdf, figure_png), data/ (data_csv, boring_log), qa/
(qa_report), or the package root (other). Paths and collisions use the existing
proposal rules. Both the folder and ZIP reserve the same Package[_vN] stem.

The generated checksums.json contains version 1 and a files array with path,
role, sheet_role, sha256 and size. It lists every payload. Its own digest is
recorded in the receipt, and the ZIP contains exactly the payload and manifest.
ZIP entry names and every decompressed entry hash are verified on reopen.
Limits are 4,096 entries including the manifest, 512 MiB per entry and 2 GiB
total; the existing ZIP dependency writes stored entries, with no new version.
Drawings and references travel as unchanged bytes under drawings/ with their
proposal-relative hierarchy; references outside the proposal are refused when
drawings travel. No drawing is repathed, edited or saved by this command.

- Full local `cargo xtask ci`: all seven gates passed; 4,092 tests passed,
  28 skipped. The local copied-launcher workaround for #49 remains untracked.
- CLI on a fresh synthetic newproposal project: two selected sheet PDFs;
  unchanged source drawing hashes; exact ZIP entry set; each payload hash
  matches checksums.json; ZIP hash matches receipt; _v2 reserves folder and
  ZIP together; an edited sheet invalidates approval; missing layout refuses
  preflight and writes no second package.
- Independent integration checks reopen the actual ZIP and compare entries to
  the plan and digests to the manifest. Folder-only and ZIP-only name occupancy,
  colliding extras and reserved checksum destination are covered. Linux CI
  additionally exercises linked-extra refusal.
- Forced second publish failure removes the published folder and staging area,
  leaves no ZIP or success receipt, and persists a Failed receipt.
- #44: lock owner PID and UTC start are stored and reported; existing/legacy
  locks remain intact and are never auto-broken.
- #45: one shared link check; empty retention folders are removed only when
  created by this run; Failed intake/configuration receipts persist; a
  register-only intake creates no host stage or retained drawing; XY-derived
  extents preserve the saved finite Z range. Red/green regressions cover each
  behavioral fix and preservation of pre-existing retention folders.
- PR #50 hosted Linux and Windows gates both passed at fbc48358. SonarCloud
  was skipped by its existing workflow. The requested full review is pending
  the review session's quota reset; no approval or review is claimed.
