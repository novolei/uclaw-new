# 阶段 5 SP3 — Shadow Git Checkpoint Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Give the agent crash-safe rollback of its edits via a **shadow git checkpoint store** — snapshots of the working tree taken (once per turn, before the first mutating tool call) into an out-of-project git store that never touches the user's `.git`. Port the core of hermes's `checkpoint_manager.py` (ensure_checkpoint + restore + list); surface rollback via a manual IPC command. Scope locked: **core depth + manual IPC** (no prune/auto-prune/legacy — deferred).

**Architecture:** New `agent/code_checkpoint.rs` — a `CheckpointStore` over `uclaw_home/checkpoints/` using `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE` env (isolated from user gitconfig). `ensure_checkpoint(working_dir, turn_token)` dedups per turn + commits a snapshot; `restore(working_dir, commit, file?)` does whole-tree/single-file rollback via `git checkout`; `list(working_dir)` enumerates a project's checkpoints. Hooked into the dispatcher's mutating-tool path. Two IPC commands (`code_checkpoint_list`, `code_checkpoint_restore`). Best-effort: a checkpoint failure never blocks the tool.

**Tech Stack:** Rust, `std::process::Command` (git), `uclaw_utils_home::uclaw_home_pathbuf()`, `anyhow`, `tracing`. Reference: `/Users/ryanliu/Documents/hermes-agent/tools/checkpoint_manager.py` (port the core; ignore prune/legacy). No new deps.

---

## Source-of-truth references

- **hermes `tools/checkpoint_manager.py`** — port these: `ensure_checkpoint` (623, per-turn dedup via a set + skip root/home), `_git_env` (236, the isolation env — port EXACTLY), `_take` (the snapshot: `git add -A` + `git commit-tree`/`commit` into the store, returns commit hash), `restore` (761, `git checkout <commit> -- <target>`), `list_checkpoints` (657), `_store_path`/`_index_path`/`_ref_name`/`_project_hash` (path layout), `_validate_commit_hash`/`_validate_file_path` (input validation), `_run_git` (273, the subprocess wrapper). **Ignore**: `prune_checkpoints`, `maybe_auto_prune_checkpoints`, `clear_all`, `clear_legacy`, `_migrate_legacy_store`, `store_status`, `format_checkpoint_list`.
- `app.rs` — `uclaw_utils_home::uclaw_home_pathbuf()` (data dir). `state.data_dir`. Dispatcher / mutating-tool execution path.
- `agent/dispatcher/mod.rs` — `tool_dispatcher` / `execute_tool` — the mutating-tool execution hook point. **Recon**: where a tool call is executed + whether there's a per-turn token (ReasoningContext turn id) for dedup + how mutating-vs-readonly is classified.
- `agent/` SafetyManager / `is_mutating` — the mutating-tool classification (edit / write_file / shell-with-writes). **Recon**: `grep -rn "is_mutating\|SafetyManager\|MutationKind\|fn.*mutat" src-tauri/src/agent/`. Reuse it to decide which tool calls trigger `ensure_checkpoint`.
- uClaw git-spawn idioms to mirror: `world/adapters/git.rs`, `tauri_commands_git.rs`, `agent/gep/git_integration.rs` — for the `Command::new("git")` pattern (arg quoting, output capture, error handling).
- `tauri_commands.rs` + `main.rs` `invoke_handler!` — IPC command def + registration.

---

## CRITICAL facts

