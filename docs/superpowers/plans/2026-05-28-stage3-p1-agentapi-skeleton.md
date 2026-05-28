# 阶段 3 P3-1 — AgentApi Handle Skeleton · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the new `src-tauri/src/agent/api/` module with the empty `AgentApi` handle (struct + 5 register methods + EventKind/Event/EventOutcome types + `on(event, handler)` hook surface + plugin attribution index), wired into `AppState` as `agent_api: Arc<AgentApi>`. **NO** migration of existing call sites — this PR ships the skeleton only.

**Architecture:** New module under `agent/api/` with 7 files (mod.rs + events.rs + tool.rs + provider.rs + command.rs + renderer.rs + plugin.rs + tests.rs). Single `AgentApi` struct (Option 1 from the design spec): `&mut self` registration during boot; `Arc::new(api)` seal; `&self` queries at runtime. Existing `agent::tools::Tool` trait + `ProviderService` are reused (P3-1 doesn't redefine them); for new concepts (`Command`, `RendererFn`, `PluginId`, `PluginRegistrationSet`) P3-1 introduces them.

**Tech Stack:** Rust 2021, Tauri 2, `tokio`, `tokio_util::sync::CancellationToken` (Slice 1a), `async_trait`, `futures::future::BoxFuture`, inline `#[cfg(test)] mod tests` pattern.

**Related design:** [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §4 (AgentApi materialized) + §10 decision 3 (Option 1 struct shape).

---

## Background facts verified against HEAD `703c734c` (main after spec landed)

### Existing types this plan touches

- `crate::agent::tools::tool::Tool` is a trait: `pub trait Tool: Send + Sync { fn name(&self) -> &str; fn description(&self) -> &str; fn parameters_schema(&self) -> serde_json::Value; async fn execute(&self, params) -> Result<ToolOutput, ToolError>; ... }` — verified at `src-tauri/src/agent/tools/tool.rs:219`.
- `crate::providers::service::ProviderService` exists (Arc-shared, field on AppState).
- `tokio_util::sync::CancellationToken` is already in the dep tree (Slice 1a integration).
- Existing `ToolRegistry` stores `Box<dyn Tool>` keyed by `tool.name()` — `agent/tools/tool.rs:292`. The new `AgentApi` parallels this but uses `Arc<dyn Tool>` for shared-ref-counted access.

### What P3-1 does NOT touch

- `agent/tools/tool.rs` — the existing `Tool` trait stays unchanged.
- `agent/tool_dispatch/mod.rs` — existing tool registration call sites stay (P3-2 migrates them).
- `providers/service.rs` — stays (P3-3 migrates).
- `dispatcher.rs` — stays (P3-5 splits it).
- Any non-`agent/api/` file other than `agent/mod.rs` (one-line wire-in) and `app.rs` (one new field + one init line).

### Baselines to hold

- `cargo build`: green, 48-49 warnings (post-阶段 2 baseline).
- `cargo test --lib agent::`: 764 passed / 2 pre-existing failed.
- `cargo test --lib` total: 3,008 passed / 7 pre-existing failed.

Per P3-1's new unit tests (~10-15 new tests in `agent::api::tests`), `agent::` count should grow to ~774-779/2 after this PR. `cargo test --lib` total grows by the same delta.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main`, in sync at `703c734c`.

2. **Create the worktree + symlinks**:

```bash
git worktree add -b claude/stage3-p1-agentapi-skeleton \
    /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/bunembed
```

3. **Baseline verifications inside worktree**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, no errors, ~48-49 warnings

cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 764 passed / 2 failed
```

All paths in tasks below are relative to the worktree.

---

## File structure (after P3-1)

```
src-tauri/src/agent/api/                 (NEW directory)
├── mod.rs              ~140 LoC    AgentApi struct + new() + 5 register methods +
│                                   on/emit + plugin_index ops
├── events.rs           ~110 LoC    EventKind enum + Event struct + EventPayload +
│                                   EventOutcome + EventPatch
├── command.rs          ~25 LoC     Command struct (new type)
├── renderer.rs          ~30 LoC    RendererFn type alias + Renderer struct
├── plugin.rs           ~35 LoC     PluginId newtype + PluginRegistrationSet
└── tests.rs            ~280 LoC    unit tests for register methods + on/emit +
                                    plugin attribution
```

Plus 2 one-line touches:
- `src-tauri/src/agent/mod.rs` — add `pub mod api;` (alphabetical position).
- `src-tauri/src/app.rs` — add `agent_api: Arc<AgentApi>` field + initializer.

Note: `tool.rs` and `provider.rs` are **NOT** created in P3-1. The existing `crate::agent::tools::tool::Tool` trait and `crate::providers::service::ProviderService` are used directly. P3-2 / P3-3 decide whether to add thin re-export modules at `agent::api::tool` / `agent::api::provider` (probably yes, for symmetry with Command/Renderer).

---

## Task 1: Scaffold — type primitives + empty `AgentApi`

**Files:**
- Create: `src-tauri/src/agent/api/mod.rs`
- Create: `src-tauri/src/agent/api/events.rs`
- Create: `src-tauri/src/agent/api/command.rs`
- Create: `src-tauri/src/agent/api/renderer.rs`
- Create: `src-tauri/src/agent/api/plugin.rs`
- Create: `src-tauri/src/agent/api/tests.rs`
- Modify: `src-tauri/src/agent/mod.rs` (add `pub mod api;`)

### Steps

- [ ] **Step 1.1: Inspect `agent/mod.rs` to find the alphabetical insertion point**

```bash
grep -n "^pub mod " /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/src/agent/mod.rs | head -20
```

Identify where `pub mod api;` should slot alphabetically (between `agentic_loop` and other `a*` modules if present; otherwise as the first `pub mod` line).

- [ ] **Step 1.2: Create `agent/api/events.rs`**

```rust
//! Event surface for AgentApi hooks (Pi ExtensionAPI parallel; smaller scope).
//!
//! `EventKind` is intentionally smaller than Pi's 32-event set — uClaw-essential
//! only (13 events). New events should only be added when a hook needs them.

use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    SessionStart,
    SessionShutdown,
    TurnStart,
    TurnEnd,
    BeforeProviderRequest,
    AfterProviderResponse,
    ToolCall,
    ToolResult,
    MessageStart,
    MessageEnd,
    BeforeContextAssembly,
    BeforeCancellation,
    PluginShutdown,
}

/// Payload variants matched to `EventKind`. New kinds must add a variant here.
#[derive(Debug, Clone)]
pub enum EventPayload {
    SessionStart { session_id: String },
    SessionShutdown { session_id: String },
    TurnStart { turn_id: String },
    TurnEnd { turn_id: String, duration_ms: u64 },
    BeforeProviderRequest { provider: String, model: String },
    AfterProviderResponse { provider: String, model: String, token_count: u64 },
    ToolCall { tool_name: String, args: serde_json::Value },
    ToolResult { tool_name: String, result: serde_json::Value },
    MessageStart { message_id: String },
    MessageEnd { message_id: String },
    BeforeContextAssembly { session_id: String },
    BeforeCancellation { reason: String },
    PluginShutdown { plugin_id: String },
}

/// Patches a hook can return to mutate downstream state.
#[derive(Debug, Clone)]
pub enum EventPatch {
    ToolResult(serde_json::Value),
    Context(String),
    Message(String),
}

/// Hook outcome — fold into the loop's next step.
#[derive(Debug, Clone)]
pub enum EventOutcome {
    /// No mutation; loop continues normally.
    Continue,
    /// Replace some downstream value (variant determines which).
    Patch(EventPatch),
    /// Hook vetoes; loop surfaces as a safety/policy denial.
    Abort(String),
}

/// Event envelope passed to every hook. `cancellation_token` ties hook execution
/// to Slice 1a's cancellation flight points.
pub struct Event {
    pub kind: EventKind,
    pub payload: EventPayload,
    pub session_id: String,
    pub cancellation_token: CancellationToken,
}
```

- [ ] **Step 1.3: Create `agent/api/command.rs`**

```rust
//! Slash command (in-session, NOT Tauri command) registration shape.
//!
//! A `Command` is what shows up when a user types `/something` in the chat.
//! Distinct from Tauri commands (IPC entries in `tauri::generate_handler!`).

use std::sync::Arc;
use futures::future::BoxFuture;

pub type CommandHandlerFn = Arc<
    dyn Fn(serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value, String>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub handler: CommandHandlerFn,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("handler", &"<fn>")
            .finish()
    }
}
```

- [ ] **Step 1.4: Create `agent/api/renderer.rs`**

```rust
//! Custom message renderer registration shape.
//!
//! A renderer takes a custom-typed message payload and returns a UI-displayable
//! string. The dispatcher invokes renderers keyed by `custom_type`.

