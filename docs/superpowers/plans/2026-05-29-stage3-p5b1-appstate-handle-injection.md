# 阶段 3 P3-5b1 — `ChatDelegate` AppState Handle Lookup · Implementation Plan (v2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **v2 NOTE:** v1 (committed at `272e61cc` then revised) attempted to plumb `Arc<AppState>` through `ChatDelegate::new()`. Mid-Task-1 it surfaced that Tauri's managed-state API gives `tauri::State<'_, T>` (a `&T` ref) not `Arc<T>`, and that converting requires refactoring 356 state bindings — out of scope. v2 pivots to the **existing in-codebase pattern at `dispatcher/turn_runner.rs:657`**: `self.app_handle.try_state::<AppState>()`. `ChatDelegate` already holds `app_handle: tauri::AppHandle`; we add two thin helper methods (`app_state()` + `try_app_state()`) and drop the redundant fields. No `Arc<AppState>` plumbing, no Tauri state refactor, no test_stub needed.

**Goal:** Drop 8 redundant `ChatDelegate` fields by looking up AppState through the existing `app_handle: tauri::AppHandle` field. Fields removed: `safety_manager`, `pending_approvals`, `db`, `learning_db`, `gbrain_extract_db`, `persist_db`, `gbrain_extract_mcp_mgr`, `hook_bus`. Field count drops 53 → ~46. Closer to the spec target of ~30; the remaining gap is closed by P3-5b2 (session-pipeline bundling). ContentBlock dedup is the orthogonal P3-5b3.

**Architecture:** `ChatDelegate.app_handle` is a Tauri `AppHandle` that has access to the process-scope `AppState` via `app_handle.state::<AppState>()` (panics if not managed) or `app_handle.try_state::<AppState>()` (None-tolerant). The dispatcher ALREADY uses this pattern once (`turn_runner.rs:657` `let Some(state) = app_handle.try_state::<crate::app::AppState>() else { return; }`). We make the pattern uniform.

After 5b1, ChatDelegate has TWO thin accessor methods:
- `pub(super) fn app_state(&self) -> tauri::State<'_, AppState>` — for hot-path reads where AppState IS managed (the production agent loop). Panics with a documented invariant if not.
- `pub(super) fn try_app_state(&self) -> Option<tauri::State<'_, AppState>>` — for paths that need None-tolerance (e.g., they previously branched on `if let Some(db) = &self.db`).

Reads of the 8 dropped fields become `self.app_state().X.clone()` or `self.try_app_state().map(|s| s.X.clone())`. Setters that previously injected these fields are deleted.

**Tech Stack:** Rust 2021, no new crates.

**Related design:**
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §6.2 (field reduction strategy — note that v1 of this plan assumed `Arc<AppState>`, v2 corrects to `tauri::State<AppState>` lookup).
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.1 (god-object critique).
- 5a structural foundation: [`2026-05-29-stage3-p5a-dispatcher-structural-split.md`](2026-05-29-stage3-p5a-dispatcher-structural-split.md), merged at `0da7bada`.

---

## Recon-discovered facts vs. v1 plan assumptions

| v1 assumption | v2 correction (verified against `272e61cc` worktree, 2026-05-29) |
|---|---|
| Tauri's managed-state exposes `Arc<AppState>` | False — Tauri exposes `tauri::State<'_, T>` (a deref wrapper around `&T`). Getting `Arc<T>` requires registering as `app.manage(Arc::new(T))` instead of `app.manage(T)`. The codebase uses the latter, with 356 `tauri::State<'_, AppState>` usages — refactoring all of them is out of scope. |
| `AppState::test_stub()` helper needed for tests | False — dispatcher tests never read `self.safety_manager` / `self.db` / `self.mcp_manager` etc. directly (they exercise pure prompt-assembly and signal-detection helpers). After we drop the redundant fields, tests just stop passing the corresponding `new()` args. AppState doesn't need to exist in tests at all. |
| Field count drops to 46 (`+1 app -8 = -7 net`) | Confirmed — but the `+1` is NOT a new field, just two accessor methods. So 53 - 8 = **45** net. |
| Pattern is new to the codebase | False — `dispatcher/turn_runner.rs:657` already uses `app_handle.try_state::<AppState>()`. We're just making it uniform across all subsystems. |

