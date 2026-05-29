# 阶段 3 P3-5b1 — `ChatDelegate` AppState Handle Injection · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inject `app: Arc<AppState>` into `ChatDelegate`, then drop 8 fields that duplicate AppState (or are derivable from `AppState.agent_api`): `safety_manager`, `pending_approvals`, `db`, `learning_db`, `gbrain_extract_db`, `persist_db`, `gbrain_extract_mcp_mgr`, `hook_bus`. Each redundant field's reads become `self.app.X` lookups (or `self.app.agent_api.hook_bus()` for the last). ChatDelegate field count drops 53 → ~46 (closer to spec target of ~30). The remaining gap to ~30 is closed by P3-5b2 (session-pipeline bundling); ContentBlock dedup is the orthogonal P3-5b3.

**Architecture:** `AppState` is the process-scope handle that the dispatcher's 3 external call sites (all in `tauri_commands.rs`) already have. The dispatcher currently destructures AppState by cloning ~8 individual `Arc<T>` refs into its own struct. After 5b1, the dispatcher holds ONE `Arc<AppState>` and queries the subsystems on demand. This matches the Pi-architecture pattern: dispatcher is session-scoped; AppState is process-scoped; one handle replaces eight refs.

**Tech Stack:** Rust 2021, no new crates.