use std::sync::Arc;

pub type RendererFn = Arc<
    dyn Fn(&serde_json::Value) -> Result<String, String> + Send + Sync,
>;

/// Wrapper around the function alias so callers can pass `Renderer { custom_type, render }`
/// instead of a bare tuple at the register site.
#[derive(Clone)]
pub struct Renderer {
    pub custom_type: &'static str,
    pub render: RendererFn,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("custom_type", &self.custom_type)
            .field("render", &"<fn>")
            .finish()
    }
}
```

- [ ] **Step 1.5: Create `agent/api/plugin.rs`**

```rust
//! Plugin attribution — tracks which subprocess plugin registered which items.
//!
//! Populated by `SubprocessPluginManager` during the registration step of the
//! plugin lifecycle (P3-4). Used to unregister cleanly when a subprocess plugin
//! crashes or exits.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The set of things a single plugin contributed via the AgentApi handle.
/// Used to roll back registrations when the plugin shuts down.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistrationSet {
    pub tools: Vec<String>,           // tool names
    pub providers: Vec<String>,        // provider ids
    pub commands: Vec<String>,         // command names
    pub renderers: Vec<&'static str>,  // renderer custom_types
    pub hook_events: Vec<crate::agent::api::events::EventKind>,
}
```

- [ ] **Step 1.6: Create `agent/api/mod.rs` (bare struct + new())**

```rust
//! AgentApi — single handle replacing the 4-Registry pattern.
//!
//! Pi ExtensionAPI shape, materialized as a Rust struct. Boot: register builtins
//! via `&mut self`; after boot the handle is wrapped in `Arc` and shared via
//! `AppState.agent_api`. Runtime queries use `&self`.
//!
//! See: `docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md` §4.