1. **Never touch the user's `.git`.** All git ops set `GIT_DIR=<store>`, `GIT_WORK_TREE=<working_dir>`, `GIT_INDEX_FILE=<store>/indexes/<hash>`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `GIT_CONFIG_NOSYSTEM=1` (port `_git_env` exactly — the config isolation prevents gpg-sign/credential-helper prompts hanging a snapshot). The shadow store is a bare-ish object DB under `uclaw_home/checkpoints/`; the working tree is the user's project, but commits land in the shadow GIT_DIR, **not** `<project>/.git`.
2. **Per-turn dedup.** `ensure_checkpoint` snapshots at most once per (turn, working_dir). The dispatcher advances a turn token each turn; the store holds a `HashSet` of `(turn_token, abs_dir)` (or clears the set at turn start). Skip `/` and `$HOME` (too broad).
3. **Best-effort, never blocks.** `ensure_checkpoint` returns `bool` (taken or not) and **never propagates an error** — a git failure / missing git logs at `debug` and returns false; the mutating tool proceeds regardless. Snapshots are a safety net, not a gate.
4. **Manual rollback only.** No automatic restore-on-error (deferred). Rollback is explicit via the IPC command.
5. **Bare snapshot via `commit-tree`** (preferred) avoids moving HEAD / branch state in the working tree's own git — use `git add -A` to the shadow index, `git write-tree`, `git commit-tree`, update the per-project ref. Port hermes's `_take` mechanism.

---

## File Structure

| File | New/Mod | Purpose | LoC |
|---|---|---|---|
| `agent/code_checkpoint.rs` | new | `CheckpointStore` (ensure_checkpoint / restore / list) + git-env + path layout + validation + `run_git` + tests | ~560 (incl. ~220 tests) |
| `agent/mod.rs` | mod | `pub mod code_checkpoint;` | +1 |
| `agent/dispatcher/mod.rs` (or the tool-exec site) | mod | call `ensure_checkpoint(workspace_root, turn)` before a mutating tool runs | ~+25 |
| `app.rs` | mod | construct `CheckpointStore` at boot (store dir = `uclaw_home/checkpoints`); expose on AppState + thread into the dispatcher | ~+15 |
| `tauri_commands.rs` + `main.rs` | mod | `code_checkpoint_list` + `code_checkpoint_restore` IPC + `invoke_handler!` registration | ~+50 |

Est. ~450 source + ~220 tests.

---

## Adaptation responsibilities