### Fields to drop in 5b1 (revised count: 8 fields → -8 net)

Same 8 fields as v1, same justification, different access pattern:

| ChatDelegate field | Today | Reach via | Optionality today | Notes |
|---|---|---|---|---|
| `safety_manager` | required `new()` param | `self.app_state().safety_manager` | Required | One caller — `safety_gate::resolve_effective_mode`. |
| `pending_approvals` | required `new()` param | `self.app_state().pending_approvals` | Required | ~5 callers across turn_runner. |
| `hook_bus` | required `new()` param | `self.app_state().agent_api.hook_bus()` (returns `Option<&Arc<HookBus>>`) | Required | P3-3 wired into AgentApi; production always has Some. Use `expect("hook_bus wired at boot")` for hot path. |
| `db` | set via `set_db()` (Option) | `self.app_state().db.clone()` | Option | Drop the `set_db` method entirely; reads become unconditional. |
| `learning_db` | set via `set_learning_pipeline()` (Option) | `self.app_state().db.clone()` (same conn) | Option | Drop the `learning_db` field + param from setter; reads become unconditional. |
| `gbrain_extract_db` | set via `set_gbrain_extractor_pipeline()` (Option) | `self.app_state().db.clone()` (same conn) | Option | Same. |
| `persist_db` | set via `with_agent_queues()` (Option) | `self.app_state().db.clone()` (same conn) | Option | Same. |
| `gbrain_extract_mcp_mgr` | set via `set_gbrain_extractor_pipeline()` (Option) | `self.app_state().mcp_manager.clone()` | Option | Same. |

All 4 DB fields hold clones of `app.db` (verified at `app.rs:967` where mcp_manager and safety_manager are constructed alongside db). 5b1 collapses them via the helpers.

### Baselines to hold

- `cargo build`: 0 errors, **≤50 warnings** (post-5a baseline).
- `cargo test --lib agent::dispatcher`: **50 passed**.
- `cargo test --lib agent::`: **796 passed / 2 pre-existing failed**.
- `cargo test --lib` total: **3,050 passed / 7 pre-existing failed**.

### External callers (must compile cleanly)

All 3 `ChatDelegate::new` sites live in `src-tauri/src/tauri_commands.rs` (lines ~1983, ~11222, ~15090). Each currently passes `safety_manager`, `pending_approvals`, `hook_bus` as args. After 5b1 they STOP passing those — the dispatcher reads them via its own `app_handle`. Tests update similarly.

---

## Target shape

### Before 5b1 — ChatDelegate `new()` signature

```rust
pub fn new(
    llm: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    app_handle: tauri::AppHandle,
    model: String,
    system_prompt: String,
    safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,
    safety_mode: Option<SafetyMode>,
    pending_approvals: Arc<PendingApprovals>,
    conversation_id: String,
    workspace_root: Option<std::path::PathBuf>,
    hook_bus: Arc<crate::agent::hook_bus::HookBus>,
) -> Self
```

### After 5b1 — ChatDelegate `new()` signature

```rust
pub fn new(
    llm: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    app_handle: tauri::AppHandle,
    model: String,
    system_prompt: String,
    safety_mode: Option<SafetyMode>,
    conversation_id: String,
    workspace_root: Option<std::path::PathBuf>,
) -> Self
```

11 params → 8. Dropped: `safety_manager`, `pending_approvals`, `hook_bus`. Subsystem reads route through `self.app_state()`.

### After 5b1 — accessor methods (added to `mod.rs`'s `impl ChatDelegate`)