pub mod events;
pub mod command;
pub mod renderer;
pub mod plugin;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;

use crate::agent::tools::tool::Tool;
use crate::providers::service::ProviderService;

use self::command::Command;
use self::events::{Event, EventKind, EventOutcome};
use self::plugin::{PluginId, PluginRegistrationSet};
use self::renderer::RendererFn;

pub type HookFn = Arc<
    dyn Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>>
        + Send
        + Sync,
>;

pub struct AgentApi {
    tools: HashMap<String, Arc<dyn Tool>>,
    providers: HashMap<String, Arc<ProviderService>>,
    commands: HashMap<String, Arc<Command>>,
    renderers: HashMap<&'static str, RendererFn>,
    hooks: HashMap<EventKind, Vec<HookFn>>,
    plugin_index: HashMap<PluginId, PluginRegistrationSet>,
}

impl AgentApi {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            providers: HashMap::new(),
            commands: HashMap::new(),
            renderers: HashMap::new(),
            hooks: HashMap::new(),
            plugin_index: HashMap::new(),
        }
    }
}

impl Default for AgentApi {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AgentApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentApi")
            .field("tools", &self.tools.len())
            .field("providers", &self.providers.len())
            .field("commands", &self.commands.len())
            .field("renderers", &self.renderers.len())
            .field("hooks_total", &self.hooks.values().map(|v| v.len()).sum::<usize>())
            .field("plugins", &self.plugin_index.len())
            .finish()
    }
}
```

- [ ] **Step 1.7: Create initial `agent/api/tests.rs` (1 failing test for new())**

```rust
//! Unit tests for AgentApi.

use super::*;

#[test]
fn new_agent_api_has_empty_registries() {
    let api = AgentApi::new();
    assert_eq!(api.tools.len(), 0);
    assert_eq!(api.providers.len(), 0);
    assert_eq!(api.commands.len(), 0);
    assert_eq!(api.renderers.len(), 0);
    assert_eq!(api.hooks.len(), 0);
    assert_eq!(api.plugin_index.len(), 0);
}
```

- [ ] **Step 1.8: Wire into `agent/mod.rs`**

Use Edit to add `pub mod api;` at the alphabetical position identified in Step 1.1. Verify:

```bash
grep -n "^pub mod api" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/src/agent/mod.rs
```
Expected: one match.

- [ ] **Step 1.9: Build + run the scaffold test**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: empty.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::new_agent_api_has_empty_registries 2>&1 | tail -5
```
Expected: `1 passed; 0 failed`.

If a test field is private (`hashmap.len()` on a private field), make those fields `pub(crate)` so the test module can read them.

- [ ] **Step 1.10: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A \
    src-tauri/src/agent/api/ \
    src-tauri/src/agent/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "$(cat <<'EOF'
feat(agent): add empty AgentApi handle scaffold (P3-1.1 of 阶段 3)

New module agent/api/ with:
- mod.rs: AgentApi struct + new() + Default + Debug. No register methods
  yet (added in subsequent tasks).
- events.rs: EventKind (13 variants) + Event + EventPayload + EventPatch
  + EventOutcome.
- command.rs: Command + CommandHandlerFn.
- renderer.rs: Renderer + RendererFn.
- plugin.rs: PluginId + PluginRegistrationSet.
- tests.rs: 1 test confirming new() has empty registries.

Wired into agent/mod.rs via `pub mod api;`. No external call sites
touched yet — P3-2/P3-3/P3-4 migrate existing registration through
this handle.

