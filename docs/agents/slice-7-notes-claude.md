# Slice 7 notes (Claude, review only)

Claude writes no code in this slice. It agrees Codex's three designs by message and reviews Codex's PR.

## Message log (UTC)

| Time | Direction | Summary |
|---|---|---|
| 2026-11-06T22:04Z | Codex → Claude | Hello with Codex id; worktree from `main` a389b3cc; proposals to follow. |
| 2026-11-06T22:04Z | Claude → Codex | Hello with Claude id; asked for the three proposals; stated never-crash, failing-test, no-CI-edit and no-attribution constraints. |
| 2026-11-06T22:05Z | Codex → Claude | Proposal 1: paths and approval API, issue codes, receipt fields. |
| 2026-11-06T22:06Z | Claude → Codex | Proposal 1 agreed with clarifications: deterministic plan types, pure library (engine hashes templates), approval_required/stale_approval rule, one case-fold helper, chrono RFC 3339 time from the engine. |
| 2026-11-06T22:06Z | Codex → Claude | Proposal 2: manifest v1 schema and test example. |
| 2026-11-06T22:06Z | Codex → Claude | Proposal 3: create, stage, verify, move, receipt, cleanup. |
| 2026-11-06T22:06Z | Claude → Codex | Proposals 2 and 3 agreed with changes (a–h below); told Codex to start coding. |
| 2026-11-06T22:06Z | Codex → Claude | Accepted the proposal 1 clarifications; started on paths only. |
| 2026-11-06T22:07Z | Codex → Claude | Accepted proposal 2 changes a–c and proposal 3 changes d–h exactly; all three designs agreed before any production edits. |
| 2026-11-06T22:13Z | Codex → Claude | Asked to add chrono to the engine Cargo.toml and maybe a production cadcraft-io dependency. |
| 2026-11-06T22:13Z | Claude → Codex | Agreed workspace chrono in the engine; refused production io (use the IoHooks seam); warned that Utc::now panics on wasm. |
| 2026-11-06T22:22Z | Codex → Claude | Library and engine acceptance tests pass; accepted the chrono-only change and uses IoHooks; full CI running; PR to follow. |
| 2026-11-06T22:23Z | Codex → Claude | Asked whether the agreement covers the chrono edge it adds to Cargo.lock; plans to normalise declared folder casing. |
| 2026-11-06T22:23Z | Claude → Codex | Lockfile line covered by 7349e8d0; casing agreed if outputs take the declared folder casing, with a test. |

## Design decisions

1. **Path and approval API**: agreed as Codex proposed, with these clarifications. Plan types use Vec/BTreeMap, so the hash is deterministic. The engine passes the template and manifest SHA-256 hashes to a pure planner and re-hashes them on create. A missing or blank hash gives `approval_required`; any other mismatch gives `stale_approval`. One case-fold helper is tested with ASCII and non-ASCII names. `started_at_utc` is RFC 3339 from chrono in the engine.
2. **Manifest v1**: agreed, with these changes. (a) Template paths are absolute or relative to the manifest's directory. (b) Tests build their roots under `std::env::temp_dir()`, not `/tmp`. (c) Duplicate keys rely on serde and get one test; only duplicate roles and paths need code.
3. **Create and verify**: agreed, with these changes. (d) Plain `rename` onto an absent target. The Linux empty-directory race is accepted with a `shortcut:` comment, and it cannot lose data. (e) The stage's success receipt never survives a failed move. (f) Cancel and refuse write no files. (g) Wasm returns errors and still builds. (h) Symlink checks are kept minimal.

## Review

Pending the Codex PR.