**Related design:**
- AgentApi handle design: [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §6.2 (field reduction strategy).
- Pi-convergence gap audit: [`2026-05-27-pi-convergence-gap-audit.md`](../specs/2026-05-27-pi-convergence-gap-audit.md) §1.1 (god-object critique).
- 5a structural foundation: [`2026-05-29-stage3-p5a-dispatcher-structural-split.md`](2026-05-29-stage3-p5a-dispatcher-structural-split.md), merged at `0da7bada`.

---

## Recon-discovered facts vs. spec assumptions

| Spec assumption (§6.2) | Recon (verified against `0da7bada` main, 2026-05-29) | Plan adaptation |
|---|---|---|
| 71 fields → ~15 in one PR | Actual 53 fields. Triaged: 8 AppState-duplicates, 16 session-config bundles (5b2), 29 turn-scoped state (stays). | 5b1 targets the 8 duplicates → 53→46. 5b2 handles bundling → 46→34. ContentBlock dedup is 5b3. |
| "Replace per-subsystem fields with two handle references: `Arc<AgentApi>` + `Arc<AppState>`" | `AppState.agent_api: Arc<AgentApi>` exists (added by P3-1). So `Arc<AppState>` ALONE suffices — `api` is reachable as `app.agent_api`. No separate `api` field needed on ChatDelegate. | Add `app: Arc<AppState>` (one field). Drop the 8 redundants. Net -7 fields per PR. |
| Tests construct ChatDelegate with mocked subsystems | 9 test modules (50 dispatcher tests) construct via `ChatDelegate::new(...)` with explicit `safety_manager`, `pending_approvals`, `hook_bus` params today. | Tests need either (a) a `pub(crate) fn AppState::test_stub()` helper, or (b) a `ChatDelegate::new_for_test(...)` constructor that keeps the legacy signature. Plan picks (a) — single shared helper that other 5b PRs reuse. |

### The 8 fields collapsed in 5b1

| ChatDelegate field | Today | Reaches via | Field type | Optionality today |
|---|---|---|---|---|
| `safety_manager` | required `new()` param | `app.safety_manager` | `Arc<RwLock<SafetyManager>>` | Required |
| `pending_approvals` | required `new()` param | `app.pending_approvals` | `Arc<PendingApprovals>` | Required |
| `hook_bus` | required `new()` param | `app.agent_api.hook_bus()` (returns `Option<&Arc<HookBus>>` post-P3-3) | `Arc<HookBus>` | Required |
| `db` | set via `set_db()` | `app.db` | `Arc<Mutex<Connection>>` | `Option` |
| `learning_db` | set via `set_learning_pipeline()` | `app.db` (same conn) | `Arc<Mutex<Connection>>` | `Option` |
| `gbrain_extract_db` | set via `set_gbrain_extractor_pipeline()` | `app.db` (same conn) | `Arc<Mutex<Connection>>` | `Option` |
| `persist_db` | set via `with_agent_queues()` | `app.db` (same conn) | `Arc<Mutex<Connection>>` | `Option` |
| `gbrain_extract_mcp_mgr` | set via `set_gbrain_extractor_pipeline()` | `app.mcp_manager` | `SharedMcpManager` | `Option` |

The 4 DB fields all hold clones of the SAME `app.db` connection — verified at `app.rs:879` and `app.rs:967` where `mcp_manager` and `safety_manager` are constructed alongside `db` from the same Arc sources. After 5b1, `self.app.db.clone()` replaces all 4.

`hook_bus` access pattern post-5b1: most call sites are `self.hook_bus.dispatch(...)` — replaced with `self.app.agent_api.hook_bus().cloned().expect("hook_bus must be set by AppState::new").dispatch(...)`. Since P3-3 wired the hook bus into AgentApi at boot, the `expect` is documented as a hard invariant. (5b3 may revisit this for ergonomics.)

### Baselines to hold

- `cargo build`: 0 errors, **≤50 warnings** (post-5a baseline).
- `cargo test --lib agent::dispatcher`: **50 passed**.
- `cargo test --lib agent::`: **796 passed / 2 pre-existing failed**.
- `cargo test --lib` total: **3,050 passed / 7 pre-existing failed**.

### External callers (must compile unchanged or with trivially-replicable shape changes)

All 3 `ChatDelegate::new` sites live in `src-tauri/src/tauri_commands.rs` (lines ~1983, ~11222, ~15090). All 3 have `AppState` in scope as either `state: tauri::State<AppState>` or `app_state: &Arc<AppState>`. Each will replace ~6 explicit args with `app.clone()`.

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
    app: Arc<crate::app::AppState>,
) -> Self
```

11 params → 9. Dropped: `safety_manager`, `pending_approvals`, `hook_bus`. Added: `app`.

### After 5b1 — `set_db()`, `set_learning_pipeline()`, `set_gbrain_extractor_pipeline()`, `with_agent_queues()`

These setters drop the `db: Arc<Mutex<Connection>>` and `mcp_manager: SharedMcpManager` params; the dispatcher reads them from `self.app`. Setters may also delete if their other params merge into `app`-derived fields too — but **5b1 keeps the setters' other params intact** (they configure session-scoped things like `learning_llm`, `learning_llm_daily_budget`, `learning_buffer`). Only the redundant DB/mcp_mgr params drop.

### Field count after 5b1

- 53 (pre-5b1) − 8 (dropped) + 1 (`app` added) = **46 fields**.
- Remaining duplicates / bundlable concerns for 5b2: 16 session-config fields.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline:**

   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   ```
   Expected: `## main...origin/main` at `0da7bada`.

2. **Create worktree + symlinks:**

   ```bash
   git worktree add -b claude/stage3-p5b1-appstate-handle \
       /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/bunembed
   ```

3. **Capture baseline:**

   ```bash
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
   cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
   ```

---

## Task 1: Add `AppState::test_stub()` helper + `app: Arc<AppState>` field on ChatDelegate (DOES NOT YET DROP REDUNDANT FIELDS)

**Why this task exists:** Tests currently construct `ChatDelegate::new(...)` with explicit mocked `safety_manager` / `pending_approvals` / `hook_bus`. To later drop those from `new()`, tests need a cheap AppState stub. This task lands the stub + the `app` field first, in a single commit, with NO behavioral change. All 53 fields remain — just one is added.

**Files:**
- Modify: `src-tauri/src/app.rs` (add `pub(crate) fn AppState::test_stub() -> Arc<AppState>`)
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (add `app: Arc<AppState>` field + plumb through `new()`)
- Modify: `src-tauri/src/tauri_commands.rs` (3 call sites — pass `app.clone()` as new param)
- Modify: dispatcher test fixtures (each `ChatDelegate::new(...)` call now also passes `AppState::test_stub()`)

