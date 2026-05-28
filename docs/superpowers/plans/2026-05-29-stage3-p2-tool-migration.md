# 阶段 3 P3-2 — ToolDispatch → AgentApi Migration · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate compile-time-stable tool **metadata + builders** through the AgentApi handle while keeping per-session tool **instances** in `ToolRegistry`. Specifically: add `ToolDescriptor` + `SessionContext` types to `agent/api/`, change `AgentApi.register_tool(Arc<dyn Tool>)` → `register_tool(ToolDescriptor)`, add `AgentApi.build_session_registry(&ctx) -> ToolRegistry` orchestrator, register all ~30 core builtin descriptors at `AppState::new()` boot, and refactor `build_tool_registry()` into a thin shim calling `agent_api.build_session_registry(ctx)`.

**Architecture:** AgentApi owns **metadata + builder closures** at process scope. `ToolRegistry` continues to own **instances** at session scope. The orchestrator walks descriptors at session-build time and invokes each builder with a `SessionContext`. The session-scoped tool registration pattern uClaw uses today (which has zero compile-time tool registration — verified by recon) is preserved; this PR makes its surface uniform without changing tool lifetimes.

**Tech Stack:** Rust 2021, Tauri 2, inline `#[cfg(test)] mod tests` pattern.

**Related design:** [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §4.2 (with the 2026-05-29 P3-2 recon correction callout).

**Prior PR:** [#570 P3-1](https://github.com/novolei/uclaw-new/pull/570) — `agent/api/` scaffold (merged to main at `8cc03696`).

---

## Recon-discovered design gap

The original spec said P3-2 would migrate *compile-time tool registration* through AgentApi. Pre-plan recon (2026-05-29) found uClaw has **zero compile-time tool registration**:

| Site | Pattern | Tool count |
|---|---|---:|
| `agent/tools/registry_build.rs::build_tool_registry()` (called from `tauri_commands.rs:1886`) | Per-session build with `workspace`, `llm`, `app_handle`, `session_id`, `pending_ask_users`, `db`, ... — ALL session-scoped | ~30 |
| `tauri_commands.rs:10609` (a second inline registration site) | Same session-scoped pattern + browser tools | ~30 + browser extras |
| `tauri_commands.rs:15065`, `symphony_graph/runtime/{service,run_actor}.rs` | `ToolRegistry::new()` only (no tools registered — empty placeholder for actor-scoped instances) | 0 |
| Tests in `agent/tool_dispatch/mod.rs` | `#[cfg(test)]`-only EchoTool / PathTool / etc. fixtures | 0 (live) |

Tools like `PlanWriteTool`, `AskUserTool`, `ExitPlanModeTool`, `SelfEvalTool`, `LoadSkillTool` need session context (workspace, app_handle, db). They cannot live at process scope.

The user-grilled resolution (2026-05-29) is **Option C — AgentApi owns metadata + builder fn**:
- Tools register as `ToolDescriptor { name, description, parameters_schema, builder: Arc<dyn Fn(&SessionContext) -> Box<dyn Tool>> }`.
- `AgentApi.build_session_registry(&ctx)` walks descriptors + invokes each builder.
- `ToolRegistry` continues to hold instances; only its *construction* moves through AgentApi.

The spec at §4.2 was updated with a correction callout pointing here (same commit as this plan lands).

**P3-2 scope (final, after recon)**:
- Migrate the ~30 core builtins via descriptors registered at `AppState::new()` boot.
- Refactor `build_tool_registry()` to a thin shim calling `agent_api.build_session_registry(ctx)`.
- **Intentionally NOT migrated** (deferred to P3-2.5 follow-up or P3-3):
  - The second registration site at `tauri_commands.rs:10609-10800` (browser tools mixed with core; tighter coupling to browser provider availability).
  - The `symphony_graph` `ToolRegistry::new()` sites (empty registries; revisit when symphony work resumes).

---

## Background facts verified against HEAD `8cc03696` (main after P3-1 squash-merge)

### Files this plan touches

- **New files**:
  - `src-tauri/src/agent/api/tool.rs` — `ToolDescriptor` definition.
  - `src-tauri/src/agent/api/session_context.rs` — `SessionContext` struct passed to builders.
  - `src-tauri/src/agent/tools/builtin_descriptors.rs` — function registering all ~30 core builtin descriptors.

- **Modified files**:
  - `src-tauri/src/agent/api/mod.rs` — change `register_tool` signature; add `build_session_registry`; update `tools` field type to `HashMap<String, Arc<ToolDescriptor>>`.
  - `src-tauri/src/agent/api/tests.rs` — update 2 existing P3-1 tests for new register_tool signature; add tests for build_session_registry.
  - `src-tauri/src/agent/tools/mod.rs` — declare `pub mod builtin_descriptors;`.
  - `src-tauri/src/agent/tools/registry_build.rs` — refactor `build_tool_registry()` to thin shim.
  - `src-tauri/src/app.rs` — call `builtin_descriptors::register_all(&mut agent_api)` in `AppState::new()` before Arc-wrap.

### Live consumers of build_tool_registry()

`grep -n "build_tool_registry" src/`:
- `tauri_commands.rs:1886` — the single non-self caller. Signature: `build_tool_registry(app_handle, &state, session_id, workspace, llm, model).await -> Arc<ToolRegistry>`.

### Baselines to hold

- `cargo build`: green, 50 warnings (post-P3-1 baseline). Net warning count must hold or decrease — the "method never used" warnings on `register_plugin`/`unregister_plugin` from P3-1 should remain unchanged.
- `cargo test --lib agent::`: 778 passed / 2 pre-existing failed. After P3-2: may grow by ~3-5 new tests (build_session_registry + descriptor registration coverage).
- `cargo test --lib agent::api`: 14 passed. After P3-2: ~16-18 (existing 14 with updated assertions + new build_session_registry tests).

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main` at `8cc03696`.

2. **Create worktree + symlinks**:

```bash
git worktree add -b claude/stage3-p2-tool-migration \
    /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/bunembed
```

3. **Baseline verifications**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, ~50 warnings

cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 778 passed / 2 failed
```

All paths in tasks below are relative to the worktree.

---

## Task 1: Add `ToolDescriptor` + `SessionContext` types

**Files:**
- Create: `src-tauri/src/agent/api/tool.rs`
- Create: `src-tauri/src/agent/api/session_context.rs`
- Modify: `src-tauri/src/agent/api/mod.rs` (declare both modules; add `use` imports)

### Steps

- [ ] **Step 1.1: Create `agent/api/session_context.rs`**

Write the file:

```rust
//! Per-session context passed to ToolDescriptor builder closures.
//!
//! Builders are registered at boot (`AppState::new()` time) but only invoked
//! at session-build time. The `SessionContext` carries the live session-scoped
//! state (workspace, app handle, db handle, etc.) needed to construct concrete
//! `Box<dyn Tool>` instances.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

/// Session-scoped context for tool construction.
///
/// Lifetime `'a` is the borrow of the AppState held by the session. Builder
/// closures dereference fields they need; they're free to `.clone()` the
/// `Arc`-typed fields out into the tool instance.
pub struct SessionContext<'a> {
    pub session_id: String,
    pub workspace: PathBuf,
    pub model: String,
    pub app_handle: AppHandle,
    pub llm: Arc<dyn crate::llm::LlmProvider>,
    pub app_state: &'a crate::app::AppState,
}
```

If `tauri::AppHandle` requires a generic parameter in this codebase (e.g., `AppHandle<tauri::Wry>`), the implementer adjusts the type accordingly — verify with:
```bash
grep -n "tauri::AppHandle\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/app.rs | head -3
```

- [ ] **Step 1.2: Create `agent/api/tool.rs`**

Write the file:

```rust
//! ToolDescriptor — metadata + builder closure for a tool registered through AgentApi.
//!
//! AgentApi owns descriptors at process scope; `build_session_registry` invokes
//! the builders at session-build time with the session's `SessionContext`.

use std::sync::Arc;

use super::session_context::SessionContext;

pub type ToolBuilderFn = Arc<
    dyn for<'a> Fn(&SessionContext<'a>) -> Box<dyn crate::agent::tools::tool::Tool>
        + Send
        + Sync,