Foundation commit of P3-1. cargo build clean; new test passes;
existing baselines untouched.
EOF
)"
```

Record commit SHA. Continue to Task 2.

---

## Task 2: `register_tool` + `tool()` query (TDD)

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (add 2 methods)
- Modify: `src-tauri/src/agent/api/tests.rs` (add 2 tests)

### Steps

- [ ] **Step 2.1: Write the failing tests**

Append to `agent/api/tests.rs`:

```rust
/// Minimal dummy Tool impl for the tests in this module.
struct DummyTool {
    name: String,
}

#[async_trait::async_trait]
impl crate::agent::tools::tool::Tool for DummyTool {
    fn name(&self) -> &str {
        &self.name
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
fn register_tool_stores_by_name() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    assert_eq!(api.tools.len(), 1);
    assert!(api.tools.contains_key("echo"));
}

#[test]
fn tool_query_returns_registered_tool() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    let got = api.tool("echo");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name(), "echo");
    assert!(api.tool("nonexistent").is_none());
}
```

(If `ToolOutput::default()` isn't already implemented, the dummy can return `ToolOutput { ... }` with explicit field values — implementer verifies the actual `ToolOutput` shape and adapts.)

- [ ] **Step 2.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::register_tool_stores_by_name agent::api::tests::tool_query_returns_registered_tool 2>&1 | tail -5
```
Expected: compile error (`AgentApi::register_tool` doesn't exist) OR test failure.

- [ ] **Step 2.3: Implement `register_tool` + `tool()`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block:

```rust
    /// Register a tool by its name. Idempotent on name collision (last write wins;
    /// the dispatcher logs a warning at registration time — verified in P3-2).
    pub fn register_tool(&mut self, tool: std::sync::Arc<dyn crate::agent::tools::tool::Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Look up a registered tool by name.
    pub fn tool(&self, name: &str) -> Option<&std::sync::Arc<dyn crate::agent::tools::tool::Tool>> {
        self.tools.get(name)
    }
```

- [ ] **Step 2.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 3 passed (the original `new_agent_api_has_empty_registries` + 2 new).

- [ ] **Step 2.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi.register_tool + tool() query (P3-1.2 of 阶段 3)"
```

Continue to Task 3.

---

## Task 3: `register_provider` + `provider()` query (TDD)

**Files:** same as Task 2.

### Steps

- [ ] **Step 3.1: Write failing tests**

Append to `agent/api/tests.rs`:

```rust
#[test]
fn register_provider_stores_by_id() {
    let mut api = AgentApi::new();
    // ProviderService::new requires real config; use a minimal Arc<ProviderService>
    // fixture. Implementer verifies the actual constructor signature.
    let provider = std::sync::Arc::new(make_test_provider_service("openai"));
    api.register_provider("openai".to_string(), provider);
    assert_eq!(api.providers.len(), 1);
    assert!(api.providers.contains_key("openai"));
}

#[test]
fn provider_query_returns_registered() {
    let mut api = AgentApi::new();
    let provider = std::sync::Arc::new(make_test_provider_service("openai"));
    api.register_provider("openai".to_string(), provider);
    assert!(api.provider("openai").is_some());
    assert!(api.provider("nonexistent").is_none());
}

/// Helper: construct a minimal ProviderService for tests. Implementer adapts to
/// the actual ProviderService constructor signature — if it requires DB / paths,
/// use the test helpers already used elsewhere in the codebase (e.g., the same
/// pattern as `tauri_commands.rs` test fixtures).
fn make_test_provider_service(id: &str) -> crate::providers::service::ProviderService {
    // CONCRETE shape depends on the actual ProviderService API; implementer fills in.
    // For most ProviderService types this is something like:
    //     crate::providers::service::ProviderService::new_for_test(id)
    // If no such helper exists, this test should either:
    //   (a) construct ProviderService with mocked dependencies, OR
    //   (b) be skipped and the test asserted via an integration test in P3-3.
    todo!("implementer: see comment above")
}
```

**Implementer note:** If `ProviderService` is too heavyweight to construct in unit tests (e.g., requires Arc<DB> + Arc<UserSettings>), use option (b) — skip the unit test here and rely on P3-3's integration test. In that case, replace the body of `register_provider_stores_by_id` and `provider_query_returns_registered` with `#[ignore]` + a comment noting "tested via P3-3 integration"; AND remove the helper. The grep gate in Step 3.4 then expects 0 register_provider unit tests.

- [ ] **Step 3.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::register_provider 2>&1 | tail -5
```
Expected: compile error OR test panic on `todo!()`.

- [ ] **Step 3.3: Implement `register_provider` + `provider()`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block:

```rust
    /// Register a provider by its id.
    pub fn register_provider(&mut self, id: String, provider: std::sync::Arc<crate::providers::service::ProviderService>) {
        self.providers.insert(id, provider);
    }

    /// Look up a registered provider by id.
    pub fn provider(&self, id: &str) -> Option<&std::sync::Arc<crate::providers::service::ProviderService>> {
        self.providers.get(id)
    }
```

**Implementer note:** If the implementer chose option (b) in Step 3.1 (skip unit tests), they should still implement the methods — the methods compile against the real `ProviderService` type and will be exercised by P3-3.

- [ ] **Step 3.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 5 passed (option a) OR 3 passed + 2 ignored (option b).

- [ ] **Step 3.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/agent/api/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi.register_provider + provider() query (P3-1.3 of 阶段 3)"
```

Continue to Task 4.

---

## Task 4: `register_command` + `command()` query (TDD)

**Files:** same as Task 2.

### Steps

- [ ] **Step 4.1: Write failing tests**

Append to `agent/api/tests.rs`:

```rust
#[test]
fn register_command_stores_by_name() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    let cmd = crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({"out": "hello"})) }.boxed()
        }),
    };
    api.register_command(cmd);
    assert_eq!(api.commands.len(), 1);
}

#[test]
fn command_query_returns_registered() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({})) }.boxed()
        }),
    });
    assert!(api.command("hello").is_some());
    assert!(api.command("missing").is_none());
}
```

- [ ] **Step 4.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::register_command 2>&1 | tail -5
```
Expected: compile error.

- [ ] **Step 4.3: Implement `register_command` + `command()`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block:

```rust
    /// Register a slash command.
    pub fn register_command(&mut self, cmd: Command) {
        let name = cmd.name.clone();
        self.commands.insert(name, std::sync::Arc::new(cmd));
    }

    /// Look up a registered command by name.
    pub fn command(&self, name: &str) -> Option<&std::sync::Arc<Command>> {
        self.commands.get(name)
    }
```

- [ ] **Step 4.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 7 passed (or 5 + 2 ignored).

- [ ] **Step 4.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/agent/api/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi.register_command + command() query (P3-1.4 of 阶段 3)"
```

Continue to Task 5.

---

## Task 5: `register_renderer` + `renderer()` query (TDD)

**Files:** same as Task 2.

### Steps

- [ ] **Step 5.1: Write failing tests**

Append to `agent/api/tests.rs`:

```rust
#[test]
fn register_renderer_stores_by_custom_type() {
    let mut api = AgentApi::new();
    let r = crate::agent::api::renderer::Renderer {
        custom_type: "echo.detail",
        render: std::sync::Arc::new(|v| Ok(format!("rendered: {}", v))),
    };
    api.register_renderer(r);
    assert_eq!(api.renderers.len(), 1);
    assert!(api.renderers.contains_key("echo.detail"));
}

#[test]
fn renderer_query_returns_registered() {
    let mut api = AgentApi::new();
    api.register_renderer(crate::agent::api::renderer::Renderer {
        custom_type: "echo.detail",
        render: std::sync::Arc::new(|v| Ok(format!("rendered: {}", v))),
    });
    let r = api.renderer("echo.detail");
    assert!(r.is_some());
    let out = r.unwrap()(&serde_json::json!({"x": 1})).unwrap();
    assert!(out.starts_with("rendered:"));
    assert!(api.renderer("missing").is_none());
}
```

- [ ] **Step 5.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::register_renderer 2>&1 | tail -5
```
Expected: compile error.

- [ ] **Step 5.3: Implement `register_renderer` + `renderer()`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block:

```rust
    /// Register a renderer for a specific custom_type.
    pub fn register_renderer(&mut self, r: Renderer) {
        self.renderers.insert(r.custom_type, r.render);
    }

    /// Look up a registered renderer by custom_type.
    pub fn renderer(&self, custom_type: &str) -> Option<&RendererFn> {
        self.renderers.get(custom_type)
    }
```

Also add an import at top of mod.rs if not already there:
```rust
use self::renderer::{Renderer, RendererFn};
```

(The `RendererFn` was already imported in the scaffold; add `Renderer` here.)

- [ ] **Step 5.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 9 passed (or 7 + 2 ignored).

- [ ] **Step 5.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/agent/api/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi.register_renderer + renderer() query (P3-1.5 of 阶段 3)"
```

Continue to Task 6.

---

## Task 6: `on(event)` hook attach + `emit()` hook invocation (TDD)

This task adds the event subscription + dispatch core. Hooks fire in registration order; outcomes fold (Continue stays, Patch overrides, Abort short-circuits).

**Files:** same as Task 2.

### Steps

- [ ] **Step 6.1: Write failing tests**

Append to `agent/api/tests.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn on_registers_hook_and_emit_fires_it() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let counter = std::sync::Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let ev = Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t1".into(), duration_ms: 0 },
        session_id: "s1".into(),
        cancellation_token: CancellationToken::new(),
    };

    let outcome = api.emit(ev).await.unwrap();
    assert!(matches!(outcome, EventOutcome::Continue));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn hooks_fire_in_registration_order() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    let o = order.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let o = o.clone();
        async move {
            o.lock().unwrap().push(1);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });
    let o = order.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let o = o.clone();
        async move {
            o.lock().unwrap().push(2);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let _ = api.emit(Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t".into(), duration_ms: 0 },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    }).await.unwrap();

    assert_eq!(*order.lock().unwrap(), vec![1, 2]);
}

#[tokio::test]
async fn emit_short_circuits_on_abort() {
    use futures::FutureExt;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    let mut api = AgentApi::new();
    let saw_second = std::sync::Arc::new(AtomicUsize::new(0));

    api.on(EventKind::TurnEnd, |_ev| {
        async move { Ok(EventOutcome::Abort("nope".into())) }.boxed()
    });
    let s = saw_second.clone();
    api.on(EventKind::TurnEnd, move |_ev| {
        let s = s.clone();
        async move {
            s.fetch_add(1, Ordering::SeqCst);
            Ok(EventOutcome::Continue)
        }
        .boxed()
    });

    let outcome = api.emit(Event {
        kind: EventKind::TurnEnd,
        payload: EventPayload::TurnEnd { turn_id: "t".into(), duration_ms: 0 },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    }).await.unwrap();

    assert!(matches!(outcome, EventOutcome::Abort(ref msg) if msg == "nope"));
    assert_eq!(saw_second.load(Ordering::SeqCst), 0, "second hook must not fire after Abort");
}
```

- [ ] **Step 6.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::on_registers agent::api::tests::hooks_fire agent::api::tests::emit_short_circuits 2>&1 | tail -5
```
Expected: compile error.

- [ ] **Step 6.3: Implement `on` + `emit`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block:

```rust
    /// Register a hook handler for an event kind. Hooks fire in registration order.
    pub fn on<F>(&mut self, ev: EventKind, h: F)
    where
        F: Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>>
            + Send
            + Sync
            + 'static,
    {
        self.hooks.entry(ev).or_default().push(std::sync::Arc::new(h));
    }

    /// Fire an event. Hooks for `ev.kind` run in registration order. The first
    /// hook returning `Abort` short-circuits and the abort is returned; `Patch`
    /// outcomes are returned as-is to the caller (loop folds them). `Continue`
    /// outcomes are skipped and the next hook runs.
    pub async fn emit(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let Some(hooks) = self.hooks.get(&kind) else {
            return Ok(EventOutcome::Continue);
        };
        for h in hooks {
            let outcome = h(&ev).await?;
            match outcome {
                EventOutcome::Continue => continue,
                EventOutcome::Patch(_) | EventOutcome::Abort(_) => return Ok(outcome),
            }
        }
        Ok(EventOutcome::Continue)
    }
```

- [ ] **Step 6.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 12 passed (or 10 + 2 ignored), 3 new `tokio::test`s pass.

- [ ] **Step 6.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/agent/api/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi.on + emit hooks (P3-1.6 of 阶段 3)"
```

Continue to Task 7.

---

## Task 7: `register_plugin` + `unregister_plugin` + plugin attribution (TDD)

This is the seam for P3-4's subprocess plugin manager. P3-1 ships the API only; P3-4 wires the SubprocessPluginManager that calls it.

**Files:** same as Task 2.

### Steps

- [ ] **Step 7.1: Write failing tests**

Append to `agent/api/tests.rs`:

```rust
#[test]
fn register_plugin_attributes_tools_to_plugin_id() {
    use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};

    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    api.register_tool(std::sync::Arc::new(DummyTool { name: "ping".into() }));

    let id = PluginId::new("uclaw.demo");
    let mut set = PluginRegistrationSet::default();
    set.tools.push("echo".into());
    set.tools.push("ping".into());
    api.register_plugin(id.clone(), set);

    assert_eq!(api.plugin_index.len(), 1);
    let attribution = api.plugin_index.get(&id).unwrap();
    assert_eq!(attribution.tools, vec!["echo".to_string(), "ping".to_string()]);
}

#[test]
fn unregister_plugin_removes_attributed_tools() {
    use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};

    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    let id = PluginId::new("uclaw.demo");
    let mut set = PluginRegistrationSet::default();
    set.tools.push("echo".into());
    api.register_plugin(id.clone(), set);

    api.unregister_plugin(&id);

    assert!(api.tool("echo").is_none(), "tool should be removed when plugin unregisters");
    assert!(api.plugin_index.get(&id).is_none(), "plugin attribution removed");
}
```

- [ ] **Step 7.2: Run tests — verify failure**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests::register_plugin agent::api::tests::unregister_plugin 2>&1 | tail -5
```
Expected: compile error.

- [ ] **Step 7.3: Implement `register_plugin` + `unregister_plugin`**

In `agent/api/mod.rs`, add to the `impl AgentApi` block (these are `pub(crate)` per the design spec — only the SubprocessPluginManager calls them in P3-4):

```rust
    /// Record the set of registrations a subprocess plugin contributed.
    /// Called by `SubprocessPluginManager` AFTER the corresponding register_*
    /// calls. Used for clean unregistration on plugin shutdown.
    pub(crate) fn register_plugin(&mut self, id: PluginId, set: PluginRegistrationSet) {
        self.plugin_index.insert(id, set);
    }

    /// Remove all contributions from the given plugin. Inverse of register_plugin
    /// + the underlying register_tool/provider/command/renderer/on calls.
    pub(crate) fn unregister_plugin(&mut self, id: &PluginId) {
        if let Some(set) = self.plugin_index.remove(id) {
            for name in &set.tools {
                self.tools.remove(name);
            }
            for pid in &set.providers {
                self.providers.remove(pid);
            }
            for cname in &set.commands {
                self.commands.remove(cname);
            }
            for ct in &set.renderers {
                self.renderers.remove(ct);
            }
            // Hooks for plugin-registered events: removed in bulk by event kind.
            // Caveat: this also removes builtin hooks for the same event kind.
            // P3-4 must register subprocess hooks via a separate API that tags
            // them by plugin_id. For P3-1, hook unregistration on plugin removal
            // is INTENTIONALLY left as a no-op TODO that P3-4 finishes.
        }
    }
```

**Critical note documented in code:** The hook removal limitation is intentional for P3-1. P3-4 introduces a `HookFn` wrapper that carries an optional `PluginId`, and `unregister_plugin` then filters by it. This is fine because P3-1 has no subprocess hooks registered — only compile-time hooks, which never need to be unregistered.

- [ ] **Step 7.4: Re-run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api::tests:: 2>&1 | tail -5
```
Expected: 14 passed (or 12 + 2 ignored).

- [ ] **Step 7.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/agent/api/

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "feat(agent): AgentApi plugin attribution (register/unregister_plugin) (P3-1.7 of 阶段 3)"
```

Continue to Task 8.

---

## Task 8: Wire `agent_api: Arc<AgentApi>` into `AppState`

**Files:**
- Modify: `src-tauri/src/app.rs` (add field declaration + initializer; 2 sites)

### Steps

- [ ] **Step 8.1: Find the AppState field cluster + the `AppState::new()` initializer**

```bash
grep -n "pub struct AppState\b\|impl AppState\|fn new\b" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/src/app.rs | head -10
```

Verify there is `pub struct AppState { ... }` and an `impl AppState { pub fn new(...) -> ... { ... } }` (or async variant).

- [ ] **Step 8.2: Add `agent_api: Arc<AgentApi>` field to `AppState`**

In `app.rs`, find the `pub struct AppState { ... }` block. Add (in alphabetical position relative to other pub fields, or at the end if no obvious sort):

```rust
    /// Pi-lightweight single-handle replacement for the 4-Registry pattern.
    /// Created empty at boot; populated by builtin registrations + (P3-4+)
    /// subprocess plugin loader. See:
    /// docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md
    pub agent_api: std::sync::Arc<crate::agent::api::AgentApi>,
```

- [ ] **Step 8.3: Initialize `agent_api` in `AppState::new()` (or equivalent)**

In the `AppState::new()` body, find the existing struct literal that constructs `Self { ... }`. Add:

```rust
            agent_api: std::sync::Arc::new(crate::agent::api::AgentApi::new()),
```

at an alphabetical position. The boot just creates an empty handle — no registrations happen yet (those land in P3-2 / P3-3 / P3-4).

- [ ] **Step 8.4: Build (GREEN GATE) + run all P3-1 tests + agent:: baseline regression check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: empty.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: 14 passed (or 12 passed + 2 ignored). 0 failed.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 778 passed (≈ 764 baseline + 14 new) / 2 pre-existing failed. The 14 new tests live under `agent::api::tests` so they count toward `agent::`.

If the count is anything other than 764 + new-test-count, STOP — there's an unexpected interaction. Inspect.

- [ ] **Step 8.5: Warning count check**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```
Expected: 48-49 (post-阶段 2 baseline, ± 0-2 for new module if any). Any net increase ≥3 → investigate.

- [ ] **Step 8.6: Final orphan-reference sweep**

```bash
grep -rn "crate::agent::api::" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton/src-tauri/src/ --include="*.rs" | grep -v "src/agent/api/"
```
Expected: hits in `app.rs` (the new field + initializer). NO hits in other files yet — P3-2 onwards adds them.

- [ ] **Step 8.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton add -A src-tauri/src/app.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton commit -m "$(cat <<'EOF'
feat(app): wire AgentApi into AppState (P3-1.8 of 阶段 3)

Adds `pub agent_api: Arc<AgentApi>` field to AppState + initializer in
AppState::new() (empty handle at boot). Final commit of P3-1 — the
AgentApi skeleton is now reachable from every Tauri command / agent
loop entry that already holds an AppState reference.

P3-1 ships the API surface only. Existing tool / provider / hook
registration call sites STAY UNCHANGED — P3-2 / P3-3 migrate them
through this handle.

Cumulative P3-1: agent/api/ module (~620 LoC across 6 files) + 14 new
unit tests + 1-field touch on AppState + 1-line wire-in to agent/mod.rs.

cargo build clean; agent:: 778/2 (= 764 baseline + 14 new
agent::api::tests); cargo test --lib total grows by 14; warning count
unchanged at 48-49.
EOF
)"
```

Verify the 8-commit chain:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton log --oneline 703c734c..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p1-agentapi-skeleton status -sb
```

