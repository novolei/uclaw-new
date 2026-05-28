# Skeleton Cleanup P3 — Harness Split + Rename · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Free up the `harness/` module name for the future autonomy supervisor (ADR §10) by (1) extracting the 2 load-bearing production types out of `harness/` into `agent/`, (2) renaming the remaining offline eval machine `harness/` → `eval/`, (3) renaming the 4 Tauri eval commands `run_*_harness` → `run_*_eval`, and (4) renaming the `*Harness*` Rust types + UI surface to match. Zero net LoC — pure mechanical rename. Zero behavior change.

**Architecture:** 5 bisectable commits in 1 PR. Each commit is `cargo build` clean + `cargo test --lib agent::` at baseline. The final commit also gates on `npm test -- --run` so the UI invoke surface lands atomically with the backend command rename (the breaking-change boundary lives entirely inside that commit).

**Tech Stack:** Rust 2021, Tauri 2, TypeScript / React (UI), vitest, `cargo`, ripgrep.

---

## Background facts verified against HEAD `81f0ac79` (main after P2 squash-merge)

### Total harness/ surface

```
src-tauri/src/harness/         18 files / 7,203 LoC
├── adapters/                  9 files / 4,163 LoC
│   ├── agent_loop.rs          594
│   ├── browser_provider.rs    391
│   ├── browser.rs           1,715
│   ├── hosted_provider.rs     349
│   ├── live_room.rs           283
│   ├── memory_policy.rs        14   <- slated for P5 deletion; moves to eval/adapters/ in P3
│   ├── memory_policy_tests.rs  74   <- ditto
│   ├── memory.rs              690
│   └── mod.rs                  53
├── artifacts.rs              110
├── budget.rs                  67   <- EXTRACT to agent/tool_budget.rs (Task 2)
├── campaign.rs               460
├── campaign_tests.rs         163
├── case.rs                   246
├── episode.rs                 95
├── graders.rs                144
├── memory_inventory.rs       380
├── mod.rs                     35
├── performance_scorecard.rs  373
├── performance_scorecard_tests.rs  277
├── runtime.rs                143
├── self_improvement.rs       300
├── trace.rs                  112
└── trajectory.rs             135   <- EXTRACT to agent/trajectory.rs (Task 1)
```

### Load-bearing production types (extract before rename)

**`TrajectoryStore`** + types (`TurnRecord`, `TrajectorySearchHit`) — 5 cross-tree consumer files:
- `src-tauri/src/app.rs:322,661` — `AppState.trajectory_store`, constructed at boot.
- `src-tauri/src/agent/dispatcher.rs:66,632,633` — field on dispatcher + `set_trajectory_store`.
- `src-tauri/src/agent/tool_dispatch/mod.rs:119,137,161,500` — field + 2 constructors + `TurnRecord` import for emit.
- `src-tauri/src/tauri_commands.rs:14473,14482` — 2 query commands return `TurnRecord` / `TrajectorySearchHit`.

**`ToolBudgetManager`** — 3 cross-tree consumer files:
- `src-tauri/src/app.rs:323,662` — `AppState.tool_budget`, constructed at boot.
- `src-tauri/src/agent/dispatcher.rs:68,637,638` — field + `set_tool_budget`.
- `src-tauri/src/agent/tool_dispatch/mod.rs:120,138,162` — field + 2 constructors.

### Tauri eval command surface (4 commands, NOT 7)

`src-tauri/src/main.rs:1259-1262` registers exactly 4:
- `run_memory_gbrain_eval_harness` (tauri_commands.rs:558)
- `run_browser_parity_harness` (tauri_commands.rs:575)
- `run_agent_control_plane_harness` (tauri_commands.rs:594)
- `run_self_improvement_gate_harness` (tauri_commands.rs:610)

Plus 1 internal helper `build_memory_gbrain_eval_harness_report` (tauri_commands.rs:536, also referenced by test at :764, :816). This helper is NOT a Tauri command — it's a pub fn called by `run_memory_gbrain_eval_harness`. It gets renamed too for symmetry.

The 2 trajectory query commands at tauri_commands.rs:14473, :14482 do NOT have `harness` in their command names — only their return types reference `crate::harness::trajectory::*`. These commands keep their names; only the import path changes when trajectory.rs moves.

### UI surface (SystemTab.tsx + test)

`ui/src/components/settings/SystemTab.tsx` (lines verified):
- L83-102: 3 TS interfaces (`HarnessCheckResult`, `HarnessScorecard`, `HarnessSuiteReport`).
- L116: type alias `HarnessKind`.
- L118-123: `harnessCommands: Record<HarnessKind, string>` mapping with 4 `run_*_harness` strings.
- L188-189: state hooks `harnessBusy`/`setHarnessBusy`, `harnessReports`/`setHarnessReports`.
- L253-275: 2 handlers `handleHarnessRun`, `handleRunAllHarnesses`.
- L449-450: Chinese section label `"Harness 评估"`.
- L463-487: 5 `<HarnessButton>` JSX usages.
- L591: `function HarnessButton(...)`.
- L656: `function normalizeHarnessReport(kind: HarnessKind, ...)`.