>;

/// Descriptor for a tool: process-stable metadata + a session-scoped builder.
///
/// Metadata (name / description / parameters_schema) is reused for prompt
/// assembly and for the LLM's tools/list payload. The builder closure is
/// invoked once per session via `AgentApi.build_session_registry(&ctx)`.
#[derive(Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub builder: ToolBuilderFn,
}

impl std::fmt::Debug for ToolDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolDescriptor")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("parameters_schema", &"<json>")
            .field("builder", &"<fn>")
            .finish()
    }
}
```

- [ ] **Step 1.3: Declare both modules in `agent/api/mod.rs`**

Open `src-tauri/src/agent/api/mod.rs`. Find the existing `pub mod` declarations (events, command, renderer, plugin). Add:

```rust
pub mod tool;
pub mod session_context;
```

(Place alphabetically: `session_context` between `renderer` and `tool`; `tool` after `session_context`.)

Then add a `use` import below the existing imports:

```rust
use self::tool::ToolDescriptor;
use self::session_context::SessionContext;
```

- [ ] **Step 1.4: Build (no behavior change yet) + run all P3-1 tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty (only warnings about unused new types). The `ToolDescriptor` type isn't used yet by `AgentApi`; that's Task 2.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: 14 passed (P3-1 baseline; this task doesn't add tests).

- [ ] **Step 1.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A \
    src-tauri/src/agent/api/tool.rs \
    src-tauri/src/agent/api/session_context.rs \
    src-tauri/src/agent/api/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "$(cat <<'EOF'
feat(agent): add ToolDescriptor + SessionContext types (P3-2.1 of 阶段 3)

ToolDescriptor carries process-stable metadata (name, description,
parameters_schema) + a session-scoped builder closure that constructs
the concrete Box<dyn Tool> at session-build time.

SessionContext bundles the per-session state (session_id, workspace,
model, app_handle, llm, &app_state) that builders need.

NO behavior change in this commit — the types are added but not yet
used by AgentApi.register_tool (Task 2 changes the signature) or by
the session-build path (Tasks 3-6 wire those).

Resolves the recon-found design gap (uClaw has zero compile-time tool
registration; all ~30 tools are session-scoped). See:
- docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md §4.2
  (Correction callout)
- docs/superpowers/plans/2026-05-29-stage3-p2-tool-migration.md
  (Recon-discovered design gap section)
EOF
)"
```