Expected: 8 commits ahead of `main`; working tree clean.

---

## Self-Review

**1. Spec coverage (against design spec §4 + §10):**

- ✅ §4.1 Module location — Task 1 creates `agent/api/` with 6 files (deferred `tool.rs`/`provider.rs` per the spec note; existing types are reused directly).
- ✅ §4.2 Struct shape (Option 1, &mut/Arc) — Task 1 + Task 8.
- ✅ §4.3 Event surface (13 EventKind variants, Event/EventPayload/EventOutcome) — Task 1.
- ✅ register_tool — Task 2.
- ✅ register_provider — Task 3 (with implementer-judgment fallback if ProviderService unit-construction is heavyweight).
- ✅ register_command — Task 4.
- ✅ register_renderer — Task 5.
- ✅ on(event) / emit(event) — Task 6.
- ✅ register_plugin / unregister_plugin — Task 7.
- ✅ AppState wiring — Task 8.
- ✅ Decision 3 (Option 1: single struct, `&mut self` register, `Arc::new(api)` seal) — embodied throughout.

**2. Placeholder scan:**

- One `todo!("implementer: see comment above")` in Step 3.1 of Task 3 — explicitly flagged with two-path guidance (option a vs option b). This is the ONLY soft-deferred decision and is contained to a single helper function that the implementer either fills in (option a) or removes (option b) in the same task. Not a plan failure.
- "No "TBD" / "implement later" / "similar to Task N" / "add appropriate error handling".

