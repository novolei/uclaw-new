# gbrain Sprint 2.2 — launcher script + paths.json + init recovery script

**Status:** ready to merge (assuming manual verify is green on Mac).
**Branch:** (set at finishing-branch time)
**Base:** `main`
**Predecessor:** PR #205 (Sprint 2.1 init-fix — established `ensure_bundled_gbrain_initialized` + the real `.gbrain/brain.pglite/` layout).

## Why this PR exists

PR #205 closed the original "gbrain serve exits with No brain configured"
bug by adding `ensure_bundled_gbrain_initialized` to Stage 3 boot. Two
follow-on gaps remained:

1. **Discoverability.** Other Cowork sessions, debug scripts, and CLI
   users had no way to invoke the bundled gbrain without grepping uClaw
   internals to figure out where `bun` and `gbrain/src/cli.ts` live (paths
   differ between dev mode and the release `.app` bundle).

2. **Manual init / reset path.** PR #205 made auto-init bulletproof, but
   power-user workflows (reset a brain, verify fresh-init in CI, init
   without launching uClaw) had no first-class entry point.

This PR ships both:

- Every boot writes `~/.uclaw/gbrain/run.sh` + `~/.uclaw/gbrain/paths.json`.
- `scripts/init-gbrain.sh` mirrors the existing `setup-bun-runtime.sh`
  style for manual init / `--force` reset.

## What changed

| File | Diff |
|---|---|
| `src-tauri/src/app.rs` | +`pub fn write_gbrain_launcher_files(gbrain_home, bun_path, entry_path)` + private `shell_quote_path` helper + 4 unit tests |
| `src-tauri/src/main.rs` | +5 lines (call site inside the existing Stage 3 `if let (Some(bun), Some(entry))` arm, BEFORE `ensure_bundled_gbrain_initialized`, passing `&gbrain_home`) |
| `scripts/init-gbrain.sh` | new file, executable |

## How the launcher works

Boot order (Stage 3, inside the `Some(bun), Some(entry)` arm):