1. **Read hermes `checkpoint_manager.py` core fns** (`ensure_checkpoint`, `_git_env`, `_take`, `restore`, `list_checkpoints`, path/validation helpers, `_run_git`). Port faithfully; skip the prune/legacy/status code.
2. **`_git_env` EXACT port** (CRITICAL #1) — GIT_DIR/GIT_WORK_TREE/GIT_INDEX_FILE + the 3 config-isolation vars + drop GIT_NAMESPACE/GIT_ALTERNATE_OBJECT_DIRECTORIES. Use `Command::env`/`env_remove`.
3. **`_take` mechanism** — port hermes's snapshot (likely `git add -A` → `write-tree` → `commit-tree` → update ref `refs/uclaw/<project_hash>`). Verify against the source; if hermes uses a simpler `commit`, mirror that. The store must be `git init --bare`'d (or init'd) lazily on first use.
4. **Store path layout** — `uclaw_home/checkpoints/` = GIT_DIR; per-project index `checkpoints/indexes/<project_hash>`; ref `refs/uclaw/<project_hash>` (or hermes's ref scheme). `project_hash` = a stable hash of the abs working_dir (port `_project_hash`).
5. **uclaw_home** — use `uclaw_utils_home::uclaw_home_pathbuf()` (NOT `dirs::home_dir` — the pre-commit hook blocks `dirs::home_dir` for `.uclaw`). Verify the helper name/path.
6. **git subprocess** — mirror an existing uClaw git-spawn (`world/adapters/git.rs` or `gep/git_integration.rs`): `Command::new("git").args(...).envs(git_env).output()`, capture stdout/stderr, map non-zero to an error. Async vs sync: `ensure_checkpoint` on the mutating path should be fast + non-blocking-ish — git add/commit of a small project is ~10-50ms; acceptable synchronously, but if the dispatcher path is async, `tokio::task::spawn_blocking` the git calls. Decide based on the hook context.
7. **Dispatcher hook + per-turn token** — recon `agent/dispatcher/mod.rs` `execute_tool` + the mutating classification (SafetyManager / `is_mutating`). Hook `ensure_checkpoint(workspace_root, turn_token)` before a mutating tool executes. The turn token: reuse the ReasoningContext turn id if available, else a per-loop-iteration counter. If no clean turn token exists, the store's dedup set can be cleared by the loop at turn start (add a `CheckpointStore::reset_turn()` called per turn). **Flag the chosen dedup mechanism.**
8. **Mutating-tool set** — reuse SafetyManager's classification if it exists; else a fixed set: `edit`, `write_file`, `multi_edit`, and `shell` (shell is ambiguous — snapshot before shell too, conservatively). Confirm the actual builtin tool names.
9. **IPC validation** — `code_checkpoint_restore` validates the commit hash (`_validate_commit_hash`: hex, length) + the file path (within working_dir, `_validate_file_path`) before `git checkout`. Port those guards (prevent path traversal / arbitrary checkout).
10. **AppState wiring** — `CheckpointStore` built at boot, shared (`Arc`) with the dispatcher + reachable by the IPC commands (an AppState field).
11. **Tests** — use a `tempfile::TempDir` as a fake project + a `TempDir` as the store; real `git` (CI has git). Test: ensure_checkpoint creates a commit + is per-turn-deduped; restore rolls back a modified file (whole-tree + single-file); list returns checkpoints newest-first; never touches the project's own `.git` (assert no `.git` created in the project dir); skip-broad-dir guard; git-absent → returns false (don't fail). ~10 tests.
12. **Pre-commit hooks** — no `--no-verify`. The hook blocks `dirs::home_dir` for `.uclaw` → use `uclaw_home_pathbuf()`. Also blocks `memory_graph::write` (irrelevant here).

---

## Tasks

### Task 1: `CheckpointStore` core (git-env + ensure_checkpoint + _take + paths)

- [ ] **Step 1: Read hermes** `_git_env`, `_take`, `ensure_checkpoint`, `_store_path`/`_index_path`/`_ref_name`/`_project_hash`, `_run_git`. **Read a uClaw git-spawn** (`world/adapters/git.rs`).

- [ ] **Step 2: Write failing tests** (in `code_checkpoint.rs`): ensure_checkpoint on a temp project creates a snapshot (a ref exists in the store) + does NOT create `<project>/.git`; second ensure_checkpoint same turn → no-op (dedup); skip-broad-dir (`/`, home) → false. Use real `git` + TempDirs.

- [ ] **Step 3: Implement** `CheckpointStore { store_dir: PathBuf, taken: Mutex<HashSet<(u64, String)>> }` (turn-token + abs-dir dedup) + `new(store_dir)` (lazy `git init` the bare store on first use) + `git_env(working_dir, index)` (exact port) + `run_git(args, env) -> Result<Output>` + `project_hash` / path helpers + `ensure_checkpoint(working_dir, turn) -> bool` (dedup, skip-broad, `take`) + `take(abs_dir, turn) -> Result<String commit>` (`git add -A` → `write-tree` → `commit-tree` → update `refs/uclaw/<hash>`). Never-raise contract on `ensure_checkpoint`.

- [ ] **Step 4: Run + commit.**
```bash
cd src-tauri && cargo test --lib agent::code_checkpoint 2>&1 | tail
git add src-tauri/src/agent/code_checkpoint.rs src-tauri/src/agent/mod.rs
git commit -m "feat(agent): shadow checkpoint store — ensure_checkpoint + git-env (SP3.1 of 阶段 5)"
```

### Task 2: restore + list

- [ ] **Step 1: Write failing tests**: modify a file after a checkpoint → `restore(working_dir, commit, None)` reverts the whole tree; `restore(working_dir, commit, Some(file))` reverts one file; `list(working_dir)` returns checkpoints newest-first; restore validates a bad commit hash / out-of-tree path → Err.

- [ ] **Step 2: Implement** `restore(working_dir, commit, file: Option<&str>) -> Result<RestoreOutcome>` (`git checkout <commit> -- <target>` with the shadow env; `_validate_commit_hash` + `_validate_file_path` guards) + `list(working_dir) -> Result<Vec<CheckpointInfo { commit, when, reason }>>` (`git log` the project ref).

- [ ] **Step 3: Run + commit.**
```bash
cd src-tauri && cargo test --lib agent::code_checkpoint 2>&1 | tail
git add src-tauri/src/agent/code_checkpoint.rs
git commit -m "feat(agent): checkpoint restore + list (SP3.2 of 阶段 5)"
```

### Task 3: dispatcher hook + AppState wiring

- [ ] **Step 1: Recon** `agent/dispatcher/mod.rs` `execute_tool` + the mutating classification (SafetyManager/`is_mutating`) + the per-turn token. Decide the dedup mechanism (turn id vs `reset_turn()` per loop iteration). **Flag it.**

- [ ] **Step 2: Build `CheckpointStore` at boot** in `app.rs` (store_dir = `uclaw_home_pathbuf()?.join("checkpoints")`), wrap in `Arc`, add to AppState, thread into the dispatcher.

- [ ] **Step 3: Hook `ensure_checkpoint`** before a mutating tool executes in the dispatcher (workspace_root + turn token). Best-effort (ignore the bool/never-fail). Reset the per-turn dedup at turn start if using the `reset_turn` mechanism.

- [ ] **Step 4: Build + commit.**
```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
git add src-tauri/src/app.rs src-tauri/src/agent/dispatcher/mod.rs
git commit -m "feat(agent): snapshot before mutating tools (dispatcher hook) (SP3.3 of 阶段 5)"
```

### Task 4: IPC commands

- [ ] **Step 1: Define** in `tauri_commands.rs`: `code_checkpoint_list(state, working_dir) -> Result<Vec<CheckpointInfo>, String>` + `code_checkpoint_restore(state, working_dir, commit: Option<String>, file: Option<String>) -> Result<RestoreOutcome, String>` (commit None → latest checkpoint). Both via `state.checkpoint_store`.

- [ ] **Step 2: Register** both in `main.rs` `invoke_handler!`.

- [ ] **Step 3: Full build + commit.**
```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(tauri): code_checkpoint_list + restore IPC (SP3.4 of 阶段 5)"
```

### Task 5: Verification

- [ ] `cd src-tauri && cargo test --lib agent::code_checkpoint 2>&1 | tail` (~10 pass).
- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (clean — dispatcher hook + IPC wiring).
- [ ] `cd src-tauri && cargo test --lib agent 2>&1 | tail -5` (broader green; 2 pre-existing failures unchanged).
- [ ] `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "code_checkpoint|dispatcher|app\.rs|tauri_commands" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] `grep -n "code_checkpoint_restore\|code_checkpoint_list" src-tauri/src/main.rs` (registered).
- [ ] **Isolation sanity**: the ensure_checkpoint test asserts NO `.git` is created in the project dir (the snapshot landed in the shadow store).
- [ ] **No-git sanity**: with git unavailable the path degrades to `false` (don't fail the tool) — covered by a test or a documented manual check.

---

## Self-Review

- ✅ Spec coverage: core (ensure_checkpoint Task 1 + restore/list Task 2), dispatcher hook (Task 3), manual IPC (Task 4). Prune/legacy/auto-restore deferred per the locked scope.
- ✅ No placeholders — the `_take`/`_git_env`/`restore` bodies are "port from hermes" with file+line refs + the exact env contract; the implementer translates (like SP1 ported fuzzy_match.py). The dedup-mechanism + mutating-classification are flagged recon-and-decide items.
- ✅ Type consistency: `CheckpointStore::{new, ensure_checkpoint(&self, &str, u64)->bool, restore(&self,&str,&str,Option<&str>)->Result<RestoreOutcome>, list(&self,&str)->Result<Vec<CheckpointInfo>>}`, `CheckpointInfo { commit, when, reason }`, IPC signatures consistent.
- ✅ Risk-scaled: touches the dispatcher (high blast radius) but the hook is best-effort + additive (snapshot before mutate, never blocks). Isolation env + path-validation guards are the safety gates; the no-`.git`-touch test is the key assertion.
- Decisions: core-only depth + manual IPC (locked); bare-store snapshot via commit-tree; uclaw_home store; git-config isolation ported exactly; best-effort never-blocks.