`ui/src/components/settings/SystemTab.test.tsx`:
- Test names mention "harness" (e.g., `'SystemTab harness reporting'`, `'runs the agent control-plane harness ...'`).
- Mock invoke arms: 4 `run_*_harness` command strings.

### Rust internal type-name surface (commit 4)

Names containing the `Harness` substring inside `harness/` (and 1 site in `memory_policy/receipts.rs`):
- `HarnessRuntime` (runtime.rs)
- `HarnessEvent` (trace.rs) — used by `memory_policy/receipts.rs:107-116`
- `MemoryHarnessTarget` (trace.rs) — used by `memory_policy/receipts.rs:107-116`
- `MemoryGbrainHarnessAdapter` (adapters/memory.rs)
- `BrowserHarnessAdapter` (adapters/browser.rs)
- `AgentLoopControlPlaneHarnessAdapter` (adapters/agent_loop.rs)

Plus lowercase `harness` in non-name positions:
- module doc lines (`//! Offline eval harness ...`)
- comments
- string literals (e.g., `tauri_commands.rs:543,581,600` — disk path `data_dir.join("harness")`. These are eval **artifact paths on disk** — renaming them changes where artifacts are written. **DECISION**: keep the on-disk path string as `"harness"` to avoid orphaning existing eval artifacts on dev machines. Comment explaining the historical name will be added inline at each site.)

### Non-harness consumer file count (cross-tree imports of `crate::harness`)

5 files outside `src-tauri/src/harness/`:
- `agent/dispatcher.rs`
- `agent/tool_dispatch/mod.rs`
- `app.rs`
- `memory_policy/receipts.rs`
- `tauri_commands.rs`

Total `crate::harness` import lines: 103 (59 self-references inside harness/, 44 cross-tree).

### Baselines

- `cargo build`: green at HEAD.
- `cargo test --lib agent::`: 759 passed / 2 pre-existing failures (post-P2 baseline).
- `cargo test --lib` total: 3044 passed / 7 pre-existing failures.
- `npm test -- --run` in `ui/`: 1090 passed / 2 pre-existing failures (ConnectivityTab.test.tsx).

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main`, in sync at `81f0ac79`.

2. **Create the worktree + symlinks** (parent repo has gitignored `gbrain-source`, `pyembed`, `bunembed` that the build needs):

```bash
git worktree add -b claude/skeleton-cleanup-p3-harness-split-rename \
    /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/bunembed
```

3. **Baseline verifications inside the worktree**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, no errors

cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 759 passed / 2 failed (pre-existing)

cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui && npm test -- --run 2>&1 | tail -10
# expect: 1090 passed / 2 failed (ConnectivityTab.test.tsx, pre-existing)
```

Record the 3 baseline numbers — every task's regression check compares against them.

All paths in tasks below are relative to the worktree: `/Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename`.

---

## Task 1: Extract `harness/trajectory.rs` → `agent/trajectory.rs`

**Files:**
- Move: `src-tauri/src/harness/trajectory.rs` (135 LoC) → `src-tauri/src/agent/trajectory.rs`
- Modify: `src-tauri/src/harness/mod.rs` — remove `pub mod trajectory;` + any `pub use trajectory::*` re-exports
- Modify: `src-tauri/src/agent/mod.rs` — add `pub mod trajectory;` (in alphabetical position with existing modules)
- Modify (import path updates `crate::harness::trajectory::*` → `crate::agent::trajectory::*` and `crate::harness::TrajectoryStore` → `crate::agent::trajectory::TrajectoryStore`):
  - `src-tauri/src/app.rs:322,661`
  - `src-tauri/src/agent/dispatcher.rs:66,632,633`
  - `src-tauri/src/agent/tool_dispatch/mod.rs:119,137,161,500`
  - `src-tauri/src/tauri_commands.rs:14473,14482`

### Steps

- [ ] **Step 1.1: Move the file**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename mv \
    src-tauri/src/harness/trajectory.rs src-tauri/src/agent/trajectory.rs
```

Verify: `ls src-tauri/src/harness/trajectory.rs 2>&1` → "No such file or directory"; `ls src-tauri/src/agent/trajectory.rs 2>&1` → file present.

- [ ] **Step 1.2: Wire into `agent/mod.rs`**

Read `src-tauri/src/agent/mod.rs`. Find the module declarations region (likely alphabetically sorted). Add `pub mod trajectory;` in the correct alphabetical position. If there is a `pub use trajectory::*` style elsewhere in agent/mod.rs that re-exports from siblings, follow the existing convention; otherwise just `pub mod trajectory;` is enough.

- [ ] **Step 1.3: Unwire from `harness/mod.rs`**

Read `src-tauri/src/harness/mod.rs`. Remove:
- The `pub mod trajectory;` line.
- Any `pub use trajectory::*` or explicit `pub use trajectory::{TrajectoryStore, TurnRecord, ...}` re-exports.

Verify: `grep -n "trajectory" src-tauri/src/harness/mod.rs` → empty.

- [ ] **Step 1.4: Update non-harness consumer imports (path swap)**

For each of the 5 consumer files, replace `crate::harness::trajectory::*` → `crate::agent::trajectory::*` and any `crate::harness::TrajectoryStore` (re-exported shortcut) → `crate::agent::trajectory::TrajectoryStore`:

```bash
grep -rn "crate::harness::trajectory\|crate::harness::TrajectoryStore" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Use Edit (NOT sed) on each result. For each file, the substitution is:
- `crate::harness::trajectory::` → `crate::agent::trajectory::`
- `crate::harness::TrajectoryStore` → `crate::agent::trajectory::TrajectoryStore`