1. `AppState::write_gbrain_launcher_files(&gbrain_home, bun, entry)` — best-effort.
2. `ensure_bundled_gbrain_initialized(bun, entry, &gbrain_home)` (from PR #205).
3. `seed_bundled_gbrain(bun, entry, &gbrain_home)` (from PR #205).

`run.sh`:
- Shebang: `#!/usr/bin/env bash`
- Exports `GBRAIN_HOME=<gbrain_home>` (NOT `PGLITE_DATA_DIR` — gbrain reads its
  layout from `$GBRAIN_HOME/.gbrain/config.json`, written by `gbrain init`).
- `exec <bun> <gbrain_cli.ts> "$@"` — forwards all args.
- Single-quoted paths (`shell_quote_path`) so spaces AND embedded single
  quotes work.
- chmod 0o755 best-effort on Unix.

`paths.json`:
```json
{
  "uclaw_version": "0.1.0",
  "bun_path": "<abs path to bunembed/bun>",
  "gbrain_entry": "<abs path to gbrain-source/src/cli.ts>",
  "gbrain_home": "<HOME>/.uclaw/gbrain",
  "brain_dir": "<HOME>/.uclaw/gbrain/.gbrain/brain.pglite",
  "config_json": "<HOME>/.uclaw/gbrain/.gbrain/config.json",
  "generated_at_ms": 1747584000000
}
```

`brain_dir` + `config_json` reference the **real** PGLite layout `gbrain
init --pglite` produces — NOT the dead `pgdata/` directory that the
pre-PR-205 code tried to use.

## How `init-gbrain.sh` works

Three flags: `--force`, `--yes`, `--help`. Style mirrors `setup-bun-runtime.sh`.

Pre-flight checks `src-tauri/bunembed/bun` + `src-tauri/gbrain-source/src/cli.ts`
exist; on failure points the user at `scripts/setup-bun-runtime.sh` +
`scripts/setup-gbrain-source.sh`.

If `${BRAIN_DIR}/PG_VERSION` already exists without `--force` → exit 0
with a "already initialized" note. With `--force` → confirm (unless `--yes`),
`rm -rf brain.pglite`, then re-init.

Otherwise: `GBRAIN_HOME=~/.uclaw/gbrain bunembed/bun gbrain-source/src/cli.ts
init --pglite --yes`. Post-init verify checks the PG_VERSION marker landed;
if not, error out (catches the "init exited 0 but wrote elsewhere" bug class
— same defense-in-depth as `ensure_bundled_gbrain_initialized` from PR #205).

## How to verify locally

```bash
# Build (full binary, not just --lib)
cd src-tauri && cargo build && cargo test --lib gbrain_launcher_tests

# Launcher files written next boot. Trigger with cargo tauri dev:
cd src-tauri && cargo tauri dev > /tmp/uclaw-dev.log 2>&1 &
# Wait for [Stage 3] in logs, then:
ls -la ~/.uclaw/gbrain/run.sh ~/.uclaw/gbrain/paths.json
cat ~/.uclaw/gbrain/paths.json
~/.uclaw/gbrain/run.sh --help   # should print gbrain's own help

# init-gbrain.sh:
./scripts/init-gbrain.sh --help   # prints usage, exits 0
# Safe-test: --force prompts before destroying — type 'n' to abort cleanly:
./scripts/init-gbrain.sh --force
```

## Notes on Task 1 follow-on commits

Two review-driven follow-on commits ride on top of the main task commits:

- **`58fb44e` test(gbrain): cover shell_quote_path single-quote escape** —
  Code review on Task 1 flagged the non-trivial escape branch (`'` → `'\''`)
  as untested. Added one targeted test that uses `it's` as a path component
  and asserts the escape lands in `run.sh`.
- **`f905201` fix(gbrain): take gbrain_home directly to avoid state_ref
  capture in async spawn** — Task 1's original `write_gbrain_launcher_files(
  &state_ref.data_dir, ...)` call site INSIDE the
  `tauri::async_runtime::spawn(async move { ... })` closure triggered
  `error[E0597]: app_handle does not live long enough` on `cargo build`
  (full binary). Refactor function signature `data_dir → gbrain_home`;
  caller now passes `&gbrain_home` (the owned `PathBuf` already in scope
  from `let gbrain_home = state_ref.data_dir.join("gbrain");` at line 408).
  Matches the existing PR-5 `db_for_mcp = state.db.clone()` pattern of
  pre-cloning into owned types before the spawn.

Misdiagnosed as pre-existing during the first round; verified by
`git checkout 4de805e -- src-tauri/src/main.rs src-tauri/src/app.rs` (pure
PR #205 baseline) — `cargo build` is CLEAN there. So the regression was
in Task 1's contribution and is fixed in the same PR.

`74a82a` is a small style polish on init-gbrain.sh from code review
(local_version → existing_version rename, `$(basename "$0")` consistency,
confirm() placement matching setup-bun-runtime.sh).

## Commits (bisectable)

| # | sha | purpose |
|---|-----|---------|
| 1 | fe2798b | task 1: launcher files (app.rs + main.rs + 3 tests) |
| 2 | 58fb44e | task 1 follow-on: single-quote escape test (review finding) |
| 3 | 9a0964d | task 2: scripts/init-gbrain.sh |
| 4 | f905201 | task 1 follow-on: E0597 fix via signature refactor (review finding) |
| 5 | 174a82a | task 2 follow-on: readability polish (review finding) |
| 6 | <this commit> | task 3: hand-off doc + commit body + plan |

## Files index

```
docs/superpowers/plans/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script.md   (this PR's plan)
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-2-launcher-and-init-script-handoff.md ← this
docs/superpowers/handoff/COMMIT_GBRAIN_SPRINT_2_2.txt                              (PR squash/merge body)
docs/superpowers/handoff/2026-05-18-gbrain-sprint-2-1-init-fix-handoff.md          (PR #205 hand-off — predecessor)
```
