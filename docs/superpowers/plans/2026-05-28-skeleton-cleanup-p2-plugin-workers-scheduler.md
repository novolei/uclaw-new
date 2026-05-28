# Skeleton Cleanup P2 — Plugin Loader + Workers + Task Scheduler Kill · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove ~1,330 LoC across 4 dead-or-skeleton subsystems (`plugin_manifest/load.rs` installer-without-installer, `workers/` typed pilot, `task_scheduler/` orchestrator-without-orchestrator, and the `TaskScheduler` struct + tests inside `runtime/task.rs`) with zero behavior change. The `SessionTask` trait + `TaskKind` enum (load-bearing types in `runtime/task.rs`) and the `plugin_manifest::schema` types (preserved for the future subprocess RPC plugin protocol per ADR §6.5) remain intact.

**Architecture:** Pure deletion in 4 independent passes — each pass is its own commit for bisectability. TDD discipline takes the form **grep-verify zero callers (red gate) → delete → `cargo build` clean (green gate)** per pass. The same false-positive caveat from P1 applies (e.g., `PlaywrightCliWorkerStatus` substring matching `Worker*`); the recon below shows precise namespace-bounded greps already returned zero.

**Tech Stack:** Rust 2021, Tauri 2, `cargo`, `grep` / `ripgrep` for caller verification.

---

## Background facts verified against current code (HEAD `8debd782` on main, 2026-05-28 after P1 squash-merge)

### Target 1 — `plugin_manifest/load.rs` (233 LoC) + `mod.rs` trim

- Files: `src-tauri/src/plugin_manifest/load.rs` (233 LoC), `src-tauri/src/plugin_manifest/mod.rs` (33 LoC), `src-tauri/src/plugin_manifest/schema.rs` (279 LoC — **KEEP**).
- `lib.rs:39` has `pub mod plugin_manifest;` — **KEEP** (the schema module stays).
- Public surface of `load.rs`: `load_plugin_manifest` fn, `PluginLoadError` enum, 6 inline tests.
- **Caller analysis (recon at HEAD `8debd782`)**:
  - `grep -rn "plugin_manifest::\|PluginManifest\|PluginContribution\|load_plugin_manifest\|PluginLoadError" src-tauri/src/ --include="*.rs"` filtered to exclude `src/plugin_manifest/` → **zero non-test callers**.
  - Only references are inside `mod.rs` re-exports.
- Intent: `load.rs` was the TOML loader for `.plugin` zip installer (M7-T1). Installer commit 2 never landed. `schema.rs` types remain valuable for the future subprocess RPC plugin protocol (ADR `2026-05-28-uclaw-pi-lightweight-product-philosophy.md` §6.5).

### Target 2 — `workers/` module (401 LoC)

- Files: `src-tauri/src/workers/mod.rs` (28 LoC), `src-tauri/src/workers/spec.rs` (373 LoC).
- `lib.rs:43`: `pub mod workers;` — REMOVE.
- Public surface: `WorkerRole`, `WorkerScope`, `WorkerStatus`, `WorkerSpec`, `WorkerLifecycleEvent`.
- **Caller analysis (precise namespace grep)**:
  - `grep -rn "use crate::workers::\|crate::workers::" src-tauri/src/ --include="*.rs" | grep -v "src/workers/"` → **zero matches**.
  - `grep -rn "\bWorkerSpec\b" src-tauri/src/ --include="*.rs" | grep -v "src/workers/\|src/agent/teams/"` → **zero matches**. (`agent/teams/worker.rs::WorkerSpec` is a SEPARATE, live type in a different namespace — DO NOT touch.)
  - Substring false-positives (`PlaywrightCliWorkerStatus` in `browser/playwright_cli.rs`) are unrelated and not callers of `workers::`.
- Intent: M3-T3 "WorkerSpec / WorkerRole" multi-worker scheduling pilot. No orchestrator wired; the live teams orchestrator at `agent/teams/worker.rs` has its own (smaller) `WorkerSpec`.

### Target 3 — `task_scheduler/` module (391 LoC)