```rust
/// Look up the process-scope AppState through the Tauri AppHandle.
///
/// This is the canonical replacement for the 8 `ChatDelegate` fields
/// dropped in P3-5b1 (safety_manager, pending_approvals, hook_bus via
/// agent_api, 4 DB clones, mcp_manager). Reads forward as
/// `self.app_state().subsystem.clone()`.
///
/// PANICS if AppState is not registered on this Tauri AppHandle. In
/// production this is wired by `AppState::new()` at boot, so the
/// invariant holds for every code path the agent loop reaches. In
/// tests, the dispatcher's pure-helper tests never trigger this path —
/// they exercise prompt-assembly + signal-detection helpers that don't
/// reach subsystem reads. If a future test DOES need AppState, prefer
/// `try_app_state` and gate the test on `Some`.
pub(super) fn app_state(&self) -> tauri::State<'_, crate::app::AppState> {
    self.app_handle.state::<crate::app::AppState>()
}

/// None-tolerant variant of `app_state()`. Used by paths that already
/// branched on Option semantics in the dropped fields (e.g. test
/// helpers that constructed ChatDelegate without managed AppState).
pub(super) fn try_app_state(&self) -> Option<tauri::State<'_, crate::app::AppState>> {
    self.app_handle.try_state::<crate::app::AppState>()
}
```

### After 5b1 — field count

- 53 (pre-5b1) − 8 (dropped) + 0 (no new fields) = **45 fields**.
- Remaining duplicates/bundlable concerns for 5b2: 16 session-config fields.

---

## Pre-flight (worktree already exists at `claude/stage3-p5b1-appstate-handle`)

The worktree was created before v1 of this plan. The v1 Task 1 commit (`2f565729`) was reverted; the branch is back at `272e61cc` (plan commit).

1. **Confirm worktree state:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle status -sb
   git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle log --oneline -3
   ```
   Expected: clean tree at `272e61cc`.

2. **Capture baselines:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```
   Expected: 50 warnings, dispatcher 50/0, agent:: 796/2.

---

## Task 1: Add `app_state()` + `try_app_state()` accessor methods

**Why this task exists first:** The accessors are the seam every subsequent task depends on. Adding them in isolation, with no field drops yet, lets the rest of the plan be pure deletion.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (add the two accessor methods to the main `impl ChatDelegate` block)

### Steps

- [ ] **Step 1.1: Locate the main `impl ChatDelegate` block in mod.rs**

  ```bash
  grep -n "^impl ChatDelegate" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/mod.rs
  ```

- [ ] **Step 1.2: Add the two accessor methods**

  Insert right after `pub fn new(...)`:

  ```rust
  /// Look up the process-scope AppState through the Tauri AppHandle.
  ///
  /// This is the canonical replacement for the 8 `ChatDelegate` fields
  /// dropped in P3-5b1 (safety_manager, pending_approvals, hook_bus via
  /// agent_api, 4 DB clones, mcp_manager). Reads forward as
  /// `self.app_state().subsystem.clone()`.
  ///
  /// PANICS if AppState is not registered on this Tauri AppHandle. In
  /// production this is wired by `AppState::new()` at boot, so the
  /// invariant holds for every code path the agent loop reaches.
  pub(super) fn app_state(&self) -> tauri::State<'_, crate::app::AppState> {
      self.app_handle.state::<crate::app::AppState>()
  }

  /// None-tolerant variant of `app_state()`. For paths that previously
  /// tolerated Option semantics on the dropped fields.
  pub(super) fn try_app_state(&self) -> Option<tauri::State<'_, crate::app::AppState>> {
      self.app_handle.try_state::<crate::app::AppState>()
  }
  ```