After edits, re-grep:
```bash
grep -rn "crate::harness::trajectory\|crate::harness::TrajectoryStore" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected: **empty**.

- [ ] **Step 1.5: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: **empty**. If errors, inspect — likely missed import path; fix and re-run.

- [ ] **Step 1.6: Regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: 759 passed / 2 failed (baseline).

- [ ] **Step 1.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename add -A \
    src-tauri/src/harness/trajectory.rs \
    src-tauri/src/agent/trajectory.rs \
    src-tauri/src/harness/mod.rs \
    src-tauri/src/agent/mod.rs \
    src-tauri/src/app.rs \
    src-tauri/src/agent/dispatcher.rs \
    src-tauri/src/agent/tool_dispatch/mod.rs \
    src-tauri/src/tauri_commands.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename commit -m "$(cat <<'EOF'
refactor(agent): extract harness/trajectory.rs to agent/trajectory.rs (P3.1 of 阶段 2)

TrajectoryStore is load-bearing production code (every agent turn writes;
AppState holds it; 2 Tauri query commands return TurnRecord /
TrajectorySearchHit). Move it out of the offline-eval harness/ module
into agent/ where its conceptual owner already lives (dispatcher +
tool_dispatch wire it through; app.rs constructs it).

Zero behavior change — file moved, import paths rewritten in 5
consumer files (app.rs, agent/dispatcher.rs, agent/tool_dispatch/mod.rs,
tauri_commands.rs, plus harness/mod.rs unwire + agent/mod.rs wire-in).
cargo build clean; agent:: 759/2 at baseline.

First commit of P3 (free up harness/ name for future autonomy supervisor
per ADR §10).
EOF
)"
```

Record the commit SHA. Continue to Task 2.

---

## Task 2: Extract `harness/budget.rs` → `agent/tool_budget.rs`

**Files:**
- Move: `src-tauri/src/harness/budget.rs` (67 LoC) → `src-tauri/src/agent/tool_budget.rs`
- Modify: `src-tauri/src/harness/mod.rs` — remove `pub mod budget;` + any `pub use budget::*` / `pub use budget::ToolBudgetManager` re-exports
- Modify: `src-tauri/src/agent/mod.rs` — add `pub mod tool_budget;` (alphabetical position)
- Modify (import path updates `crate::harness::budget::*` → `crate::agent::tool_budget::*` and `crate::harness::ToolBudgetManager` → `crate::agent::tool_budget::ToolBudgetManager`):
  - `src-tauri/src/app.rs:323,662`
  - `src-tauri/src/agent/dispatcher.rs:68,637,638`
  - `src-tauri/src/agent/tool_dispatch/mod.rs:120,138,162`

### Steps

- [ ] **Step 2.1: Move the file (with rename: budget.rs → tool_budget.rs)**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename mv \
    src-tauri/src/harness/budget.rs src-tauri/src/agent/tool_budget.rs
```

Verify: source gone, destination present.

- [ ] **Step 2.2: Wire into `agent/mod.rs`**

Read `agent/mod.rs`. Add `pub mod tool_budget;` in the correct alphabetical position (after `tool_dispatch` if `tool_*` cluster exists, or after `trajectory` from Task 1).

- [ ] **Step 2.3: Unwire from `harness/mod.rs`**

Read `harness/mod.rs`. Remove `pub mod budget;` + any `pub use budget::*` / `pub use budget::{ToolBudgetManager}` re-exports.

Verify: `grep -n "budget" src-tauri/src/harness/mod.rs` → empty.

- [ ] **Step 2.4: Update consumer imports**

```bash
grep -rn "crate::harness::budget\|crate::harness::ToolBudgetManager" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

For each result, Edit substitutions:
- `crate::harness::budget::` → `crate::agent::tool_budget::`
- `crate::harness::ToolBudgetManager` → `crate::agent::tool_budget::ToolBudgetManager`