- Files: `src-tauri/src/task_scheduler/mod.rs` (22 LoC), `src-tauri/src/task_scheduler/queue.rs` (369 LoC).
- `lib.rs:28`: `pub mod task_scheduler;` — REMOVE.
- Public surface: `Priority`, `ScheduledTask`, `ScheduleQueue`, `ScheduleStats`.
- **Caller analysis**:
  - `grep -rn "task_scheduler::\|ScheduleQueue\|ScheduledTask\|ScheduleStats" src-tauri/src/ --include="*.rs" | grep -v "src/task_scheduler/"` → **zero non-test, non-self-doc callers**. The only hit is `task_scheduler/mod.rs:14` (self-doc).
- Intent: M3-T4 "ScheduleQueue priority queue pilot". `mod.rs:14` self-states: *"The actual tokio runner that drains the queue lives in M3-T4 commit 2 next to `runtime::task::TaskScheduler`"* — that commit never landed.

### Target 4 — `runtime/task.rs::TaskScheduler` + dead siblings (~336 LoC of 442 LoC file)

File `src-tauri/src/runtime/task.rs` (442 LoC total) structure verified by `grep -n "^pub\|^impl\|^#\[cfg(test)\]\|^}" src-tauri/src/runtime/task.rs`:

| Lines | Item | Verdict | Reason |
|---|---|---|---|
| 1-26 | module doc (mentions `TaskScheduler`/`TaskTermination`) | **TRIM** (rewrite) | doc-stale after deletion |
| 27-44 | `TaskKind` enum + doc | **KEEP** | imported by `agent/regular_task.rs`, `agent/rollout_integration.rs` |
| 46-61 | `impl TaskKind` | **KEEP** | live |
| 63-77 | `TaskTermination` enum | **KILL** | only returned by `TaskScheduler::abort_all_tasks` |
| 79-85 | `GRACEFUL_SHUTDOWN_TIMEOUT` const | **KILL** | only used by `TaskScheduler` |
| 87-104 | `SessionTask` trait doc | **KEEP** | live trait |
| 105-128 | `pub trait SessionTask` decl | **KEEP** | implemented by `RegularTask`, used by `agent/regular_task.rs` |
| 129-141 | `pub struct SpawnedTask` | **KILL** | only used by `TaskScheduler` internals |
| 142-226 | `pub struct TaskScheduler` + `impl` | **KILL** | dead per assessment §1.C |
| 228-442 | `#[cfg(test)] mod tests` | **KILL** | all tests exercise `TaskScheduler::*` — verified by grep below |

- **Caller analysis (precise greps)**:
  - `grep -rn "SpawnedTask" src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"` → **zero matches**.
  - `grep -rn "TaskTermination" src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"` → **zero matches**.
  - `grep -rn "TaskScheduler" src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"` → 3 doc-comment hits (orphan refs, see §"Doc-cleanup follow-up"):
    1. `src-tauri/src/task_scheduler/mod.rs:14` — goes away with Target 3.
    2. `src-tauri/src/runtime/mod.rs:23` — `//! - [task] — SessionTask trait + TaskScheduler (M1-T2a, PR #305)`. **Update**: drop the `+ TaskScheduler` suffix.
    3. `src-tauri/src/agent/regular_task.rs:51, :125` — doc comments mentioning `TaskScheduler::abort_all_tasks`. **Update**: rephrase to mention the live cancellation path (`CancellationToken` via Slice 1a).

### Live items in `runtime/task.rs` to **preserve**

- `TaskKind` enum (lines 40-44) — used by:
  - `agent/regular_task.rs` (per recon: imports `SessionTask` and `TaskKind`)
  - `agent/rollout_integration.rs` (per assessment §1.C)
- `impl TaskKind` (lines 46-61) — `as_str()` and similar.
- `SessionTask` trait (lines 105-128) — implemented by `RegularTask` (used by all `LoopDelegate`-driven runs via `regular_task::run`).