- [ ] **Step 1.3: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  ```
  Expected: 0 errors, **≤52 warnings** (50 baseline + 2 new `dead_code` warnings for the accessors until Tasks 2+ wire them).
  dispatcher 50/0.

- [ ] **Step 1.4: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/agent/dispatcher/mod.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): add app_state() + try_app_state() accessors on ChatDelegate (P3-5b1.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Drop the 4 DB clones (db, learning_db, gbrain_extract_db, persist_db) → `self.try_app_state().map(|s| s.db.clone())`

**Goal:** Remove the 4 Option<Arc<Mutex<Connection>>> fields. Replace all reads with the accessor.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop 4 fields from struct + struct literal; delete `set_db` method; drop DB params from `with_agent_queues`, `set_learning_pipeline`, `set_gbrain_extractor_pipeline`)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs`, `model_io.rs`, `content_assembler.rs`, `observability.rs` (every `self.{db,learning_db,gbrain_extract_db,persist_db}` read → accessor)
- Modify: `src-tauri/src/tauri_commands.rs` (drop the DB args from the 3 callers' setter chains)

### Steps

- [ ] **Step 2.1: Inventory reads**

  ```bash
  grep -rnE "self\.(db|learning_db|gbrain_extract_db|persist_db)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  For each match, decide:
  - Site that already had `if let Some(db) = self.db.as_ref()` → use `if let Some(state) = self.try_app_state() { let db = state.db.clone(); ... }` to preserve None-tolerance.
  - Site that already unwrapped or assumed `Some` → use `let db = self.app_state().db.clone()` (will panic if AppState missing, but production always has it).

  Most learning/gbrain extractor paths spawn background tasks; they took the field by `Option`/`Some` branches. After collapse, the path simply doesn't branch — it always has db.

- [ ] **Step 2.2: Drop the 4 fields**

  In `mod.rs`'s `pub struct ChatDelegate { ... }`:
  - Remove `db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,` + the matching `db: None,` initializer in `new()`'s struct literal.
  - Remove `learning_db`, `gbrain_extract_db`, `persist_db` likewise.

- [ ] **Step 2.3: Update setters in mod.rs**

  - `pub fn set_db(&mut self, db: Arc<Mutex<Connection>>)` → DELETE the method entirely.
  - `pub fn with_agent_queues(mut self, queues: AgentQueues, db: Arc<Mutex<Connection>>) -> Self` → drop the `db` param. Body no longer assigns `self.persist_db`.
  - `pub fn set_learning_pipeline(...)` → drop the `learning_db` param if present.
  - `pub fn set_gbrain_extractor_pipeline(...)` → drop the `gbrain_extract_db` param if present.

- [ ] **Step 2.4: Replace reads**

  Walk through each site from Step 2.1. Apply the transform:
  - `self.db.clone()` (returned `Option<...>`) → `self.try_app_state().map(|s| s.db.clone())`
  - `if let Some(db) = self.db.as_ref()` → `if let Some(state) = self.try_app_state() { let db = &state.db; ... }`
  - `self.db.as_ref().unwrap()` → `self.app_state().db.clone()`

  Be careful with borrow lifetimes — `tauri::State<'_, AppState>` deref's to `&AppState`, and the borrow lasts as long as the State itself. If you need to clone the Arc and let go of the State (e.g., for spawning a background task), do `let db = self.app_state().db.clone();` then drop the State (it's RAII).

- [ ] **Step 2.5: Update tauri_commands.rs callers**

  ```bash
  grep -n "\.set_db(\|\.with_agent_queues(\|\.set_learning_pipeline(\|\.set_gbrain_extractor_pipeline(" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/tauri_commands.rs
  ```

  Each call site loses the DB arg. The setter chain looks like:
  ```rust
  delegate
      .with_agent_queues(queues)  // was: with_agent_queues(queues, db.clone())
      .set_learning_pipeline(llm, buffer, budget)  // was: .set_learning_pipeline(llm, db.clone(), buffer, budget)
      ...
  ```

- [ ] **Step 2.6: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: 0 errors, ≤55 warnings, dispatcher 50/0, agent:: 796/2.

  If tests FAIL (not just compile), most likely cause: a test path now hits `app_state()`'s panic because the test app_handle has no managed AppState. Fix by either:
  - Switching the production read to `try_app_state()` if None-tolerance is semantically correct.
  - Or having the test register an AppState via `mock_app.manage(some_state)` (only viable if test_stub helpers exist; for 5b1 we expect zero such test failures).

- [ ] **Step 2.7: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop 4 redundant DB fields → self.app_state().db (P3-5b1.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: Drop `gbrain_extract_mcp_mgr` → `self.app_state().mcp_manager`

Mechanical, narrowest task. Same pattern as Task 2 for a single field.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop field + struct literal entry + drop mcp_mgr param from `set_gbrain_extractor_pipeline`)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs` (or wherever `gbrain_extract_mcp_mgr` reads happen)
- Modify: `src-tauri/src/tauri_commands.rs` (drop mcp_mgr arg from the setter caller)

### Steps

- [ ] **Step 3.1: Audit + replace**

  ```bash
  grep -rnE "self\.gbrain_extract_mcp_mgr\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Replace each site with `self.app_state().mcp_manager.clone()`.

- [ ] **Step 3.2: Drop field + struct literal + setter param**

- [ ] **Step 3.3: Update tauri_commands.rs caller**

- [ ] **Step 3.4: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: same baselines.

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop gbrain_extract_mcp_mgr → self.app_state().mcp_manager (P3-5b1.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Drop `safety_manager` → `self.app_state().safety_manager`

Required field today. Drop drops the corresponding `new()` param + caller args + test fixture args.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop field + struct literal entry + `new()` param)
- Modify: `src-tauri/src/agent/dispatcher/safety_gate.rs` (the one caller — `resolve_effective_mode`)
- Modify: `src-tauri/src/tauri_commands.rs` (drop `safety_manager` arg from 3 callers)
- Modify: any test fixtures in `dispatcher/*.rs` that pass safety_manager to ChatDelegate::new (none expected — pure-helper tests don't construct full ChatDelegate, but verify)

### Steps

- [ ] **Step 4.1: Audit reads**

  ```bash
  grep -rnE "self\.safety_manager\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Expected to be in `safety_gate.rs::resolve_effective_mode` only. Replace with `self.app_state().safety_manager.clone()`.

- [ ] **Step 4.2: Drop field + `new()` param**

  In `mod.rs`:
  - Remove `safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,` field.
  - Drop `safety_manager: Arc<...>,` from `pub fn new(...)` signature.
  - Remove the `safety_manager,` initializer from `new()`'s struct literal.

- [ ] **Step 4.3: Update 3 tauri_commands.rs callers + test fixtures**

  ```bash
  grep -n "ChatDelegate::new" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/tauri_commands.rs
  grep -rn "ChatDelegate::new" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Each site: drop the safety_manager arg.

- [ ] **Step 4.4: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  Test failure mitigation: if a dispatcher test now panics inside `resolve_effective_mode` because `app_state()` finds no managed AppState, the implementer's options are (a) make the test not call into `resolve_effective_mode` (refactor the test setup), (b) switch the read in `safety_gate.rs` to `try_app_state()` returning a default mode if no AppState. Pick (a) — production code should keep its tight invariant.

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop safety_manager field → self.app_state().safety_manager (P3-5b1.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Drop `pending_approvals` → `self.app_state().pending_approvals`

Same shape as Task 4, for `pending_approvals` instead of `safety_manager`. Audit, drop field, drop `new()` param, drop from 3 tauri_commands sites + test fixtures, build, commit.

Replacement read: `self.app_state().pending_approvals.clone()`.

Commit message:
```
refactor(agent): drop pending_approvals field → self.app_state().pending_approvals (P3-5b1.5 of 阶段 3)
```

---

## Task 6: Drop `hook_bus` → `self.app_state().agent_api.hook_bus()`

**Goal:** Remove the last redundant field. Reach via `self.app_state().agent_api.hook_bus()` which returns `Option<&Arc<HookBus>>` (P3-3 wires the bus into AgentApi at AppState construction, so production always returns `Some`).

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop field + new() param + struct literal entry)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs` (and anywhere `self.hook_bus` is read)
- Modify: `src-tauri/src/tauri_commands.rs` (drop `hook_bus` arg from 3 callers)
- Modify: test fixtures (drop `hook_bus` arg from each `ChatDelegate::new(...)` call)

### Steps

- [ ] **Step 6.1: Audit reads**

  ```bash
  grep -rnE "self\.hook_bus\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Each `self.hook_bus.dispatch(...)` becomes:
  ```rust
  self.app_state().agent_api.hook_bus()
      .expect("hook_bus must be wired into AgentApi at AppState boot")
      .dispatch(...)
  ```
  (Use `expect()` for production paths — P3-3 guarantees the bus.)

  For paths that already tolerated None: `if let Some(bus) = self.app_state().agent_api.hook_bus() { bus.dispatch(...).await }`.

  Watch lifetime: `hook_bus()` returns `Option<&Arc<HookBus>>` borrowed from the State; you may need to `cloned()` to extend lifetime past the State drop.

- [ ] **Step 6.2: Drop field + new() param**

- [ ] **Step 6.3: Update callers**

  3 tauri_commands.rs sites + any test fixtures.

- [ ] **Step 6.4: Build + tests + commit**

  Expected: same baselines.

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop hook_bus field → self.app_state().agent_api.hook_bus() (P3-5b1.6 of 阶段 3)"
  ```

Continue to Task 7.

---

## Task 7: Final shape audit + cumulative tests

**Goal:** Confirm ChatDelegate is now 45 fields (53 − 8 = 45), `new()` is 8 params, all baselines preserved.

### Steps

- [ ] **Step 7.1: Field count audit**

  ```bash
  awk '/^pub struct ChatDelegate/,/^}/' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/mod.rs | grep -E "^    [a-z_]+:|^    [a-z_]+ *$" | wc -l
  ```
  Expected: 45.

- [ ] **Step 7.2: `new()` signature audit**

  ```bash
  sed -n '/^    pub fn new(/,/-> Self/p' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/mod.rs
  ```
  Expected: 8 params.

- [ ] **Step 7.3: Final test battery**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib 2>&1 | tail -5
  ```

  Required:
  - 0 errors.
  - ≤55 warnings (target ≤50 — same as 5a baseline; cleanup in 7.4).
  - dispatcher 50/0, agent:: 796/2, cargo test --lib 3050/7.

- [ ] **Step 7.4: Clean unused imports**

  Many `use` lines in mod.rs/turn_runner.rs/etc became unused (PendingApprovals, HookBus types, rusqlite Connection direct refs, SafetyManager direct refs). Clean them up.

- [ ] **Step 7.5: Commit cleanup (if any changes)**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): clean unused imports after P3-5b1 (P3-5b1.7)"
  ```

- [ ] **Step 7.6: Verify final chain**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle log --oneline main..HEAD
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle status -sb
  ```
  Expected: 6-7 commits ahead of main, clean tree.

---

## Self-Review

**1. Spec coverage:**
- ✅ Field reduction: 53 → 45 (5b1's share toward spec's 53→30 target). One field shy of v1's `46` target because we don't add `app: Arc<AppState>`.
- ✅ AppState handle pattern — but as **lookup-via-app_handle**, not as a stored ref. Architecturally simpler than v1.
- 🟡 No ContentBlock dedup. Out of scope. P3-5b3.
- 🟡 No pipeline bundling. Out of scope. P3-5b2.

**2. Placeholder scan:**
- No `unimplemented!()` in shipped code.
- Helper bodies are concrete one-liners. No TODOs.

**3. Type consistency:**
- `self.app_state()` returns `tauri::State<'_, AppState>` everywhere.
- `self.try_app_state()` returns `Option<tauri::State<'_, AppState>>` everywhere.
- Subsystem reads spelled consistently: `self.app_state().X.clone()`.

**4. Bisectability:**
- One field-cohort per commit. Each commit's `cargo test --lib agent::` must pass.
- Helper accessors land in commit 1 in isolation; later commits use them.

**5. Test failure mitigation:**
- Plan anticipates 0-3 test failures during Tasks 2-6 (paths that now hit `app_state()`'s panic in tests without managed AppState). Mitigation: refactor the test setup to either (a) not exercise that path, or (b) use `try_app_state()` if None-tolerance is semantically correct.
- Implementer should NOT degrade production reads to `try_app_state()` just to make tests pass. If that's the only fix, escalate to controller.

---

## Cumulative summary

- **Tasks:** 7 (6 mandatory + 1 optional cleanup).
- **Estimated time:** 1-1.5 person-days.
- **Risk:** Low-medium. The accessor pattern is already in-codebase. Main risk is borrow-lifetime issues with `tauri::State<'_, AppState>` vs spawning background tasks — implementer needs to `.clone()` the Arc out of State before letting it drop.
- **Total commits:** 6-7.

After 5b1 ships: ChatDelegate has 45 fields (down 8 from 53). Two thin accessor methods abstract the AppState lookup. 5b2 closes another ~12 by bundling session config; 5b3 collapses the 5 ContentBlock dup sites.