Re-grep:
```bash
grep -rn "crate::harness::budget\|crate::harness::ToolBudgetManager" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected: **empty**.

- [ ] **Step 2.5: Build + regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; 759 passed / 2 failed.

- [ ] **Step 2.6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename add -A \
    src-tauri/src/harness/budget.rs \
    src-tauri/src/agent/tool_budget.rs \
    src-tauri/src/harness/mod.rs \
    src-tauri/src/agent/mod.rs \
    src-tauri/src/app.rs \
    src-tauri/src/agent/dispatcher.rs \
    src-tauri/src/agent/tool_dispatch/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename commit -m "$(cat <<'EOF'
refactor(agent): extract harness/budget.rs to agent/tool_budget.rs (P3.2 of 阶段 2)

ToolBudgetManager truncates oversized tool results — load-bearing
production code (AppState constructs it at boot; dispatcher +
tool_dispatch hold it). Move it out of the offline-eval harness/
module into agent/ where the consumers already live. File renamed
budget.rs -> tool_budget.rs at the move to disambiguate from other
budget concepts (e.g., token_budget) and match the type name's
naming convention.

Zero behavior change — file moved+renamed, import paths rewritten in
3 consumer files (app.rs, agent/dispatcher.rs, agent/tool_dispatch/mod.rs,
plus harness/mod.rs unwire + agent/mod.rs wire-in).
cargo build clean; agent:: 759/2 at baseline.
EOF
)"
```

Record the commit SHA. Continue to Task 3.

---

## Task 3: Rename `harness/` directory → `eval/`

**Goal**: Move the remaining `harness/` subtree (16 files / ~7,000 LoC) to `eval/` and update every `crate::harness::*` reference to `crate::eval::*` site-wide. Pure directory rename — no internal type renames yet (those land in Task 4). The Tauri command **names** also stay as `run_*_harness` until Task 5 — only Rust `mod` paths and `crate::harness::` import sites change here.