After deletion, `runtime/task.rs` will be ~104 LoC (TaskKind + SessionTask trait + module doc).

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → in sync with `origin/main` at `8debd782`.
2. **Baseline test count**: `cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -5` → expect a known number of passes (P1's `cargo test --lib` reported 3084 passed / some pre-existing failures; same count expected here pre-P2). Record this number for Step "regression check" at end of each task.
3. **Anchor verification (sanity)**:
   ```bash
   ls /Users/ryanliu/Documents/uclaw/src-tauri/src/plugin_manifest/
   ls /Users/ryanliu/Documents/uclaw/src-tauri/src/workers/
   ls /Users/ryanliu/Documents/uclaw/src-tauri/src/task_scheduler/
   grep -n "^pub\|^impl\|^#\[cfg(test)\]\|^}" /Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/task.rs
   grep -n "pub mod plugin_manifest\|pub mod workers\|pub mod task_scheduler\|pub mod runtime" /Users/ryanliu/Documents/uclaw/src-tauri/src/lib.rs
   ```
   Expected: 3 module dirs present, runtime/task.rs structure markers match the §"Target 4" table, lib.rs has 4 `pub mod` lines for these.
4. **Count tests inside `runtime/task.rs` for Task 4 regression expectation**:
   ```bash
   grep -c "#\[test\]\|#\[tokio::test\]" /Users/ryanliu/Documents/uclaw/src-tauri/src/runtime/task.rs
   ```
   Record the count (e.g., "13"). The new pass count post-Task-4 will drop by this number.
5. **Create the worktree + symlinks**:
   ```bash
   git worktree add -b claude/skeleton-cleanup-p2-plugin-workers-scheduler \
       /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/bunembed
   git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler status -sb
   ```
   Expected: `## claude/skeleton-cleanup-p2-plugin-workers-scheduler`, 3 symlinks created.

All paths in the tasks below are relative to the worktree: `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler`.

---

## Task 1: Kill `plugin_manifest/load.rs` + trim `mod.rs`

**Files:**
- Delete: `src-tauri/src/plugin_manifest/load.rs` (233 LoC)
- Modify: `src-tauri/src/plugin_manifest/mod.rs` (drop `pub mod load;` + `pub use load::*` / `pub use load::{...}` re-exports — keep schema re-exports)

### Steps

- [ ] **Step 1.1: Verify zero callers**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler
grep -rn "load_plugin_manifest\|PluginLoadError\|plugin_manifest::load" src-tauri/src/ --include="*.rs" | grep -v "src/plugin_manifest/"
```

Expected: **empty**. Any non-empty result → STOP and report BLOCKED.

- [ ] **Step 1.2: Read `plugin_manifest/mod.rs` to identify what to remove**

```bash
cat src-tauri/src/plugin_manifest/mod.rs
```

Expect ~33 LoC: `pub mod schema;` + `pub mod load;` + likely `pub use load::*` or `pub use load::{load_plugin_manifest, PluginLoadError};` + `pub use schema::*` or similar.

Identify the EXACT lines that reference `load` (the module decl + any re-exports). KEEP schema-related lines.

- [ ] **Step 1.3: Delete `load.rs`**

```bash
rm src-tauri/src/plugin_manifest/load.rs
ls src-tauri/src/plugin_manifest/
```

Expected: `mod.rs` + `schema.rs` remain.

- [ ] **Step 1.4: Trim `mod.rs`**

Open `src-tauri/src/plugin_manifest/mod.rs`. Remove:
- The `pub mod load;` declaration.
- Any `pub use load::*` or `pub use load::{...}` re-exports.

Keep `pub mod schema;` and any `pub use schema::*` / explicit schema re-exports.

If the `mod.rs` module-level `//!` doc mentions the installer/loader explicitly (e.g., "loads plugins via TOML"), update the doc to say "schema-only — installer was removed in P2 cleanup; future subprocess RPC plugin protocol per ADR §6.5".

Verify:
```bash
grep -n "load" src-tauri/src/plugin_manifest/mod.rs
```

Expected: **empty** (no `load`-mentioning lines).

- [ ] **Step 1.5: Build + commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: **empty** (no error lines).

Stage + commit:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler add -A src-tauri/src/plugin_manifest/
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler commit -m "$(cat <<'EOF'
chore(plugin_manifest): kill dead load.rs installer (P2.1 of 阶段 2)

233 LoC of unreachable installer code. plugin_manifest/load.rs was the TOML
loader for the M7-T1 .plugin zip installer; installer commit 2 never landed,
zero non-test callers verified by grep. plugin_manifest/schema.rs (279 LoC)
is KEPT — the manifest type schema (PluginManifest, PluginContribution,
PluginAuthor, PluginPermissions, PluginRuntimeRequirement) remains valuable
for the future subprocess RPC plugin protocol per ADR §6.5.

mod.rs trimmed to schema-only re-exports.
EOF
)"
```

Record the commit SHA. Continue to Task 2.

---

## Task 2: Kill `workers/` module

**Files:**
- Delete: `src-tauri/src/workers/mod.rs` (28 LoC) + `src-tauri/src/workers/spec.rs` (373 LoC) + `src-tauri/src/workers/` directory.
- Modify: `src-tauri/src/lib.rs:43` — remove `pub mod workers;` (+ any preceding doc comment line).

### Steps

- [ ] **Step 2.1: Re-verify zero callers (precise namespace-bounded grep)**

```bash
grep -rn "use crate::workers::\|crate::workers::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/workers/"
grep -rn "\bWorkerSpec\b\|\bWorkerRole\b\|\bWorkerScope\b\|\bWorkerStatus\b\|\bWorkerLifecycleEvent\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/workers/\|src/agent/teams/"
```

Expected: BOTH **empty**. Any match → STOP. (Note: `src/agent/teams/worker.rs` has its own `WorkerSpec`/`WorkerRole`/etc. — those are LIVE and unrelated. The `grep -v "src/agent/teams/"` filter excludes them.)

Bonus check that `PlaywrightCliWorkerStatus` substring matches are excluded:
```bash
grep -rn "PlaywrightCliWorker" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | head -3
```

Should return matches **only** in `src/browser/playwright_cli.rs` and `src/browser/mod.rs` — these are an UNRELATED type in browser automation, not a `workers::` consumer.

- [ ] **Step 2.2: Delete the `workers/` directory**

```bash
rm src-tauri/src/workers/mod.rs
rm src-tauri/src/workers/spec.rs
rmdir src-tauri/src/workers/
ls src-tauri/src/workers/ 2>&1
```

Expected: "No such file or directory".

- [ ] **Step 2.3: Remove `pub mod workers;` from `lib.rs`**

Open `src-tauri/src/lib.rs`. At line 43 (verify exact line first via `grep -n "pub mod workers" src-tauri/src/lib.rs`), delete the `pub mod workers;` line + any preceding `///` doc line that exclusively describes the workers module.

