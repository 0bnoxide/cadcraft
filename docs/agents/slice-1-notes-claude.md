# Slice 1 notes: Claude (Session 1)

## What I did
- Started on 2026-11-04 at about 05:30 UTC. PR #3 opened at 05:52 UTC.
- I worked in my own worktree, `.claude/worktrees/claude-rtk`, on `slice1/claude-rtk`. Codex already had `.claude/worktrees/codex-levels` in the same clone. Each worktree has its own `target/`, so the two builds didn't contend for a lock.
- I ported `import_rtk_survey.py`, `validate_rtk_survey.py`, `assign_qa_flags` and `rtk_control_check.py`, along with their reference tests, and added the `rtkimport` and `controlcheck` commands.

## What worked
- The ownership table was clear enough that I never needed to open a Codex file.
- No new dependencies. A 20-line CSV splitter in `rtk.rs` avoided the conflict that two agents each adding `csv` to `crates/survey/Cargo.toml` and `Cargo.lock` would have caused.
- `Finding` has no category field, so each message starts with the reference's QA category (`guessed_coord_order: …`). The tests assert on that prefix.

## What broke or caused friction
- **CI is red on `main`.** `cadcraft-ui-egui i18n::tests::chinese_menus_and_commands_are_fully_translated` fails because `Import Handoff Surface` has no `zh.tsv` entry. Every new command label adds to that failure. `zh.tsv` is in neither agent's ownership list, so the "CI passes on each PR head" criterion can't be met without a scaffold change.
- **Ownership versus dependencies.** The survey doc treats `crates/survey/Cargo.toml` as scaffold, but it also says to add new dependencies there. Two agents doing that would conflict.
- **CLAUDE.md versus the slice doc.** CLAUDE.md says "push to `main`", while the slice doc requires PRs. I followed the slice doc.
- **Auto-mode on this PC** allowed `git push` and `gh pr create`. It blocked the `gh api` calls that add Greg as assignee and request his review, so he has to do those by hand.
- **The AutoGIS-Civil3D write hook** applies to every file write from this session, including writes in the cadcraft clone. I had to claim `slice1/claude-rtk` in that repo's coordination registry before I could write any file.
- **Leftover file.** An untracked `CLAUDE_TEST.txt` from an earlier folder-access test sits in the main clone. I didn't commit it.
- My first CI run failed on clippy `field_reassign_with_default`. That cost one extra full CI run of roughly 10 minutes.
- **Auto-mode blocked the force push** after I rebased #3 onto `main` once #4 had merged. It flagged `git push --force-with-lease` as "Git Destructive". I didn't work around the block. The planning thread redid the rebase and force-pushed from its cloud clone, then merged. The rebased branch passed all 7 CI steps on this PC before that.
- **The second PR always needs a rebase** in this process, and on this PC that means a person or a cloud session has to do the force push. Next time, either allow `--force-with-lease` on `slice1/*` branches or have the second PR merge `main` in rather than rebasing.

## Review
- **Codex reviewed PR #3** at `dcd2982` and found three real issues, now fixed in `a8f1459` with regression tests:
  - The row cap didn't bound memory, because one huge line could still allocate. Lines, fields per line and total fields are now capped before allocating.
  - The RMSE overflowed for large finite residuals. It now uses a scaled norm.
  - The `controlcheck` output could lose a final flush error. It now flushes explicitly and writes atomically.
- **Codex's chrono and zh.tsv edits.** PR #4 also edits shared files (`crates/survey/Cargo.toml`, `Cargo.lock`, `zh.tsv`), with Greg's approval in the Codex session. That approval wasn't visible to this session, which had been told to wait for a separate fix on `main`. The two sessions only saw each other's decisions through PR prose.
- **My review of PR #4.** I ran its 22 survey tests, its 5 command tests and the zh test at its head, and checked `level_loop.rs` against the reference `process_level_loop`. My comment is on PR #4.

## Timing
- Orientation (finding the clone, reading the docs and reference): about 10 minutes.
- Writing the code and tests: about 10 minutes.
- CI runs on Windows: five full `cargo xtask ci` runs in all. The first, with the cold egui and wasm build, took about 10 minutes, and the later ones were faster. CI was most of the elapsed time.
- From the start to both PRs merged: about 2 h 20 min (05:30 to 07:50 UTC), much of it spent waiting on the other session or on merges.

## Result
- After #4 and #3 merged, `cargo xtask ci` on `origin/main` (`6062e84`), run in a clean detached worktree on this PC, passed all 7 steps (fmt, clippy, test, i18n, assets, layers, wasm).
- Pass criteria: both sessions used separate worktrees of the same clone. No PR touched a file the other agent owned, apart from the shared files Greg approved in #4. Each PR got a review from the other agent, and every comment was answered. CI passed on each PR head and on `main`. Neither merge needed Greg to resolve a conflict by hand.