**Files:**
- `git mv`: `src-tauri/src/harness/` → `src-tauri/src/eval/` (preserves history for all 16 remaining files: artifacts.rs, campaign.rs, campaign_tests.rs, case.rs, episode.rs, graders.rs, memory_inventory.rs, mod.rs, performance_scorecard.rs, performance_scorecard_tests.rs, runtime.rs, self_improvement.rs, trace.rs, adapters/*).
- Modify: `src-tauri/src/lib.rs` — `pub mod harness;` → `pub mod eval;`
- Modify each remaining `eval/` file: any `crate::harness::*` → `crate::eval::*` (59 self-references).
- Modify (cross-tree): `crate::harness::*` → `crate::eval::*` in:
  - `src-tauri/src/tauri_commands.rs` (~30 sites)
  - `src-tauri/src/memory_policy/receipts.rs:107-116` (3 sites: `crate::harness::trace::HarnessEvent`, `crate::harness::trace::MemoryHarnessTarget`)
- **NOT touched** (stays `harness`):
  - On-disk artifact path string at `tauri_commands.rs:543,581,600` (`data_dir.join("harness")`) — keep as `"harness"` to avoid orphaning existing eval artifacts on dev machines. **Add a single-line comment at each site** explaining the historical name.

### Steps

- [ ] **Step 3.1: Confirm Tasks 1+2 are clean**

```bash
grep -rn "crate::harness::trajectory\|crate::harness::TrajectoryStore\|crate::harness::budget\|crate::harness::ToolBudgetManager" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Expected: **empty** (Tasks 1+2 already migrated these to `crate::agent::*`). Any hit → STOP, BLOCKED (Task 1 or 2 incomplete).

- [ ] **Step 3.2: Git-move the directory**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename mv \
    src-tauri/src/harness src-tauri/src/eval
```

Verify:
```bash
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/harness 2>&1
# expect: No such file or directory
ls /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/eval/
# expect: 16 files (mod.rs, runtime.rs, ..., adapters/)
```

- [ ] **Step 3.3: Update `lib.rs`**

```bash
grep -n "pub mod harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/lib.rs
```

Use Edit to change `pub mod harness;` → `pub mod eval;` (preserve any preceding doc comment, but if that comment SPECIFICALLY mentions "harness module" / "offline harness", update it to "eval module" / "offline eval" — preserve the rest of the comment).

- [ ] **Step 3.4: Update intra-eval `crate::harness::*` references (59 self-refs)**

```bash
grep -rn "crate::harness::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/eval/ --include="*.rs"
```

For each match, Edit `crate::harness::` → `crate::eval::`. This may be 30-60 distinct edits across ~15 files. Use the Edit tool, NOT sed.

Re-grep after edits:
```bash
grep -rn "crate::harness::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/eval/ --include="*.rs"
```
Expected: **empty**.

- [ ] **Step 3.5: Update cross-tree `crate::harness::*` references**

```bash
grep -rn "crate::harness::" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Expected: hits in `tauri_commands.rs` (~30) and `memory_policy/receipts.rs` (3). Anything else → STOP and inspect.

For each, Edit `crate::harness::` → `crate::eval::`.

Re-grep:
```bash
grep -rn "crate::harness::\|use crate::harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected: **empty**.

- [ ] **Step 3.6: Add on-disk path comments**

At each of `tauri_commands.rs:543, :581, :600` (the `data_dir.join("harness")` sites — verify exact line numbers with `grep -n 'join("harness")' src-tauri/src/tauri_commands.rs`), add a single-line comment ABOVE the `.join("harness")` line:

```rust
// Kept as "harness" intentionally: preserves backward-compat with existing on-disk eval artifacts.
```

(Or fold into the line's existing trailing comment if any.)

- [ ] **Step 3.7: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: **empty**. If errors:
- Most likely missed `crate::harness::` import (re-run Step 3.5 grep).
- Or a doc comment with `[\`harness::Foo\`]` rustdoc link that doesn't compile error but generates a doc warning — those are fine, ignore.

- [ ] **Step 3.8: Regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: 759 passed / 2 failed.

- [ ] **Step 3.9: Final orphan sweep**

```bash
grep -rn "crate::harness\|use crate::harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Expected: **empty**.

- [ ] **Step 3.10: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename status -sb
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename diff --cached --stat | tail -5
# (Stage everything that changed.)

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename add -A src-tauri/src/

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename commit -m "$(cat <<'EOF'
refactor(eval): rename harness/ directory to eval/ (P3.3 of 阶段 2)

Pure directory rename — no behavior change. The remaining harness/
subtree (16 files / ~7,000 LoC of offline eval machinery: HarnessRuntime,
campaign/case/episode runner, graders, performance_scorecard,
self_improvement, memory_inventory, trace, artifacts, adapters/*) moves
to eval/. Frees up the harness/ name for the future autonomy
supervisor per ADR §10.

- `pub mod harness;` → `pub mod eval;` in lib.rs
- ~59 intra-tree `crate::harness::*` → `crate::eval::*` references
  inside the moved subtree
- ~30 cross-tree references updated in tauri_commands.rs
- 3 references in memory_policy/receipts.rs (P5-scheduled file, but
  must compile in the meantime)
- The on-disk artifact path string `data_dir.join("harness")` at
  3 sites in tauri_commands.rs is kept as "harness" to preserve
  backward-compat with existing dev-machine eval artifacts;
  single-line comments added explaining the historical name

Tauri command names (`run_*_harness`) and internal type names
(HarnessRuntime, HarnessEvent, …) are renamed in Tasks 4-5.

cargo build clean; agent:: 759/2 at baseline.
EOF
)"
```

Record the commit SHA. Continue to Task 4.

---

## Task 4: Rename Rust types `*Harness*` → `*Eval*`

**Goal**: Inside `eval/` (and the 1 cross-tree consumer at `memory_policy/receipts.rs`), rename Rust types whose names contain the `Harness` substring so the module rename is semantically symmetric. After Task 4, the only places `harness` still appears in the source tree are:
- Tauri command names `run_*_harness` (Task 5 renames these).
- UI strings/types (Task 5).
- The 3 on-disk path strings `"harness"` (intentionally preserved).
- Doc comments and string literals where "harness" is a noun describing the historical name.

**Type renames (verified targets):**

| Old | New | Defined in |
|---|---|---|
| `HarnessRuntime` | `EvalRuntime` | `eval/runtime.rs` |
| `HarnessEvent` | `EvalEvent` | `eval/trace.rs` |
| `MemoryHarnessTarget` | `MemoryEvalTarget` | `eval/trace.rs` |
| `MemoryGbrainHarnessAdapter` | `MemoryGbrainEvalAdapter` | `eval/adapters/memory.rs` |
| `BrowserHarnessAdapter` | `BrowserEvalAdapter` | `eval/adapters/browser.rs` |
| `AgentLoopControlPlaneHarnessAdapter` | `AgentLoopControlPlaneEvalAdapter` | `eval/adapters/agent_loop.rs` |

### Steps

- [ ] **Step 4.1: Confirm Task 3 is clean** (`crate::harness` is gone)

```bash
grep -rn "crate::harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Expected: **empty**.

- [ ] **Step 4.2: List all candidate identifiers**

```bash
grep -rn "\b\(HarnessRuntime\|HarnessEvent\|MemoryHarnessTarget\|MemoryGbrainHarnessAdapter\|BrowserHarnessAdapter\|AgentLoopControlPlaneHarnessAdapter\)\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Inspect the output. Confirm each match is the identifier (not just a doc-comment mention). For doc comments that mention these by name in markdown links (e.g., `[\`HarnessRuntime\`]`), they should rename too (otherwise the link is dead).

Also check for any Harness-prefixed identifier missed by the scan:
```bash
grep -rn "\bHarness[A-Z]" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

If new candidates appear (e.g., `HarnessReporter`, `HarnessTrace`, etc.), add them to the rename list. If unsure, halt and report NEEDS_CONTEXT with the candidate name + its definition site.

- [ ] **Step 4.3: Rename each identifier (use Edit, NOT sed)**

For each of the 6 types in the table, find its definition and all uses with grep, then use Edit's `replace_all` to rename. Order doesn't matter (each rename is independent).

After each Edit, run a confirmation grep:
```bash
grep -rn "\b<OLD-NAME>\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected after each: **empty** (or only matches inside doc strings explaining the rename, which is acceptable).

- [ ] **Step 4.4: Update lowercase `harness` in module docs/comments inside eval/**

```bash
grep -rn "\bharness\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/eval/ --include="*.rs"
```

Inspect results. For each lowercase `harness` inside a doc comment, function comment, error string, or variable name where it refers to the (now-renamed) module, change to `eval`. Exceptions:
- "harness" inside a string literal that is a stable disk-path component → keep as `"harness"`.
- "harness" inside an error message that ALSO references the historical name as documentation → judgment call (prefer "eval" for new code; keep "harness" if it's part of a saved JSON schema).

When in doubt about a specific occurrence, leave as-is and add to a `DEFERRED` list in the commit body.

- [ ] **Step 4.5: Update doc comments mentioning the renamed types**

After identifier renames, the rustdoc cross-link should still work. Verify by grep:
```bash
grep -rn "\[\`Harness\|\[Harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected: empty (any rustdoc-link to a Harness-prefixed type should have updated when its identifier was replaced; if any remain, fix them).

- [ ] **Step 4.6: Build (GREEN GATE) + regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; 759 passed / 2 failed.

- [ ] **Step 4.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename add -A src-tauri/src/

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename commit -m "$(cat <<'EOF'
refactor(eval): rename *Harness* Rust types to *Eval* (P3.4 of 阶段 2)

Six internal type names renamed for symmetry with the harness/→eval/
directory rename in P3.3:

  HarnessRuntime                       -> EvalRuntime
  HarnessEvent                         -> EvalEvent
  MemoryHarnessTarget                  -> MemoryEvalTarget
  MemoryGbrainHarnessAdapter           -> MemoryGbrainEvalAdapter
  BrowserHarnessAdapter                -> BrowserEvalAdapter
  AgentLoopControlPlaneHarnessAdapter  -> AgentLoopControlPlaneEvalAdapter

Doc comments and lowercase `harness` references inside eval/ also
updated where they refer to the renamed module. The on-disk path
string `data_dir.join("harness")` is intentionally preserved
(backward-compat with existing dev-machine eval artifacts; see P3.3
commit body).

Tauri command names (`run_*_harness`) and UI surface (HarnessButton,
HarnessKind, etc.) renamed in P3.5.

cargo build clean; agent:: 759/2 at baseline.
EOF
)"
```

Record the commit SHA. Continue to Task 5.

---

## Task 5: Rename Tauri commands `run_*_harness` → `run_*_eval` + sync UI

**Goal**: Atomic rename of the IPC surface — 4 Tauri command names + 1 internal helper function name + UI command-string call sites + UI TS interfaces + UI component/handler/state names + UI Chinese label. Done in ONE commit so the IPC boundary lands atomically (any intermediate state would have backend ≠ frontend invoking).

**Backend renames (`src-tauri/`):**

| Old function name | New function name |
|---|---|
| `run_memory_gbrain_eval_harness` | `run_memory_gbrain_eval` |
| `run_browser_parity_harness` | `run_browser_parity_eval` |
| `run_agent_control_plane_harness` | `run_agent_control_plane_eval` |
| `run_self_improvement_gate_harness` | `run_self_improvement_gate_eval` |
| `build_memory_gbrain_eval_harness_report` | `build_memory_gbrain_eval_report` |

Sites:
- `src-tauri/src/tauri_commands.rs:536, 558, 571, 575, 594, 610, 764, 816` — function defs + internal call + 2 test refs.
- `src-tauri/src/main.rs:1259-1262` — invoke_handler entries.

**Frontend renames (`ui/`):**

`ui/src/components/settings/SystemTab.tsx`:

| Old | New |
|---|---|
| TS interface `HarnessCheckResult` | `EvalCheckResult` |
| TS interface `HarnessScorecard` | `EvalScorecard` |
| TS interface `HarnessSuiteReport` | `EvalSuiteReport` |
| TS type `HarnessKind` | `EvalKind` |
| const `harnessCommands` | `evalCommands` |
| state `harnessBusy`/`setHarnessBusy` | `evalBusy`/`setEvalBusy` |
| state `harnessReports`/`setHarnessReports` | `evalReports`/`setEvalReports` |
| handler `handleHarnessRun` | `handleEvalRun` |
| handler `handleRunAllHarnesses` | `handleRunAllEvals` |
| component `HarnessButton` | `EvalButton` |
| helper `normalizeHarnessReport` | `normalizeEvalReport` |
| Chinese label `"Harness 评估"` | `"评估套件"` (drops the English noun entirely; the Chinese label is now self-contained) |
| Command strings `run_*_harness` (×4) | `run_*_eval` |

`ui/src/components/settings/SystemTab.test.tsx`:
- Test descriptions: `"SystemTab harness reporting"` → `"SystemTab eval reporting"`, etc.
- All mock `invoke` arm command strings: `run_*_harness` → `run_*_eval` (4 sites).

### Steps

- [ ] **Step 5.1: Confirm Task 4 is clean**

```bash
grep -rn "\b\(HarnessRuntime\|HarnessEvent\|MemoryHarnessTarget\|MemoryGbrainHarnessAdapter\|BrowserHarnessAdapter\|AgentLoopControlPlaneHarnessAdapter\)\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```

Expected: **empty**.

- [ ] **Step 5.2: Rename the 5 Rust functions**

For each of the 5 function-name pairs in the backend table above, find the definition + ALL call sites, then Edit. Order: rename `build_memory_gbrain_eval_harness_report` first (it's only called from one Tauri command + 2 tests), then the 4 Tauri commands.

After each rename, grep to confirm the OLD name is gone:
```bash
grep -rn "\b<OLD-FN-NAME>\b" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
```
Expected: empty.

- [ ] **Step 5.3: Update `main.rs` invoke_handler entries**

```bash
grep -n "run_.*_harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/main.rs
```

Use Edit to replace each `run_*_harness` reference with `run_*_eval`.

Verify: `grep -n "run_.*_harness" src-tauri/src/main.rs` → empty.

- [ ] **Step 5.4: Build (GREEN GATE)** — Rust side only at this point

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```

Expected: empty errors; 759 passed / 2 failed.

- [ ] **Step 5.5: Update `ui/src/components/settings/SystemTab.tsx`**

Read the file first. The renames in the frontend table above can be done via Edit's `replace_all` on each token (because each TS identifier is unique within the file). Recommended order:
1. Type/interface names (5: `HarnessCheckResult`, `HarnessScorecard`, `HarnessSuiteReport`, `HarnessKind`, `harnessCommands`).
2. State variable names + setters (4: `harnessBusy`, `setHarnessBusy`, `harnessReports`, `setHarnessReports`).
3. Function names (3: `handleHarnessRun`, `handleRunAllHarnesses`, `normalizeHarnessReport`).
4. Component name (1: `HarnessButton`).
5. Chinese label string `"Harness 评估"` → `"评估套件"`.
6. Command strings (4: `run_*_harness` → `run_*_eval`).

After each rename, sanity-check with grep:
```bash
grep -n "<OLD-TOKEN>" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui/src/components/settings/SystemTab.tsx
```
Expected after each: empty.

- [ ] **Step 5.6: Update `ui/src/components/settings/SystemTab.test.tsx`**

Same procedure. Test descriptions + mock invoke command strings.

Final whole-UI sweep:
```bash
grep -rn "\bHarness\|harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui/src/ --include="*.ts" --include="*.tsx" | grep -v node_modules
```
Expected: empty (or only "harness" inside a non-eval test file — unlikely; investigate any hit).

- [ ] **Step 5.7: TypeScript check + UI test run**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui && npx tsc --noEmit 2>&1 | head -10
```

Expected: empty (no TS errors).

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui && npm test -- --run 2>&1 | tail -10
```

Expected: 1090 passed / 2 failed (same pre-existing ConnectivityTab.test.tsx failures from baseline). If `SystemTab.test.tsx` fails, it means a mock invoke arm or test description was missed — fix and re-run.

- [ ] **Step 5.8: Final cargo build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```

Expected: empty.

- [ ] **Step 5.9: Final orphan sweep across whole worktree**

```bash
grep -rn "crate::harness\|\bHarnessRuntime\|\bHarnessEvent\b\|\bMemoryHarnessTarget\b\|\bMemoryGbrainHarnessAdapter\b\|\bBrowserHarnessAdapter\b\|\bAgentLoopControlPlaneHarnessAdapter\b\|run_.*_harness" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/src-tauri/src/ --include="*.rs"
grep -rn "\bHarness[A-Z]\|run_.*_harness\|harnessCommands\|harnessBusy\|harnessReports\|HarnessButton\|HarnessKind" /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename/ui/src/ --include="*.ts" --include="*.tsx"
```

Expected: **both empty**.

- [ ] **Step 5.10: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename status -sb
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename add -A \
    src-tauri/src/tauri_commands.rs \
    src-tauri/src/main.rs \
    ui/src/components/settings/SystemTab.tsx \
    ui/src/components/settings/SystemTab.test.tsx

git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename commit -m "$(cat <<'EOF'
refactor(eval+ui): rename Tauri eval commands and UI surface (P3.5 of 阶段 2)

Atomic IPC rename — backend + frontend in one commit so no
intermediate state has backend ≠ frontend command names. Completes
P3 of the 阶段 2 skeleton cleanup series.

Backend (`src-tauri/`):
- 4 Tauri commands renamed:
    run_memory_gbrain_eval_harness    -> run_memory_gbrain_eval
    run_browser_parity_harness        -> run_browser_parity_eval
    run_agent_control_plane_harness   -> run_agent_control_plane_eval
    run_self_improvement_gate_harness -> run_self_improvement_gate_eval
- 1 internal helper renamed:
    build_memory_gbrain_eval_harness_report -> build_memory_gbrain_eval_report
- main.rs `tauri::generate_handler!` entries updated.

Frontend (`ui/src/components/settings/SystemTab.tsx` + test):
- TS interfaces: HarnessCheckResult/Scorecard/SuiteReport -> Eval*
- Types/consts: HarnessKind -> EvalKind; harnessCommands -> evalCommands
- State: harnessBusy/Reports -> evalBusy/Reports
- Handlers: handleHarnessRun/RunAllHarnesses -> handleEvalRun/RunAllEvals
- Component: HarnessButton -> EvalButton
- Helper: normalizeHarnessReport -> normalizeEvalReport
- Chinese label: "Harness 评估" -> "评估套件" (self-contained Chinese)
- Mock invoke arms in test updated to new command strings.

Completes the harness→eval rename arc started by P3.1-4:
- P3.1: harness/trajectory.rs -> agent/trajectory.rs (load-bearing)
- P3.2: harness/budget.rs -> agent/tool_budget.rs (load-bearing)
- P3.3: harness/ directory -> eval/ + crate::harness:: import paths
- P3.4: *Harness* Rust types -> *Eval*
- P3.5: Tauri command names + UI surface

The on-disk artifact path `data_dir.join("harness")` (3 sites in
tauri_commands.rs) is intentionally preserved for backward-compat
with existing dev-machine eval artifacts.

cargo build clean; agent:: 759/2 at baseline.
ui npm test 1090/2 at baseline (same pre-existing ConnectivityTab.test.tsx).

The harness/ namespace is now free for the future autonomy supervisor
per ADR §10.
EOF
)"
```

Verify chain:
```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename log --oneline 81f0ac79..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/skeleton-cleanup-p3-harness-split-rename status -sb
```

Expected: 5 commits ahead of `main`; working tree clean.

---

## Self-Review

**1. Spec coverage (against assessment §1.C "harness/ split-rename" + Open Decision #2 "选项 A 单独 P3"):**

- ✅ Extract `trajectory.rs` → `agent/trajectory.rs` (assessment line: "抽 `trajectory.rs` → `agent/trajectory.rs`(或 `db/`)") → Task 1
- ✅ Extract `budget.rs` → `agent/tool_budget.rs` (assessment line: "抽 `budget.rs` → `agent/tool_budget.rs`") → Task 2
- ✅ Rename remaining `harness/` → `eval/` → Task 3
- ✅ Internal `*Harness*` Rust types → `*Eval*` → Task 4 (NOT explicitly in assessment but required for clean rename — added per grill answer "改成 run_*_eval + UI 同步" implies semantic consistency)
- ✅ 4 Tauri command names → `run_*_eval` (assessment said "7 Tauri 评估命令名 + import 全更新" — actually 4 commands + 1 helper, corrected by recon) → Task 5
- ✅ UI sync (SystemTab.tsx + test) → Task 5 (combined with Tauri rename for atomic IPC boundary)

The assessment's "7 Tauri commands" count was off — actual is 4 commands + 1 helper. Plan corrects this with precise recon.

**2. Placeholder scan:**
- No "TBD" / "TODO" / "implement later" / "similar to Task N".
- Step 4.4 "judgment call" language is intentional micro-flexibility for lowercase-`harness`-in-comments cleanup — the rule is "prefer 'eval' for new code; keep 'harness' if it's part of a saved schema". A `DEFERRED` list in the commit body absorbs edge cases.
- "Add a single-line comment" in Step 3.6 is explicit instruction, not a placeholder.

**3. Type consistency:**
- `TrajectoryStore`, `TurnRecord`, `TrajectorySearchHit` named consistently across Tasks 1, 3, 5.
- `ToolBudgetManager` named consistently.
- `HarnessRuntime`, `HarnessEvent`, `MemoryHarnessTarget`, `MemoryGbrainHarnessAdapter`, `BrowserHarnessAdapter`, `AgentLoopControlPlaneHarnessAdapter` named consistently across §"Background facts" and Task 4.
- Tauri command names `run_*_harness` / `run_*_eval` listed verbatim in §"Background facts" and Task 5.
- UI TS identifier renames listed verbatim — old/new symmetric.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 0.5-1 person-day (5 commits, mostly mechanical search-and-replace; the largest commit is Task 3 with ~30 cross-tree path edits + 59 intra-tree edits, all `crate::harness::` → `crate::eval::`).
- **Risk:** medium. The Task 3 build-gate after the directory rename is the highest-risk single step (any missed `crate::harness::` import breaks build). Task 5 atomically lands the IPC boundary — if cargo build OR `cd ui && npm test` fails after Task 5, the commit gets amended before push (since this is the last commit).
- **Files touched:**
  - Task 1: 7 (1 move + 5 import updates + 2 mod.rs wire/unwire)
  - Task 2: 6 (1 move+rename + 3 import updates + 2 mod.rs wire/unwire)
  - Task 3: 1 directory move + 1 lib.rs + ~20 files with import edits (5 cross-tree + ~15 intra-eval)
  - Task 4: 6 type defs + their use sites (~20-40 sites across ~10 files)
  - Task 5: 2 backend files (tauri_commands.rs, main.rs) + 2 UI files (SystemTab.tsx, SystemTab.test.tsx)
- **Net LoC:** ~0 (pure mechanical rename). All commit bodies note "Zero behavior change".
- **PR shape:** 1 worktree → 5 commits → 1 PR. Bisectable per-task — each commit builds green + agent:: tests at baseline. Task 5 also gates UI test suite. Squash-on-land per P1/P2 convention.
- **No new tests written.** No tests deleted. Test count unchanged: `cargo test --lib agent::` 759/2 throughout; `cargo test --lib` total unchanged (3044/7); `npm test -- --run` unchanged (1090/2).
- **No Open Decisions block P3.** All 4 grill questions answered:
  1. `agent/trajectory.rs` (recommended)
  2. `agent/tool_budget.rs` (recommended)
  3. `run_*_eval` + UI sync (recommended)
  4. TS interface renames in same PR (recommended)