Verify:
```bash
grep -n "workers" src-tauri/src/lib.rs
```

Expected: **empty** (no `workers`-mentioning lines).

- [ ] **Step 2.4: Build + commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: **empty**.

Stage + commit:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler add -A src-tauri/src/workers/ src-tauri/src/lib.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler commit -m "$(cat <<'EOF'
chore(workers): kill dead workers/ module (P2.2 of 阶段 2)

401 LoC of unreachable multi-worker scheduling pilot. workers/spec.rs
was the M3-T3 'WorkerSpec / WorkerRole / WorkerScope / WorkerStatus /
WorkerLifecycleEvent' typed pilot; orchestrator never wired. Zero
non-test callers verified by precise namespace-bounded grep (the
substring 'PlaywrightCliWorkerStatus' in browser/playwright_cli.rs is
an unrelated browser-automation type, not a consumer).

The live agent teams orchestrator's separate WorkerSpec at
agent/teams/worker.rs is untouched.

lib.rs cleaned: pub mod workers; declaration removed.
EOF
)"
```

Record the commit SHA. Continue to Task 3.

---

## Task 3: Kill `task_scheduler/` module

**Files:**
- Delete: `src-tauri/src/task_scheduler/mod.rs` (22 LoC) + `src-tauri/src/task_scheduler/queue.rs` (369 LoC) + `src-tauri/src/task_scheduler/` directory.
- Modify: `src-tauri/src/lib.rs:28` — remove `pub mod task_scheduler;`.

### Steps

- [ ] **Step 3.1: Re-verify zero callers**

```bash
grep -rn "use crate::task_scheduler::\|crate::task_scheduler::\|ScheduleQueue\|ScheduledTask\b\|ScheduleStats" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/task_scheduler/"
```

Expected: **empty**. (Note: `ScheduledTask` as a substring may match other types; the `\b` word boundary should filter most. If any match returns, inspect it.)

- [ ] **Step 3.2: Delete the `task_scheduler/` directory**

```bash
rm src-tauri/src/task_scheduler/mod.rs
rm src-tauri/src/task_scheduler/queue.rs
rmdir src-tauri/src/task_scheduler/
ls src-tauri/src/task_scheduler/ 2>&1
```

Expected: "No such file or directory".

- [ ] **Step 3.3: Remove `pub mod task_scheduler;` from `lib.rs`**

```bash
grep -n "pub mod task_scheduler" src-tauri/src/lib.rs
```

Note the exact line (recon said `:28`). Delete that line + any `///` doc above describing only that module.