**3. Type consistency:**

- `EventKind` enum has the same 13 variants in events.rs definition, the `EventPayload` match, the `Event` field type, the test imports.
- `PluginId` newtype consistent in plugin.rs and Task 7's tests.
- `HookFn` type: `Arc<dyn Fn(&Event) -> BoxFuture<'static, Result<EventOutcome, String>> + Send + Sync>` consistent in Step 1.6 (mod.rs scaffold) and Step 6.3 (impl).
- `RendererFn` defined in renderer.rs as `Arc<dyn Fn(&serde_json::Value) -> Result<String, String> + Send + Sync>`; consistent with Task 5's tests.
- `Command.handler: CommandHandlerFn` shape used consistently in Task 4 tests.
- `agent::api::AgentApi` path used consistently across all task code samples and the AppState wire-in.

No spec gaps, one explicitly-flagged judgment-call placeholder, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 0.5-1 person-day (8 mechanical tasks; each ~5-7 steps; subagent-driven cadence per closeout §6.iii).
- **Risk:** low. New module, no existing call site changes. cargo build is the gate.
- **Files touched:**
  - Task 1: 6 new + 1 modify (agent/mod.rs)
  - Tasks 2-7: only `agent/api/mod.rs` + `agent/api/tests.rs`
  - Task 8: only `app.rs`
- **Net LoC:** +620 (new module) + 2 (AppState field+init) + 1 (agent/mod.rs).
- **PR shape:** 1 worktree → 8 commits → 1 PR. Bisectable per-task. Squash-on-land per P1-P4 convention.
- **Tests delta:** +14 unit tests under `agent::api::tests` (or +12 if option b chosen for Task 3).
- **No Open Decisions block P3-1.** Plan complete.