### Steps

- [ ] **Step 1.1: Locate the existing AppState construction pattern**

  ```bash
  grep -n "impl AppState\|pub async fn new\|pub fn new_for_test\|test_stub" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/app.rs | head -10
  ```
  Verify there's no existing test stub. If there is, skip Step 1.2 and reuse.

- [ ] **Step 1.2: Add `AppState::test_stub()` to app.rs**

  In the `impl AppState { ... }` block, add:

  ```rust
  /// Build a minimal `AppState` for unit tests.
  ///
  /// All fields are zeroed / default-constructed. The DB is an in-memory
  /// SQLite (`:memory:`). No real services, no Tauri runtime, no MCP
  /// servers — tests that need any of those should construct a more
  /// elaborate fixture.
  ///
  /// Used by P3-5b1+ to give `ChatDelegate` an always-present `Arc<AppState>`
  /// without requiring tests to spin up the full app.
  #[cfg(test)]
  pub(crate) fn test_stub() -> std::sync::Arc<Self> {
      // implementer fills this in by mirroring the minimal subset of
      // AppState fields that ChatDelegate touches. Use defaults / in-memory
      // SQLite / Arc::new(RwLock::new(Default::default())) liberally.
      // EVERY field of AppState must be populated. If a field has no Default,
      // construct the minimum acceptable value (e.g. an empty Vec, an empty
      // SkillsRegistry, a stub Notification Manager).
      //
      // Goal: make `ChatDelegate::new(.., app: AppState::test_stub())` compile
      // and let tests run against in-memory state. Real behavior is exercised
      // by integration tests outside `agent::dispatcher::*`.
      unimplemented!("flesh out per AppState's actual field set")
  }
  ```

  Then flesh out the body. AppState has ~30+ fields by 0da7bada. Use this strategy:
  - `data_dir`, `config_path`, `llm_config_path`, `db_path`, `workspace_root`: `PathBuf::from("/tmp/uclaw-test-stub")`.
  - `settings`, `llm_config`: `Arc::new(RwLock::new(Default::default()))`.
  - `db`: `Arc::new(std::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap()))`.
  - `db_ready`: `false`.
  - `session_manager`: `Arc::new(RwLock::new(SessionManager::new(...stub...)))` — check SessionManager's constructor for the minimum it needs.
  - `mcp_manager`: `Arc::new(RwLock::new(McpManager::new_for_test()))` if that exists, else minimum boot.
  - `safety_manager`: `Arc::new(tokio::sync::RwLock::new(SafetyManager::new(...stub policy...)))`.
  - `pending_approvals`: `Arc::new(PendingApprovals::new())`.
  - `agent_api`: `Arc::new(AgentApi::new())` (uses the new() from P3-1).
  - All other fields: default / `None` / empty.

  If a field has NO Default and NO obvious test constructor, the implementer reads its definition + finds the minimum acceptable value. Don't hard-code values that imply real behavior — tests should pass because the dispatcher doesn't read those fields, not because the stub has plausible defaults.

- [ ] **Step 1.3: Add `app: Arc<AppState>` field to ChatDelegate**

  In `src-tauri/src/agent/dispatcher/mod.rs`, find the `pub struct ChatDelegate { ... }` block. Add as the LAST field:

  ```rust
  /// Process-scope handle to the AppState. P3-5b1 introduced this to
  /// replace 8 fields that duplicated AppState subsystems:
  /// safety_manager, pending_approvals, hook_bus (via app.agent_api),
  /// and 4 DB clones + the mcp_manager. Subsequent tasks in 5b1 drop
  /// those fields one at a time; until then, both forms coexist.
  app: Arc<crate::app::AppState>,
  ```

- [ ] **Step 1.4: Wire `app` through `new()`**

  Append `app: Arc<AppState>` as the LAST parameter of `pub fn new(...)`. In the struct literal, add `app,`.