Verify:
```bash
grep -n "task_scheduler" src-tauri/src/lib.rs
```

Expected: **empty**.

- [ ] **Step 3.4: Build + commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: **empty**.

Stage + commit:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler add -A src-tauri/src/task_scheduler/ src-tauri/src/lib.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler commit -m "$(cat <<'EOF'
chore(task_scheduler): kill dead task_scheduler/ module (P2.3 of 阶段 2)

391 LoC of unreachable orchestrator pilot. task_scheduler/queue.rs was
the M3-T4 ScheduleQueue / ScheduledTask / Priority / ScheduleStats
priority-queue pilot. mod.rs:14 self-stated 'the actual tokio runner
that drains the queue lives in M3-T4 commit 2 next to
runtime::task::TaskScheduler' — that commit never landed.

Zero non-test callers verified by grep (only self-doc references in
mod.rs:14 which goes away with the file).

lib.rs cleaned: pub mod task_scheduler; declaration removed.
EOF
)"
```

Record the commit SHA. Continue to Task 4.

---

## Task 4: Kill `TaskScheduler` struct + `SpawnedTask` + `TaskTermination` + tests from `runtime/task.rs` + orphan doc cleanup

**Files:**
- Modify: `src-tauri/src/runtime/task.rs` — remove lines per the "Target 4" table in §"Background facts" (the `TaskTermination` enum, `GRACEFUL_SHUTDOWN_TIMEOUT` const, `SpawnedTask` struct, `TaskScheduler` struct + impl, the entire `#[cfg(test)] mod tests` block). Update module-level doc.
- Modify: `src-tauri/src/runtime/mod.rs:23` — update doc comment (drop `+ TaskScheduler` from `//! - [task] — SessionTask trait + TaskScheduler (M1-T2a, PR #305)`).
- Modify: `src-tauri/src/agent/regular_task.rs:51, :125` — update 2 doc comments referencing `TaskScheduler::abort_all_tasks` (live cancellation is via `CancellationToken` from Slice 1a).

### Steps

- [ ] **Step 4.1: Re-verify caller status precisely**

```bash
grep -rn "TaskScheduler\|SpawnedTask\|TaskTermination\|GRACEFUL_SHUTDOWN_TIMEOUT" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"
```

Expected: 2 results (orphan doc-comment references in `agent/regular_task.rs:51` and `:125`) + 1 result in `runtime/mod.rs:23`. Anything else → STOP.

(`task_scheduler/mod.rs:14` was deleted in Task 3.)

Then verify the LIVE items stay:
```bash
grep -rn "\bTaskKind\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"
grep -rn "\bSessionTask\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs" | grep -v "src/runtime/task.rs"
```

Expected: both return AT LEAST a few hits (importers in `agent/regular_task.rs` and `agent/rollout_integration.rs`). Confirm both still have live consumers — if zero, the keep-decision was wrong, abort with NEEDS_CONTEXT.

- [ ] **Step 4.2: Count `runtime/task.rs` tests for regression expectation**

```bash
grep -c "#\[test\]\|#\[tokio::test\]" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/runtime/task.rs
```

Record the count (e.g., "13"). The post-deletion test count drops by this number.

- [ ] **Step 4.3: Surgically delete from `runtime/task.rs`**

Open `src-tauri/src/runtime/task.rs`. Per the §"Target 4" table, **delete the following ranges** (re-verify line numbers with `grep -n` first):

1. Lines containing the `TaskTermination` enum + its doc (originally ~63-77).
2. Lines containing the `GRACEFUL_SHUTDOWN_TIMEOUT` const + its doc (originally ~79-85).
3. Lines containing the `SpawnedTask` struct (originally ~129-141).
4. Lines containing the `TaskScheduler` struct + `impl TaskScheduler` block (originally ~142-226).
5. The entire `#[cfg(test)] mod tests { ... }` block at the end (originally ~228-442).

**Keep**:
- Module doc at lines 1-26 (REWRITE — see Step 4.4).
- `TaskKind` enum + impl (lines 27-44 + 46-61).
- `SessionTask` trait doc + decl (lines 87-128).

If the file has separator blank lines between items, preserve normal Rust spacing (one blank between top-level items).

- [ ] **Step 4.4: Rewrite the module-level doc**

The original module doc at lines 1-26 mentions `TaskScheduler` and `TaskTermination`. Replace the whole module doc block with:

```rust
//! M1-T2a — `SessionTask` trait + `TaskKind` enum.
//!
//! Shared task-shape vocabulary for long-running session activities
//! (model+tool loops, review tasks, compaction). The `TaskScheduler`
//! preemption scaffold was removed in P2 of the 阶段 2 skeleton cleanup
//! (never wired into production; Slice 1a's `CancellationToken` covers
//! the cancellation surface it was designed for, and `agent/regular_task.rs`
//! drives `run_agentic_loop` directly).
//!
//! - [`TaskKind`] enum — discriminates the three current task shapes.
//! - [`SessionTask`] trait — the contract every long-running session
//!   activity implements.
```

- [ ] **Step 4.5: Clean up unused imports**

After the deletion, `runtime/task.rs` may have unused imports that previously served the deleted code (e.g., `tokio::sync::Mutex`, `tokio::time::Duration`, `tokio_util::sync::CancellationToken`, `std::collections::HashMap`). Run:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "warning: unused" | grep "runtime/task.rs"
```

For each warning, remove the unused import from `runtime/task.rs`. Re-run until empty.

Likely remaining imports after cleanup: `use crate::agent::types::LoopOutcome;` (if `SessionTask` returns it), `tokio_util::sync::CancellationToken` (`SessionTask::run` signature), and Arc / async_trait if used. Verify the trait body still compiles.

- [ ] **Step 4.6: Update orphan doc comments in `runtime/mod.rs`**

Open `src-tauri/src/runtime/mod.rs`. Find line 23 (verify with `grep -n "TaskScheduler" src-tauri/src/runtime/mod.rs`):

```rust
//! - [`task`]           — SessionTask trait + TaskScheduler (M1-T2a, PR #305)
```

Change to:

```rust
//! - [`task`]           — SessionTask trait + TaskKind enum (M1-T2a, PR #305; TaskScheduler removed in P2 cleanup)
```

- [ ] **Step 4.7: Update orphan doc comments in `agent/regular_task.rs`**

Find the 2 references via:
```bash
grep -n "TaskScheduler" src-tauri/src/agent/regular_task.rs
```

Expected: lines ~51 and ~125 (verify exact lines).

**At ~:51**: existing comment says something like *"Held by `TaskScheduler` between user messages."* Change to: *"Drives one `run_agentic_loop` invocation per user message. (Originally designed to be held by `TaskScheduler`, which was removed in P2 cleanup; cancellation is now via `CancellationToken` per Slice 1a.)"*

**At ~:125**: existing comment says something like *"exactly once through `TaskScheduler::abort_all_tasks`"*. Change to: *"exactly once through the `CancellationToken` installed on the `ReasoningContext` (Slice 1a)."*

(Adjust wording to fit the surrounding sentence's grammar — the goal is to drop `TaskScheduler` references while preserving the original comment's intent.)

- [ ] **Step 4.8: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: **empty**.

Also check no new warnings:
```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo build 2>&1 | grep -E "^warning" | head -5
```

If any new warning attributable to this PR appears (e.g., "unused import" in a file we touched), clean it.

- [ ] **Step 4.9: Test regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo test --lib agent:: 2>&1 | tail -10
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri && cargo test --lib 2>&1 | tail -10
```

Expected: the `agent::` count stays at the post-P1 baseline (e.g., 759 passed + 2 pre-existing failures). The broader `cargo test --lib` count drops by exactly the count recorded in Step 4.2 (the `runtime/task.rs` test count). The same pre-existing unrelated failures remain.

- [ ] **Step 4.10: Final orphan-reference sweep**

```bash
grep -rn "TaskScheduler\|SpawnedTask\|TaskTermination\|GRACEFUL_SHUTDOWN_TIMEOUT" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler/src-tauri/src/ --include="*.rs"
```

Expected: **empty** (all references to these 4 symbols cleaned up after Tasks 1-4).

- [ ] **Step 4.11: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler add -A \
    src-tauri/src/runtime/task.rs \
    src-tauri/src/runtime/mod.rs \
    src-tauri/src/agent/regular_task.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler diff --cached --stat