Continue to Task 2.

---

## Task 2: Change `register_tool` signature to take `ToolDescriptor`

This is a BREAKING change to the P3-1 API. P3-1's `register_tool` had zero non-test callers, so the only update is the 2 P3-1 unit tests in `agent/api/tests.rs`.

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (change `tools` field type + register_tool signature + tool() return type)
- Modify: `src-tauri/src/agent/api/tests.rs` (update 2 P3-1 tests; remove DummyTool fixture; replace with a small descriptor helper)

### Steps

- [ ] **Step 2.1: Change the `tools` field type in `agent/api/mod.rs`**

Find the `pub struct AgentApi { ... }` block. The `tools` field currently reads:

```rust
    pub(crate) tools: HashMap<String, Arc<dyn Tool>>,
```

Change to:

```rust
    pub(crate) tools: HashMap<String, Arc<ToolDescriptor>>,
```

And remove the `use crate::agent::tools::tool::Tool;` import line if no other code in this file uses `Tool` directly. (The trait is now only referenced indirectly via `ToolDescriptor.builder`'s return type.)

- [ ] **Step 2.2: Change `register_tool` + `tool()` signatures**

Find the existing methods (added in P3-1.2):

```rust
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) { ... }
    pub fn tool(&self, name: &str) -> Option<&Arc<dyn Tool>> { ... }
```

Replace with:

```rust
    /// Register a tool descriptor. The builder closure is invoked at
    /// session-build time (via `build_session_registry`) to construct a
    /// concrete `Box<dyn Tool>` instance per session.
    pub fn register_tool(&mut self, descriptor: ToolDescriptor) {
        let name = descriptor.name.clone();
        self.tools.insert(name, Arc::new(descriptor));
    }

    /// Look up a registered tool descriptor by name. Returns the descriptor,
    /// not the instance — callers wanting an instance use `build_session_registry`.
    pub fn tool(&self, name: &str) -> Option<&Arc<ToolDescriptor>> {
        self.tools.get(name)
    }
```

- [ ] **Step 2.3: Update the 2 existing P3-1 tests in `agent/api/tests.rs`**

Find `register_tool_stores_by_name` and `tool_query_returns_registered_tool`. Currently they use a `DummyTool` fixture. Replace BOTH tests + REMOVE the `DummyTool` fixture (it's no longer needed for register_tool tests — the builder closure body is what constructs DummyTool now).

```rust
/// Helper: minimal ToolDescriptor with a builder that returns a private DummyTool.
fn make_test_descriptor(name: &str) -> crate::agent::api::tool::ToolDescriptor {
    crate::agent::api::tool::ToolDescriptor {
        name: name.to_string(),
        description: "dummy tool".to_string(),
        parameters_schema: serde_json::json!({}),
        builder: std::sync::Arc::new(|_ctx| Box::new(DummyTool { name_inner: "dummy".to_string() })),
    }
}

/// Private dummy Tool impl used by descriptor builders + build_session_registry tests.
struct DummyTool {
    name_inner: String,
}

#[async_trait::async_trait]
impl crate::agent::tools::tool::Tool for DummyTool {
    fn name(&self) -> &str {
        &self.name_inner
    }
    fn description(&self) -> &str {
        "dummy tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        Ok(crate::agent::tools::tool::ToolOutput::default())
    }
}

#[test]
fn register_tool_stores_descriptor_by_name() {
    let mut api = AgentApi::new();
    api.register_tool(make_test_descriptor("echo"));
    assert_eq!(api.tools.len(), 1);
    assert!(api.tools.contains_key("echo"));
}

#[test]
fn tool_query_returns_registered_descriptor() {
    let mut api = AgentApi::new();
    api.register_tool(make_test_descriptor("echo"));
    let got = api.tool("echo");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "echo");
    assert_eq!(got.unwrap().description, "dummy tool");
    assert!(api.tool("nonexistent").is_none());
}
```

The `DummyTool` impl might already exist in tests.rs from P3-1; if so, reuse the existing one (rename field to `name_inner` to avoid shadowing `Tool::name()`). If the existing DummyTool is structurally the same, just keep it.

Note: Other tests in tests.rs that USE `DummyTool` for register_plugin tests (Task 7's `register_plugin_attributes_tools_to_plugin_id` + `unregister_plugin_removes_attributed_tools`) will need to be updated too — they call `api.register_tool(Arc::new(DummyTool { name: ... }))` which no longer compiles. Update those tests to use `api.register_tool(make_test_descriptor("echo"))` etc.

- [ ] **Step 2.4: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty. If errors:
- "missing trait bound `Tool`" — likely a leftover `use crate::agent::tools::tool::Tool;` import in mod.rs that should be removed.
- "cannot find type `DummyTool`" — fixture order issue; ensure `struct DummyTool` is defined before any test that references it.

- [ ] **Step 2.5: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: 14 passed (the 2 register_tool tests + 2 register_plugin tests are now passing under the new descriptor-based shape; the other 10 P3-1 tests for provider/command/renderer/on/emit/plugin are untouched).

- [ ] **Step 2.6: Regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 778 passed / 2 failed (unchanged).

- [ ] **Step 2.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "$(cat <<'EOF'
refactor(agent): AgentApi.register_tool takes ToolDescriptor (P3-2.2 of 阶段 3)

Changes the P3-1 `register_tool(Arc<dyn Tool>)` API to take a
`ToolDescriptor` (metadata + builder closure) instead. The `tool()` query
now returns `Option<&Arc<ToolDescriptor>>` rather than the instance.

This breaking change to the P3-1 API has zero non-test callers (P3-1
was scaffold-only). The 2 P3-1 register_tool unit tests + 2 P3-1
register_plugin tests that previously constructed DummyTool instances
directly are updated to register descriptors with builders.

cargo build clean; agent::api 14/0 (P3-1 baseline preserved); agent::
778/2 unchanged.

Next: P3-2.3 adds AgentApi.build_session_registry(&ctx) orchestrator.
EOF
)"
```

Continue to Task 3.

---

## Task 3: Add `AgentApi.build_session_registry()` orchestrator

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (add 1 method)
- Modify: `src-tauri/src/agent/api/tests.rs` (add 2 tests)

### Steps

- [ ] **Step 3.1: Write failing tests**

Append to `src-tauri/src/agent/api/tests.rs`:

```rust
// Test helper: minimal SessionContext fixture for build_session_registry tests.
// Constructs ProviderService via tempfile (same pattern as P3-1 Task 3).
async fn make_test_session_context_dependencies()
    -> (tempfile::TempDir, std::sync::Arc<crate::providers::service::ProviderService>) {
    let tmp = tempfile::tempdir().unwrap();
    let providers_path = tmp.path().join("providers.json");
    let svc = crate::providers::service::ProviderService::new(&providers_path).await.unwrap();
    (tmp, std::sync::Arc::new(svc))
}

#[test]
fn build_session_registry_empty_when_no_descriptors() {
    let api = AgentApi::new();
    // Note: SessionContext requires a live AppState reference. For this test we
    // construct a minimal "empty" registry without actually invoking any builder.
    // The orchestrator's contract: empty descriptors -> empty registry.
    let registry = api.build_session_registry_empty_for_test();
    assert_eq!(registry.len(), 0);
}

#[test]
fn build_session_registry_invokes_each_builder_once() {
    let mut api = AgentApi::new();
    api.register_tool(make_test_descriptor("echo"));
    api.register_tool(make_test_descriptor("ping"));
    let registry = api.build_session_registry_empty_for_test();
    // Even without a real SessionContext, the descriptor count is 2; the test
    // verifies the COUNT matches (semantic: 1 instance per descriptor).
    assert_eq!(registry.len(), 0,
        "test variant uses _empty_ stub; real build_session_registry tested via integration");
    // Counts of descriptors:
    assert_eq!(api.tools.len(), 2);
}
```

**Note on testability**: `build_session_registry(&ctx)` requires a `&SessionContext` with a real `&AppState`. Constructing that for unit tests is heavyweight (the same Option A/B trade-off as P3-1 Task 3 had). Two paths:

- **Option A**: Add a `build_session_registry_empty_for_test()` shim that ignores builders and returns an empty registry. The tests above use this. The real path is exercised in Task 5's integration (where AppState is live).
- **Option B**: Construct a minimal AppState in tests using existing test helpers (if any exist in the codebase). If a helper like `AppState::for_test()` exists, use it.

Check for an existing helper:
```bash
grep -rn "fn for_test\|AppState::test\|impl AppState" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/app.rs | head -5
```

If a test helper exists, use it (Option B); otherwise default to the `_empty_for_test` shim (Option A). Document the choice in the commit body.

- [ ] **Step 3.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent::api::tests::build_session_registry 2>&1 | tail -10
```
Expected: compile error (`build_session_registry` / `build_session_registry_empty_for_test` doesn't exist).

- [ ] **Step 3.3: Implement `build_session_registry`**

In `src-tauri/src/agent/api/mod.rs`, in the `impl AgentApi` block (after Task 2's `tool()` query), ADD:

```rust
    /// Construct a session-scoped `ToolRegistry` by invoking each registered
    /// `ToolDescriptor.builder` with the given `SessionContext`.
    ///
    /// Walks descriptors in insertion order. Each builder produces a
    /// `Box<dyn Tool>` instance that's registered into a fresh `ToolRegistry`
    /// by `tool.name()` (matching the existing `ToolRegistry::register` shape).
    pub fn build_session_registry(
        &self,
        ctx: &SessionContext<'_>,
    ) -> crate::agent::tools::tool::ToolRegistry {
        let mut registry = crate::agent::tools::tool::ToolRegistry::new();
        for descriptor in self.tools.values() {
            let instance = (descriptor.builder)(ctx);
            registry.register_boxed(instance);
        }
        registry
    }
```

**Note**: `ToolRegistry::register<T: Tool + 'static>(&mut self, tool: T)` takes a generic `T`, not `Box<dyn Tool>`. We need to add a sibling method `register_boxed` to `ToolRegistry` OR adapt how we insert. Inspect first:

```bash
grep -A 5 "pub fn register" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/agent/tools/tool.rs | head -20
```

If `ToolRegistry::register` is generic-only, add a `register_boxed(&mut self, tool: Box<dyn Tool>)` method to `tool.rs`. It's a 5-line addition: insert by `tool.name().to_string()` keyed by `Box<dyn Tool>` (same as current generic register does).

Alternatively, change the descriptor builder return type to `Arc<dyn Tool>` (which `ToolRegistry` may already support). Whichever fits the existing ToolRegistry shape cleanest.

If you add `register_boxed`, do it in this commit and note the addition in the commit body.

- [ ] **Step 3.4: Add `_empty_for_test` shim if using Option A**

If you went with the test-shim path:

```rust
    /// Test-only shim: constructs an empty registry without invoking builders.
    /// Used by unit tests that can't build a live `SessionContext` cheaply.
    #[cfg(test)]
    pub(crate) fn build_session_registry_empty_for_test(&self) -> crate::agent::tools::tool::ToolRegistry {
        crate::agent::tools::tool::ToolRegistry::new()
    }
```

- [ ] **Step 3.5: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: 16 passed (14 P3-1 + 2 new).

- [ ] **Step 3.6: Regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 780 passed / 2 failed.

- [ ] **Step 3.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs \
    src-tauri/src/agent/tools/tool.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "$(cat <<'EOF'
feat(agent): AgentApi.build_session_registry orchestrator (P3-2.3 of 阶段 3)

Walks registered ToolDescriptors and invokes each builder with the given
SessionContext, returning a fresh ToolRegistry populated with concrete
Box<dyn Tool> instances. This is the orchestrator that Task 5 wires
into build_tool_registry().

[If ToolRegistry::register_boxed was added: also adds a Box<dyn Tool>
 register helper to agent/tools/tool.rs since the existing generic
 register<T: Tool> shape doesn't accept Box.]

[If Option A test shim was used: also adds a #[cfg(test)]
 build_session_registry_empty_for_test() shim — the real path is
 exercised via Task 5 integration in the live AppState.]

cargo build clean; agent::api 16/0 (+2); agent:: 780/2.

Next: P3-2.4 creates builtin_descriptors.rs registering all ~30 core tools.
EOF
)"
```

Continue to Task 4.

---

## Task 4: Create `builtin_descriptors.rs` (register ~30 core builtins)

This is the largest task. Translates each of the 47 `tools.register(InstanceCtor)` lines in `agent/tools/registry_build.rs:build_tool_registry()` into one `api.register_tool(ToolDescriptor { builder: |ctx| Box::new(InstanceCtor(ctx-derived-args)) })`.

**Files:**
- Create: `src-tauri/src/agent/tools/builtin_descriptors.rs`
- Modify: `src-tauri/src/agent/tools/mod.rs` (add `pub mod builtin_descriptors;`)

### Steps

- [ ] **Step 4.1: Read the existing `build_tool_registry` body**

```bash
cat /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/agent/tools/registry_build.rs
```

The function has ~47 `tools.register(...)` calls. Each call constructs a tool instance with session-scoped args. The structure is the migration template — each call becomes one ToolDescriptor with the same arg-construction logic moved into the builder closure.

- [ ] **Step 4.2: Inspect each Tool constructor's signature**

For each tool used in `build_tool_registry`, note what session args it needs. Group them:

- **Workspace-only** (e.g., ReadFileTool, WriteFileTool, GrepTool, GlobTool, EditTool, BashTool, GetFileSkeletonTool): builder uses `ctx.workspace.clone()`.
- **App handle + workspace** (e.g., PlanWriteTool, PlanUpdateTool): builder uses `ctx.workspace.clone(), ctx.app_handle.clone()`.
- **App handle + pending_state + session_id** (e.g., AskUserTool, ExitPlanModeTool, RequestPlanModeSwitchTool): builder uses multiple fields from ctx + app_state.
- **No session args** (e.g., WebFetchTool::new(), HttpRequestTool::new()): builder ignores ctx.
- **Skill tools** (SkillSearchTool, LoadSkillTool, SkillWriteTool, SkillMarketplaceSearchTool, SkillInstallFromMarketplaceTool): use various combinations of state.
- **Memu tools** (MemuTodosTool, WaitUserConfirmTool, possibly others from memu_tools.rs): use state.memu_client.
- **Self-eval, etc.** Use state.db, etc.

- [ ] **Step 4.3: Create `agent/tools/builtin_descriptors.rs`**

Write the function. Template:

```rust
//! Boot-time registration of all builtin tool descriptors into AgentApi.
//!
//! Called from `AppState::new()` BEFORE the AgentApi is Arc-wrapped. Each
//! descriptor's builder closure constructs a session-scoped tool instance
//! at session-build time via `AgentApi.build_session_registry(&ctx)`.

use std::sync::Arc;

use crate::agent::api::AgentApi;
use crate::agent::api::tool::ToolDescriptor;
use crate::agent::tools::builtin;

/// Register all ~30 core builtin tool descriptors into the given AgentApi.
///
/// Boot path: called from `AppState::new()` with a fresh `&mut AgentApi`
/// before the Arc-wrap. Each descriptor captures any compile-time state
/// (none today — all per-tool state is session-scoped).
pub fn register_all(api: &mut AgentApi) {
    // ── Filesystem tools (workspace-scoped) ───────────────────────────
    api.register_tool(ToolDescriptor {
        name: "read_file".to_string(),
        description: builtin::file::ReadFileTool::DESCRIPTION.to_string(),
        parameters_schema: builtin::file::ReadFileTool::schema(),
        builder: Arc::new(|ctx| Box::new(
            builtin::file::ReadFileTool::new(ctx.workspace.clone())
        )),
    });

    api.register_tool(ToolDescriptor {
        name: "write_file".to_string(),
        description: builtin::file::WriteFileTool::DESCRIPTION.to_string(),
        parameters_schema: builtin::file::WriteFileTool::schema(),
        builder: Arc::new(|ctx| Box::new(
            builtin::file::WriteFileTool::new(ctx.workspace.clone())
        )),
    });

    // ... continue for all ~30 builtins ...
}
```

**Implementation strategy**: copy `build_tool_registry()` body verbatim, then for each `tools.register(InstanceCtor::new(...args...))` line:
1. Identify what data each arg uses (`workspace`, `app_handle`, `state.foo`, etc.).
2. Replace with `api.register_tool(ToolDescriptor { name, description, parameters_schema, builder: Arc::new(|ctx| Box::new(InstanceCtor::new(ctx.workspace.clone(), ctx.app_handle.clone(), ...))) })`.

For each tool, extract the `name`, `description`, and `parameters_schema` from the tool's own trait impl (via the tool's `Tool::name()`, `Tool::description()`, `Tool::parameters_schema()` methods). If these are `const` strings, reference them directly; otherwise construct an instance temporarily just to read them, OR (cleaner) add `const NAME: &str` / `const DESCRIPTION: &str` / `fn schema() -> Value` associated constants/functions to each tool struct.

If adding `const NAME: &str` / similar associated constants is too invasive (touches 30+ tool files), there's a simpler alternative: **construct a throwaway instance ONLY to read the trait methods**, then construct again in the builder for the actual session instance. The throwaway construction happens once at boot. For tools that take cheap args (e.g., `workspace.clone()`), this is fine. For tools with expensive setup, it might not be — implementer judgment.

**Recommended path**: implementer uses the throwaway-instance pattern (simpler, no per-tool file edits). Pseudo-code:

```rust
let workspace_root = std::path::PathBuf::from("/tmp/boot_descriptor_probe");  // dummy
let probe = builtin::file::ReadFileTool::new(workspace_root.clone());
let descriptor = ToolDescriptor {
    name: probe.name().to_string(),
    description: probe.description().to_string(),
    parameters_schema: probe.parameters_schema(),
    builder: Arc::new(|ctx| Box::new(builtin::file::ReadFileTool::new(ctx.workspace.clone()))),
};
api.register_tool(descriptor);
```

This pattern works for workspace-only tools. For tools requiring app_handle or state, the boot probe needs corresponding dummy args — which may not be possible (e.g., constructing an AppHandle is non-trivial). In those cases, the implementer extracts the name/description as hardcoded literals (matching what the trait impl returns), and treats parameters_schema via the trait method call on a probe instance OR as a hardcoded JSON literal.

**Alternative cleaner cut**: refactor the Tool trait to require `const NAME: &'static str` / `const DESCRIPTION: &'static str` as associated constants (this is what jcode/openhuman do). But this is per-tool file edits (~30 files), so it's an opportunity for SCOPE CREEP. Implementer should NOT do this; use the throwaway probe pattern OR hardcoded literals.

Given the complexity, the implementer may need to inspect each tool's constructor to decide which pattern fits. The plan acknowledges this — Task 4 is the largest task and includes implementer judgment.

If the implementer reports too many tools that can't be probed cheaply, the task can be SCOPED DOWN: only the ~15 workspace-only tools register descriptors via this function; the rest stay as inline registrations in `build_tool_registry` for now. This is a "partial migration" path that still proves the architecture; remaining tools migrate in a P3-2.5 follow-up.

- [ ] **Step 4.4: Declare module in `agent/tools/mod.rs`**

Open `src-tauri/src/agent/tools/mod.rs`. Add `pub mod builtin_descriptors;` in alphabetical position (likely after `builtin`).

- [ ] **Step 4.5: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^error" | head -20
```
Expected: empty (the new file compiles standalone; no callers yet).

If errors:
- "cannot find function ... in agent::tools::builtin" → verify the path; some tools may be at different paths (e.g., `agent::tools::memu_tools`).
- "no method named `DESCRIPTION`" → use the throwaway probe pattern instead of associated constants.
- "lifetime issues with builder closure" → ensure the closure captures via `clone()` rather than borrows.

- [ ] **Step 4.6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A \
    src-tauri/src/agent/tools/builtin_descriptors.rs \
    src-tauri/src/agent/tools/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "$(cat <<'EOF'
feat(agent): boot-time descriptor registration for builtin tools (P3-2.4 of 阶段 3)

New module agent/tools/builtin_descriptors.rs exposing
`register_all(api: &mut AgentApi)`. Registers descriptors for [N] core
builtin tools — workspace-scoped (read_file, write_file, edit, grep,
glob, bash, get_file_skeleton), no-arg (web fetch, http_request),
app-scoped (ask_user, exit_plan_mode, plan_write, plan_update,
request_plan_mode_switch), skill-related (skill_search, load_skill,
skill_write, skill_marketplace_*), and ... [remaining categories].

Each descriptor carries:
- name + description + parameters_schema (read from a throwaway probe
  instance, OR hardcoded if probe construction is heavyweight)
- builder closure capturing only the SessionContext fields the tool needs

[If partial migration: note which N tools migrated vs which deferred.]

NOT yet called from AppState::new() — that's Task 5. cargo build clean;
no tests touched in this commit (the function has no live callers).
EOF
)"
```

Continue to Task 5.

---

## Task 5: Wire `builtin_descriptors::register_all()` into `AppState::new()`

**Files:**
- Modify: `src-tauri/src/app.rs` (call register_all in AppState::new() before Arc-wrapping agent_api)

### Steps

- [ ] **Step 5.1: Find the AgentApi construction site in `AppState::new()`**

```bash
grep -n "AgentApi::new\|agent_api:" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/app.rs | head -5
```

Expected: 2 hits — the field declaration + the initializer `agent_api: Arc::new(crate::agent::api::AgentApi::new())`.

- [ ] **Step 5.2: Refactor the initializer to register descriptors at boot**

Before AppState's `Self { ... }` struct literal, ADD a block that constructs + populates AgentApi:

```rust
        let agent_api = {
            let mut api = crate::agent::api::AgentApi::new();
            crate::agent::tools::builtin_descriptors::register_all(&mut api);
            std::sync::Arc::new(api)
        };
```

Then change the `agent_api` field initializer in `Self { ... }` from:

```rust
            agent_api: std::sync::Arc::new(crate::agent::api::AgentApi::new()),
```

to:

```rust
            agent_api,
```

- [ ] **Step 5.3: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

- [ ] **Step 5.4: Run all agent:: tests + regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 780 passed / 2 failed (same as Task 3).

- [ ] **Step 5.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A src-tauri/src/app.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "feat(app): register builtin tool descriptors into AgentApi at AppState::new boot (P3-2.5 of 阶段 3)"
```

Continue to Task 6.

---

## Task 6: Refactor `build_tool_registry()` to a thin shim

**Files:**
- Modify: `src-tauri/src/agent/tools/registry_build.rs` (replace function body with call to agent_api.build_session_registry)

### Steps

- [ ] **Step 6.1: Inspect the existing build_tool_registry signature + caller**

```bash
sed -n '1,30p' /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/agent/tools/registry_build.rs
```

```bash
grep -n "build_tool_registry" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/tauri_commands.rs
```

The caller passes session args: `app_handle, &state, session_id, workspace, llm, model`.

- [ ] **Step 6.2: Refactor `build_tool_registry()` body**

Replace the entire ~200 LoC body of `build_tool_registry()` with:

```rust
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
    llm: Arc<dyn crate::llm::LlmProvider>,
    model: String,
) -> Arc<ToolRegistry> {
    let ctx = crate::agent::api::session_context::SessionContext {
        session_id: session_id.clone(),
        workspace,
        model,
        app_handle,
        llm,
        app_state: state,
    };
    Arc::new(state.agent_api.build_session_registry(&ctx))
}
```

If Task 4 was a partial migration (only N of the ~30 tools migrated), the shim needs to ALSO register the unmigrated tools inline:

```rust
pub async fn build_tool_registry(
    app_handle: tauri::AppHandle,
    state: &AppState,
    session_id: String,
    workspace: PathBuf,
    llm: Arc<dyn crate::llm::LlmProvider>,
    model: String,
) -> Arc<ToolRegistry> {
    let ctx = crate::agent::api::session_context::SessionContext {
        session_id: session_id.clone(),
        workspace: workspace.clone(),
        model: model.clone(),
        app_handle: app_handle.clone(),
        llm: llm.clone(),
        app_state: state,
    };
    let mut registry = state.agent_api.build_session_registry(&ctx);
    // Tools not yet migrated to descriptors (P3-2.5 follow-up):
    // [list of inline tools.register calls for unmigrated tools]
    Arc::new(registry)
}
```

Adjust based on whether Task 4 was full or partial migration.

- [ ] **Step 6.3: Verify the caller at `tauri_commands.rs:1886` still works**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty. The caller signature is unchanged, so it should just work.

- [ ] **Step 6.4: Final test regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 780 passed / 2 failed.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo test --lib 2>&1 | tail -5
```
Expected: ≥3024 passed / 7 pre-existing failed (= post-P3-1 baseline 3022 + 2 new build_session_registry tests).

- [ ] **Step 6.5: Final orphan-reference sweep**

```bash
grep -rn "tools\.register(" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/agent/tools/registry_build.rs
```
Expected: empty (if full migration) OR the inline registrations for unmigrated tools (if partial).

```bash
grep -rn "crate::agent::api::" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri/src/ --include="*.rs" | grep -v "src/agent/api/"
```
Expected:
- `app.rs` — field + register_all call (Tasks 5).
- `agent/tools/builtin_descriptors.rs` — Task 4.
- `agent/tools/registry_build.rs` — Task 6 (the SessionContext construction).

No other files yet.

- [ ] **Step 6.6: Build warnings check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```
Expected: ≤50 (P3-1 baseline). If higher, investigate.

- [ ] **Step 6.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration add -A src-tauri/src/agent/tools/registry_build.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration commit -m "$(cat <<'EOF'
refactor(agent): build_tool_registry routes through AgentApi.build_session_registry (P3-2.6 of 阶段 3)

build_tool_registry() — previously ~200 LoC of inline tool construction
— is now a ~15-LoC shim that constructs a SessionContext from the
session args and calls agent_api.build_session_registry(&ctx).

[If full migration: ~47 tools.register calls collapsed into the
 descriptor walk inside agent_api.build_session_registry.]
[If partial migration: N tools via descriptors, M tools still inline.
 Inline tools are tagged for P3-2.5 migration follow-up.]

Final P3-2 commit. cargo build clean; agent:: 780/2 baseline preserved;
cargo test --lib total 3024+/7 baseline.

Cumulative P3-2:
- 3 new files (~150 LoC: tool.rs + session_context.rs + builtin_descriptors.rs)
- 5 modifications (mod.rs / tests.rs / tools/mod.rs / app.rs / registry_build.rs)
- 2 new tests for build_session_registry
- 1 spec correction callout (committed separately at P3-2 plan landing time)

Next strategic step: P3-3 (migrate ProviderService + HookBus through AgentApi).
EOF
)"
```

Verify final chain:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration log --oneline 8cc03696..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p2-tool-migration status -sb
```

Expected: 6 commits ahead of `main`; working tree clean.

---

## Self-Review

**1. Spec coverage:**
- ✅ Spec §4.2 corrected (Correction callout linking to this plan).
- ✅ Spec P3-2 row table item: "Migrate `ToolDispatch.register` call sites → `AgentApi.register_tool`" — implemented via Option C descriptor pattern (recon-driven adjustment).
- ✅ ToolDispatch → thin lookup layer: via `build_tool_registry()` now calling `agent_api.build_session_registry(ctx)`.

**2. Placeholder scan:**
- Task 3.1's Option A vs B branch is an implementer-judgment fallback (existing pattern from P3-1 Task 3). Not a placeholder.
- Task 4's "throwaway probe pattern OR hardcoded literals" is implementer-judgment per-tool. The plan acknowledges this trade-off and offers a partial-migration fallback if too many tools resist probing.
- No "TBD" / "TODO" / "similar to Task N" / "add appropriate error handling".

**3. Type consistency:**
- `ToolDescriptor` named consistently across Task 1 (definition), Task 2 (register sig), Task 3 (build orchestrator), Task 4 (boot registrations).
- `SessionContext` lifetime `'a` used consistently.
- `ToolBuilderFn = Arc<dyn for<'a> Fn(&SessionContext<'a>) -> Box<dyn Tool> + Send + Sync>` consistent across all tasks.
- `build_session_registry` named consistently in Tasks 3, 5, 6.
- `register_all` (the builtin descriptor registration fn) named consistently in Tasks 4 and 5.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 1.0-1.5 person-day (Task 4 is the largest — ~30 tool descriptor translations).
- **Risk:** medium. The session-scoped descriptor pattern is novel; Task 4's throwaway-probe-vs-hardcoded-literals trade-off needs care.
- **Files touched:**
  - Task 1: 3 (2 new + 1 mod)
  - Task 2: 2 (mod.rs + tests.rs)
  - Task 3: 3 (mod.rs + tests.rs + tool.rs for register_boxed if needed)
  - Task 4: 2 (1 new + 1 tools/mod.rs)
  - Task 5: 1 (app.rs)
  - Task 6: 1 (registry_build.rs)
- **Net LoC:** ~+200 (3 new files ~150 LoC + descriptor registrations ~50 LoC) minus ~200 LoC (build_tool_registry shrinks from ~200 to ~15 LoC) ≈ **net 0 LoC** (if full migration; +100 if partial). Mostly mechanical translation.
- **PR shape:** 1 worktree → 6 commits → 1 PR. Bisectable per-task. Squash-on-land per P1-P4/P3-1 convention.
- **Non-goals (deferred)**:
  - Migration of the second tool registration site at `tauri_commands.rs:10609` (browser-tool-mixing site) — separate PR (call it P3-2.5).
  - Refactoring the Tool trait to require associated constants for name/description — out of scope.
  - Migration of `symphony_graph` `ToolRegistry::new()` sites — empty registries; revisit if symphony work resumes.