- [ ] **Step 1.5: Update the 3 external callers in `tauri_commands.rs`**

  ```bash
  grep -n "ChatDelegate::new" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/tauri_commands.rs
  ```

  At each site, find the `app_state` / `state` / `app` binding in scope (typically a `tauri::State<AppState>` or similar). Pass `app.inner().clone()` or `(*app).clone()` as the trailing argument. Each site is a 1-line addition — no other refactoring.

- [ ] **Step 1.6: Update dispatcher test fixtures**

  Use `grep -n "ChatDelegate::new" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/*.rs` to find every test call site. Add `crate::app::AppState::test_stub()` as the trailing argument.

  Estimated count: ~5-10 sites across the 9 test modules. Mechanical.

- [ ] **Step 1.7: Build + tests**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: 0 errors, ≤55 warnings (one new field = potentially one new `dead_code` warning until Tasks 2-7 wire it up), dispatcher 50/0, agent:: 796/2.

  If `dead_code` warnings appear for the new `app` field, that's fine — it'll be used by Tasks 2+.

- [ ] **Step 1.8: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/app.rs src-tauri/src/agent/dispatcher/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/agent/dispatcher/*.rs
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): inject app: Arc<AppState> into ChatDelegate + add AppState::test_stub (P3-5b1.1 of 阶段 3)"
  ```

Continue to Task 2.

---

## Task 2: Drop the 4 DB clones (db, learning_db, gbrain_extract_db, persist_db) → `self.app.db`

**Goal:** Remove the 4 fields holding `Arc<Mutex<rusqlite::Connection>>` clones. Replace all reads with `self.app.db.clone()` (or `&self.app.db` for `&Arc` callers).

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop 4 fields from struct + struct literal + drop `set_db`/`with_agent_queues` DB params/drop DB params from `set_learning_pipeline`/`set_gbrain_extractor_pipeline`)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs`, `model_io.rs`, `content_assembler.rs`, `observability.rs` (every read of the 4 fields → `self.app.db`)
- Modify: `src-tauri/src/tauri_commands.rs` (drop the DB args from the 3 callers' setter chains)

### Steps

- [ ] **Step 2.1: Audit reads of each field**

  ```bash
  grep -rnE "self\.(db|learning_db|gbrain_extract_db|persist_db)\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Inventory every site. Most will be `self.field.as_ref().map(|db| ...)`. After collapse: `let db = self.app.db.clone();` (the `app.db` is always-present, NOT Option).

  **Subtle semantic change:** the dropped fields were `Option`; the new path is always-present. If a code path branched on the Option (e.g., `if let Some(db) = &self.db { ... }`), it now always takes the Some branch. Verify each site that this is desired — for the 4 DB fields, it IS desired (we never want "no DB available" in production; the prior None was just an artifact of optional setters).

- [ ] **Step 2.2: Drop the 4 fields from the struct + struct literal in `new()`**

  In `src-tauri/src/agent/dispatcher/mod.rs`:
  - Remove `db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>` field + the `db: None,` initializer.
  - Remove `learning_db`, `gbrain_extract_db`, `persist_db` likewise.

- [ ] **Step 2.3: Update setters**

  - `pub fn set_db(&mut self, db: ...)` — DELETE the entire method.
  - `pub fn with_agent_queues(mut self, queues: AgentQueues, db: Arc<Mutex<Connection>>) -> Self` — change signature to drop the `db` param. Body no longer assigns `self.persist_db`.
  - `pub fn set_learning_pipeline(...)` — drop the `learning_db` param if present.
  - `pub fn set_gbrain_extractor_pipeline(...)` — drop the `gbrain_extract_db` param if present.

  Each tauri_commands.rs caller of these setters now drops the corresponding arg.

- [ ] **Step 2.4: Replace reads with `self.app.db`**

  For each call site found in Step 2.1, replace `self.<field>.clone()` with `self.app.db.clone()`. For `if let Some(db) = self.<field>.as_ref()` patterns, unwrap: `let db = &self.app.db;`.

- [ ] **Step 2.5: Build + tests**

  Same battery as Step 1.7. Expected: same pass count.

  Likely failure mode: a call site referenced a field by `as_ref()` but the surrounding code (e.g., spawning a background task) expected `Option<Arc<...>>`. Easy fix — wrap in `Some(self.app.db.clone())`.

- [ ] **Step 2.6: Commit**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop 4 redundant DB fields → self.app.db (P3-5b1.2 of 阶段 3)"
  ```

Continue to Task 3.

---

## Task 3: Drop `gbrain_extract_mcp_mgr` → `self.app.mcp_manager`

**Goal:** Remove the single field holding a clone of `SharedMcpManager`. Replace reads with `self.app.mcp_manager.clone()`.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop field + struct literal entry + drop param from `set_gbrain_extractor_pipeline`)
- Modify: `src-tauri/src/agent/dispatcher/turn_runner.rs` (or wherever `gbrain_extract_mcp_mgr` is read)
- Modify: `src-tauri/src/tauri_commands.rs` (drop mcp_manager arg from setter caller)

### Steps

- [ ] **Step 3.1: Audit + replace**

  ```bash
  grep -rnE "self\.gbrain_extract_mcp_mgr\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Replace each site with `self.app.mcp_manager.clone()`. Drop the field + struct literal entry + setter param.

- [ ] **Step 3.2: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```
  Expected: same baselines.

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop gbrain_extract_mcp_mgr → self.app.mcp_manager (P3-5b1.3 of 阶段 3)"
  ```

Continue to Task 4.

---

## Task 4: Drop `safety_manager` → `self.app.safety_manager`

**Goal:** Remove the required `safety_manager` field + the corresponding `new()` param.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher/mod.rs` (drop field + struct literal + `new()` param)
- Modify: `src-tauri/src/agent/dispatcher/safety_gate.rs` (the one caller — `resolve_effective_mode`)
- Modify: `src-tauri/src/tauri_commands.rs` (drop `safety_manager` arg from 3 callers)
- Modify: test fixtures in `dispatcher/*.rs` (drop the corresponding arg from each `ChatDelegate::new(...)` call)

### Steps

- [ ] **Step 4.1: Audit reads of `self.safety_manager`**

  ```bash
  grep -rnE "self\.safety_manager\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/
  ```

  Most are in `safety_gate.rs::resolve_effective_mode`. Replace with `self.app.safety_manager`.

- [ ] **Step 4.2: Drop field + new() param + struct literal entry**

- [ ] **Step 4.3: Update callers**

  - 3 tauri_commands.rs call sites: drop the `safety_manager` arg.
  - Every test fixture's `ChatDelegate::new(...)` call: drop the `safety_manager` arg.

- [ ] **Step 4.4: Build + tests + commit**

  ```bash
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -3
  cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
  ```

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop safety_manager field → self.app.safety_manager (P3-5b1.4 of 阶段 3)"
  ```

Continue to Task 5.

---

## Task 5: Drop `pending_approvals` → `self.app.pending_approvals`

Same shape as Task 4, for `pending_approvals` instead of `safety_manager`. Audit, drop field, drop `new()` param, drop from 3 tauri_commands sites + test fixtures, build, commit.

Commit message:
```
refactor(agent): drop pending_approvals field → self.app.pending_approvals (P3-5b1.5 of 阶段 3)
```

---

## Task 6: Drop `hook_bus` → `self.app.agent_api.hook_bus()`

**Goal:** Remove the last redundant field. The `app.agent_api: Arc<AgentApi>` exposes `hook_bus()` → `Option<&Arc<HookBus>>`. P3-3 wires the HookBus into AgentApi at AppState construction, so the `Option` is `Some` in production.

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

  Each `self.hook_bus.dispatch(ev)` becomes:
  ```rust
  if let Some(bus) = self.app.agent_api.hook_bus() {
      bus.dispatch(ev).await
  }
  ```
  OR with an `expect()` if the callsite has no graceful fallback:
  ```rust
  self.app.agent_api.hook_bus()
      .expect("hook_bus must be wired into AgentApi at AppState boot")
      .dispatch(ev).await
  ```

  Pick `expect()` for "should never fire in production" sites (most hook_bus reads), `if let` for sites that already had a None-tolerant branch (e.g., test contexts).

- [ ] **Step 6.2: Drop field + new() param**

- [ ] **Step 6.3: Build + tests + commit**

  Expected: same baselines.

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle commit -m "refactor(agent): drop hook_bus field → self.app.agent_api.hook_bus() (P3-5b1.6 of 阶段 3)"
  ```

Continue to Task 7.

---

## Task 7: Final shape audit + cumulative tests

**Goal:** Confirm ChatDelegate is now 46 fields (53 − 8 + 1 = 46), `new()` is 9 params, all 3 tauri_commands.rs callers compile cleanly, dispatcher tests still pass.

### Steps

- [ ] **Step 7.1: Field count audit**

  ```bash
  awk '/^pub struct ChatDelegate/,/^}/' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/mod.rs | grep -E "^    [a-z_]+:|^    [a-z_]+ *$" | wc -l
  ```
  Expected: 46.

- [ ] **Step 7.2: new() signature audit**

  ```bash
  sed -n '/^    pub fn new(/,/-> Self/p' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle/src-tauri/src/agent/dispatcher/mod.rs
  ```
  Expected: 9 params, with `app: Arc<crate::app::AppState>` as the last.

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
  - ≤55 warnings (target ≤50 — same as 5a baseline).
  - dispatcher 50/0, agent:: 796/2, cargo test --lib 3050/7.

- [ ] **Step 7.4: Clean unused imports (if warnings crept)**

  Same pattern as 5a Task 7.4 — `cargo build 2>&1 | grep -B 1 "dispatcher" warnings`, remove unused `use` lines.

- [ ] **Step 7.5: Commit cleanup (if any)**

  ```bash
  git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p5b1-appstate-handle add -A src-tauri/src/agent/dispatcher/
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
- ✅ Field reduction: 53 → 46 (5b1's share toward spec's 53→30 target).
- ✅ AppState handle injection — the "two handle references" pattern from §6.2.
- 🟡 AgentApi handle injection: NOT a separate field. Reached via `self.app.agent_api`. This is intentional — adding a separate `api` field would be redundant since AppState already holds it.
- 🟡 No ContentBlock dedup. Out of scope. P3-5b3.
- 🟡 No pipeline bundling (learning_*, gbrain_extract_*, gene_*, telemetry collectors stay). Out of scope. P3-5b2.

**2. Placeholder scan:**
- Task 1 Step 1.2 has an `unimplemented!()` placeholder — but it's in the SCAFFOLD that the implementer is expected to flesh out IN THAT STEP. The plan does not ship `unimplemented!()` into a commit.
- No "TBD" / "TODO" / "implement later" in shipped commits.

**3. Type consistency:**
- `Arc<crate::app::AppState>` named consistently throughout (Task 1, struct, new() param, all callers).
- All 8 dropped fields' replacement paths spelled out: `self.app.db`, `self.app.mcp_manager`, `self.app.safety_manager`, `self.app.pending_approvals`, `self.app.agent_api.hook_bus()`.

**4. Bisectability:**
- One field-cohort per commit. Each commit's `cargo test --lib agent::` must pass; that's the gate.
- Test fixture updates ride with the corresponding field drop, NOT bundled at the end — otherwise intermediate commits would break tests.

---

## Cumulative summary

- **Tasks:** 7 (6 mandatory + 1 optional cleanup).
- **Estimated time:** 1.5-2 person-days.
- **Risk:** Medium. The `AppState::test_stub()` helper is the trickiest task (~30+ AppState fields to populate sensibly); after that, Tasks 2-6 are mechanical drops.
- **Total commits:** 6-7.

After 5b1 ships: `ChatDelegate` has 46 fields (down 7 from 53). The struct now holds `app: Arc<AppState>` as the universal subsystem-handle. 5b2 can drop another ~12 by bundling session config; 5b3 collapses the 5 ContentBlock dup sites.