```

Expected: 3 files in the stat, only the targeted regions modified.

Commit:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler commit -m "$(cat <<'EOF'
chore(runtime): kill dead TaskScheduler from runtime/task.rs (P2.4 of 阶段 2)

~336 LoC of unreachable preemption scaffold. The TaskScheduler struct,
SpawnedTask helper, TaskTermination enum, GRACEFUL_SHUTDOWN_TIMEOUT const,
and the entire test module are removed; the production cancellation
surface is covered by Slice 1a's CancellationToken (no TaskScheduler ever
called).

Live items in runtime/task.rs are PRESERVED:
- TaskKind enum (imported by agent/regular_task.rs + agent/rollout_integration.rs)
- SessionTask trait (implemented by RegularTask)

Module-level doc rewritten to reflect the actual surface; orphan
TaskScheduler doc references in runtime/mod.rs:23 and agent/regular_task.rs
(2 sites) updated to mention the live CancellationToken path.

Completes P2 of the 阶段 2 skeleton cleanup series. Cumulative P2 deletion:
~1,360 LoC across plugin_manifest/load.rs (-233), workers/ (-401),
task_scheduler/ (-391), runtime/task.rs (-336 dead). Live agent path
unchanged; cargo build clean; agent:: regression at post-P1 baseline.
EOF
)"
```

Verify:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler log --oneline 8debd782..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p2-plugin-workers-scheduler status -sb
```

Expected: 4 commits ahead of `main` (Tasks 1-4); working tree clean.

---

## Self-Review

**1. Spec coverage (against `docs/superpowers/specs/2026-05-28-skeleton-cleanup-assessment.md` §1.D + §1.C):**
- §1.D — `plugin_manifest/load.rs` (233 LoC) KILL, schema.rs preserved → Task 1. ✓
- §1.C — `workers/` (401 LoC) KILL → Task 2. ✓
- §1.C — `task_scheduler/` (391 LoC) KILL → Task 3. ✓
- §1.C — `TaskScheduler` struct (~60 LoC originally estimated, actually ~336 LoC with SpawnedTask + TaskTermination + tests + const) KILL → Task 4. Live `SessionTask` trait + `TaskKind` preserved. ✓

The 336 LoC figure in Task 4 is larger than the assessment's ~60 LoC estimate because the assessment underestimated `SpawnedTask` + `TaskTermination` + the test module (~214 LoC of tests testing only `TaskScheduler`). The plan corrects this with precise recon — the assessment number was a floor estimate.

**2. Placeholder scan:**
- No "TBD" / "TODO" / "implement later" / "similar to Task N".
- Step 4.7 wording adjustments ("Adjust wording to fit the surrounding sentence's grammar") are intentional micro-flexibility for prose updates, not placeholders — the goal is explicit ("drop TaskScheduler references; preserve original intent").
- Step 4.5 "Likely remaining imports" is a hint, not a placeholder — the cargo warning loop is the authoritative driver.

**3. Type consistency:**
- `runtime/task.rs` symbols (`TaskKind`, `SessionTask`, `SpawnedTask`, `TaskScheduler`, `TaskTermination`, `GRACEFUL_SHUTDOWN_TIMEOUT`) named consistently across all sub-tasks.
- `workers/` types (`WorkerSpec`, `WorkerRole`, `WorkerScope`, `WorkerStatus`, `WorkerLifecycleEvent`) named consistently.
- `task_scheduler/` types (`ScheduleQueue`, `ScheduledTask`, `Priority`, `ScheduleStats`) named consistently.
- Plugin types (`PluginManifest`, `PluginContribution`, `load_plugin_manifest`, `PluginLoadError`) named consistently.
- Doc-comment updates in Task 4.6 and 4.7 refer to the same `CancellationToken` (Slice 1a) live cancellation path.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 1 person-day (4 deletion passes + 2 doc cleanups).
- **Risk:** low. 4 independent grep gates + 4 independent build gates + 1 final orphan sweep.
- **Files touched:** Tasks 1 (2 files: 1 delete, 1 modify), 2 (3 files: 2 delete + dir + 1 modify), 3 (3 files: 2 delete + dir + 1 modify), 4 (3 files: 1 modify in runtime/task.rs + 2 doc updates).
- **Net LoC:** ~−1,360 (precise numbers in commit messages).
- **PR shape:** 1 worktree → 4 commits → 1 PR. Bisectable per-task (each commit is a complete deletion of one target). Squash on land optional.
- **No new tests written.** The deletion removes ~13 tests inside `runtime/task.rs::tests` (Task 4); other test modules unaffected.
- **No Open Decisions block P2.** P2 is the second lowest-risk PR; P5 still needs the `memory_policy` recency answer.
