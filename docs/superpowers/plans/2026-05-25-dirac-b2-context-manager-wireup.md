# Dirac-B2 — ContextManager Wire-Up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Steps use `- [ ]` syntax.
>
> ⚠️ **C2 ORDERING**: Do NOT start until C1 closes (A1-A4 merged). B2 also requires A4 specifically (uses `InjectionContext`). B1 and B2 are parallelizable.

**Goal:** Wire the M2-B `ContextManager` pilot into `ChatDelegate::effective_system_prompt` and `build_dynamic_context`. Register `ContextToolSet::search` + `read` as builtin tools. Expose `ComposeStats` via Tauri command.

**Architecture:** ChatDelegate gains a per-session `Arc<RwLock<ContextManager>>` + injection state (is_first_act_turn, last_error_kind). `effective_system_prompt` calls `ContextManager::for_prompt_with_injection(query, inj_ctx)` (new method) which uses A4's `render_with_context`. Fragments inject via `build_dynamic_context` (per-turn block, not system prompt — preserves cache discipline).

**Tech Stack:** Rust only. No new crates. Uses `tokio::task::block_in_place` for sync→async bridge.

**Spec:** `docs/superpowers/specs/2026-05-25-dirac-b2-context-manager-wireup-design.md`

**PR tag:** `[C2-Dirac-B2]`

**Depends on:**
- C1-Dirac-A4 merged (uses `InjectionContext` + `BaselineBlockRegistry::render_with_context`)
- B1 is independent — parallelizable

---

## File Structure

### Modified files

| Path | What changes |
|---|---|
| `src-tauri/src/agent/context_manager/manager.rs` | Add `for_prompt_with_injection` method + back-compat `for_prompt` shim. ~30 lines. |
| `src-tauri/src/agent/dispatcher.rs` | New fields on ChatDelegate; rewrite `effective_system_prompt` to route through ContextManager; extend `build_dynamic_context` with fragment injection. ~150 lines. |
| `src-tauri/src/tauri_commands.rs` | Add `get_compose_stats` command. ~30 lines. |
| `src-tauri/src/main.rs` | Register `get_compose_stats` in invoke_handler + register context.* tools at the same place EditTool / ReadFileTool are registered. ~10 lines. |

### New files

| Path | Purpose |
|---|---|
| `src-tauri/src/agent/tools/builtin/context_tools_adapter.rs` | `ContextSearchTool` + `ContextReadTool` wrapping `ContextToolSet`. ~150 lines. |
| `src-tauri/tests/context_wireup_bench.rs` | Integration test: 20-turn fixture verifying fragments_selected > 0. ~100 lines. |

---

## Pre-flight

- [ ] **Step 0.1: Confirm C1 + A4 merged**

```bash
cd /Users/ryanliu/Documents/uclaw
git fetch origin && git checkout main && git pull
grep -A 1 "C1-Dirac-A4\|render_with_context\|InjectionContext" docs/superpowers/MILESTONE_STATUS.md
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
```

Expect: A4 row shows merged. Drift GREEN/YELLOW. If A4 not merged →
STOP.

- [ ] **Step 0.2: Branch + baseline**

```bash
git checkout -b claude/dirac-b2-context-manager-wireup
cd src-tauri && cargo test --lib agent::context_manager 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::baseline_blocks 2>&1 | tail -10
```

Expect: all green.

- [ ] **Step 0.3: Discover tokio runtime shape**

```bash
grep -n "tokio::main\|new_multi_thread\|new_current_thread" src-tauri/src/main.rs src-tauri/src/lib.rs
```

Confirm multi-thread runtime. If single-thread, `block_in_place`
won't work — use channel-based pattern instead (`oneshot` + tokio
spawn). Document in commit if non-default.

- [ ] **Step 0.4: Confirm ContextArtifact / ContextFragment fields**

```bash
grep -n "struct ContextArtifact\|struct ContextRef\|impl ContextFragment" src-tauri/src/runtime/context.rs | head -20
```

We need:
- `ContextArtifact { ref_id, content }` or similar — used in fragment injection
- `ContextRef { id }` or similar — returned by `context.search`

Adapt field names in Task 5 to match actual struct shapes.

- [ ] **Step 0.5: Locate builtin tool registration site**

```bash
grep -n "EditTool::new\|ReadFileTool::new\|ToolRegistry\|register_tool" src-tauri/src/main.rs src-tauri/src/agent/dispatcher.rs | head -20
```

Identify the function/site where EditTool, ReadFileTool etc. get
registered. Our context.* tools go in the same place.

---

## Task 1: Extend `ContextManager` with InjectionContext support

**Files:**
- Modify: `src-tauri/src/agent/context_manager/manager.rs`

- [ ] **Step 1.1: Add `for_prompt_with_injection`**

After the existing `for_prompt` method:

```rust
use crate::agent::baseline_blocks::InjectionContext;

impl ContextManager {
    /// Same as `for_prompt` but uses A4's render_with_context for
    /// the baseline. Lets the caller pass per-turn injection state
    /// (is_first_act_turn, last_error_kind, context_pressure_ratio)
    /// so FirstActTurnOnly / OnErrorRecovery / OnContextPressure
    /// blocks are gated correctly.
    pub async fn for_prompt_with_injection(
        &self,
        query: &ComposeQuery,
        injection_ctx: &InjectionContext,
    ) -> ComposedContext {
        let mut stats = ComposeStats {
            fragments_available: self.fragments.len(),
            ..Default::default()
        };

        // A4: use injection-aware render
        let system_prompt = crate::agent::baseline_blocks::render_with_context(injection_ctx);

        if query.max_fragments == 0 || query.fragment_token_budget == 0 {
            return ComposedContext {
                system_prompt,
                injected_fragments: Vec::new(),
                stats,
            };
        }

        // ... rest of body identical to existing for_prompt — scoring,
        // selection, fetch loop ...
        // (Copy from existing for_prompt; only the system_prompt line above differs)
    }
}
```

- [ ] **Step 1.2: Convert `for_prompt` to back-compat shim**

```rust
impl ContextManager {
    /// Back-compat — calls for_prompt_with_injection with baseline()
    /// context (renders all Always-policy blocks, skips others).
    pub async fn for_prompt(&self, query: &ComposeQuery) -> ComposedContext {
        self.for_prompt_with_injection(query, &InjectionContext::baseline()).await
    }
}
```

Risk: the existing `for_prompt` body is duplicated inside
`for_prompt_with_injection`. To avoid code drift, extract the shared
body into a private `for_prompt_inner` taking a `system_prompt: String`:

```rust
async fn for_prompt_inner(
    &self,
    query: &ComposeQuery,
    system_prompt: String,
) -> ComposedContext {
    // Existing body using `system_prompt` argument
}

pub async fn for_prompt(&self, query: &ComposeQuery) -> ComposedContext {
    let sp = self.baseline_system_prompt();
    self.for_prompt_inner(query, sp).await
}

pub async fn for_prompt_with_injection(
    &self,
    query: &ComposeQuery,
    injection_ctx: &InjectionContext,
) -> ComposedContext {
    let sp = crate::agent::baseline_blocks::render_with_context(injection_ctx);
    self.for_prompt_inner(query, sp).await
}
```

Cleaner.

- [ ] **Step 1.3: Build + tests**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::context_manager 2>&1 | tail -10
# Expect: existing tests pass (use baseline_system_prompt internally)
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(agent/context_manager): for_prompt_with_injection accepting InjectionContext

Adds for_prompt_with_injection(query, injection_ctx) which calls A4's
render_with_context for the baseline (vs render_all). Existing
for_prompt becomes a thin shim calling with InjectionContext::baseline().

Body extracted to private for_prompt_inner taking system_prompt as
arg to avoid duplication between the two public methods.

Spec: docs/superpowers/specs/2026-05-25-dirac-b2-context-manager-wireup-design.md"
```

---

## Task 2: Add `ContextManager` to `ChatDelegate`

- [ ] **Step 2.1: Add new fields**

In `dispatcher.rs::ChatDelegate` struct (~line 92):

```rust
use std::sync::atomic::AtomicBool;
use crate::agent::context_manager::{ContextManager, ComposeStats};
use crate::runtime::context::ContextArtifact;

pub struct ChatDelegate {
    // ... existing fields ...

    /// Per-session context orchestrator (M2-B wire-up).
    context_manager: Arc<tokio::sync::RwLock<ContextManager>>,
    /// Most-recent ComposeStats; read by get_compose_stats Tauri command.
    last_compose_stats: Arc<Mutex<ComposeStats>>,
    /// Fragments selected on the most recent for_prompt_with_injection call;
    /// injected into build_dynamic_context.
    last_injected_fragments: Arc<Mutex<Vec<ContextArtifact>>>,
    /// True for the first ACT-mode turn, false after. Reset to true on
    /// mode toggle back to ACT.
    is_first_act_turn: AtomicBool,
    /// Set on tool execution error; consumed by next effective_system_prompt
    /// call's InjectionContext, then cleared.
    last_error_kind: Mutex<Option<String>>,
}
```

- [ ] **Step 2.2: Initialize in constructor**

In `ChatDelegate::new` (or equivalent):

```rust
context_manager: Arc::new(tokio::sync::RwLock::new(ContextManager::new())),
last_compose_stats: Arc::new(Mutex::new(ComposeStats::default())),
last_injected_fragments: Arc::new(Mutex::new(Vec::new())),
is_first_act_turn: AtomicBool::new(true),
last_error_kind: Mutex::new(None),
```

- [ ] **Step 2.3: Update `is_first_act_turn` toggling**

Find where ACT mode transitions happen (`taskState.didSwitchToActMode`
equivalent in uClaw — likely in `dispatcher.rs` post-LLM-call or in
`agentic_loop.rs`). On first ACT turn execution, after building the
system prompt, set `is_first_act_turn` to `false`. On mode toggle
*back* to ACT from Plan, set to `true`.

If the exact toggle site isn't obvious from grep, leave a `TODO(M2-A)`
comment + always-`false` after first call:

```rust
// Set false unconditionally after first call; M2-A finalization PR
// wires the proper mode-transition tracking.
self.is_first_act_turn.store(false, Ordering::Relaxed);
```

This isn't ideal but keeps B2 small. Full toggle correctness lands
with M2-A finalization.

- [ ] **Step 2.4: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

No tests yet — keep this commit narrow.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(dispatcher): ChatDelegate holds ContextManager + injection state

Adds 5 new fields:
- context_manager: Arc<RwLock<ContextManager>> (M2-B wire-up)
- last_compose_stats: Arc<Mutex<ComposeStats>>
- last_injected_fragments: Arc<Mutex<Vec<ContextArtifact>>>
- is_first_act_turn: AtomicBool (A4 InjectionContext input)
- last_error_kind: Mutex<Option<String>> (A4 InjectionContext input)

No behavior change yet — fields are populated but not yet read by
effective_system_prompt. Next commit wires them through.

TODO(M2-A): proper is_first_act_turn toggle on mode transition.
Currently set false after first read."
```

---

## Task 3: Rewrite `effective_system_prompt` to route through `ContextManager`

- [ ] **Step 3.1: Add sync wrapper for `for_prompt_with_injection`**

In `dispatcher.rs::ChatDelegate`:

```rust
fn context_manager_for_prompt_blocking(
    &self,
    query: &ComposeQuery,
    injection_ctx: &InjectionContext,
) -> ComposedContext {
    let cm = self.context_manager.clone();
    let q = query.clone();
    let ic = injection_ctx.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            let cm_read = cm.read().await;
            cm_read.for_prompt_with_injection(&q, &ic).await
        })
    })
}

fn estimate_context_pressure_ratio(&self) -> f32 {
    // Use existing token-budget calc if available, else 0.0.
    // M2-J snapshot should already expose this — wire if so.
    0.0
}
```

- [ ] **Step 3.2: Rewrite `effective_system_prompt`**

Replace the existing body (~line 640):

```rust
fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String {
    let is_first_act = self.is_first_act_turn.load(Ordering::Relaxed);
    let last_err = self.last_error_kind.lock().unwrap().clone();
    let pressure = self.estimate_context_pressure_ratio();

    let inj_ctx = crate::agent::baseline_blocks::InjectionContext {
        is_first_act_turn: is_first_act,
        last_error_kind: last_err,
        context_pressure_ratio: pressure,
    };

    let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);

    let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);

    // Persist stats + fragments for build_dynamic_context + Tauri snapshot
    {
        let mut s = self.last_compose_stats.lock().unwrap();
        *s = composed.stats.clone();
    }
    {
        let mut f = self.last_injected_fragments.lock().unwrap();
        *f = composed.injected_fragments.clone();
    }

    // First-act flag transitions to false after this read
    if is_first_act {
        self.is_first_act_turn.store(false, Ordering::Relaxed);
    }

    // Compose final prompt: ContextManager baseline + mode addition + skills manifest (if not suppressed)
    let mode_addition = crate::agent::mode_prompts::mode_addition_for(effective_mode);
    let suppress_manifest = self.skill_search_used.load(Ordering::Relaxed);

    if self.skills_manifest_block.is_empty() || suppress_manifest {
        format!("{}\n{}", composed.system_prompt, mode_addition)
    } else {
        format!("{}\n{}{}", composed.system_prompt, mode_addition, self.skills_manifest_block)
    }
}
```

> **Important**: `mode_prompts::mode_addition_for` may not exist as a
> separate function today — `compose_system_prompt` does both baseline
> + mode addition inline. **Refactor `mode_prompts`** to expose
> `mode_addition_for(mode) -> String` as a separate function. Spec
> §3.2 notes this; it's a small extraction.

- [ ] **Step 3.3: Add `mode_addition_for` to `mode_prompts.rs`**

```rust
// In src-tauri/src/agent/mode_prompts.rs
pub fn mode_addition_for(mode: &SafetyMode) -> &'static str {
    match mode {
        SafetyMode::Ask => MODE_ASK,
        SafetyMode::AcceptEdits => MODE_ACCEPT_EDITS,
        SafetyMode::Plan => MODE_PLAN,
        SafetyMode::Supervised => "",  // no addition
        SafetyMode::Yolo => MODE_BYPASS,
    }
}
```

(Adapt the variant names to match the actual `SafetyMode` enum.)

- [ ] **Step 3.4: Build + run smoke**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
```

Existing dispatcher tests should pass. If a test asserts on system
prompt content byte-equality, it may break — adjust assertion (the
new path produces *semantically* same prompt for `InjectionContext::baseline()`
but registry rendering joins blocks with `\n\n` which may differ from
the current `include_str!` baseline.md formatting by a whitespace).
Document any test adjustment.

**Commit checkpoint:**
```
git add -A
git commit -m "feat(dispatcher): route effective_system_prompt through ContextManager

effective_system_prompt now:
1. Builds InjectionContext from current task state (is_first_act_turn,
   last_error_kind, context_pressure_ratio)
2. Calls context_manager.for_prompt_with_injection(query, inj_ctx)
3. Persists composed.stats and composed.injected_fragments on
   ChatDelegate for build_dynamic_context + Tauri snapshot consumption
4. Composes final prompt: composed.system_prompt + mode_addition_for(mode)
   + skills_manifest_block (if not suppressed)

is_first_act_turn flag transitions to false after first read.

Adds mode_addition_for(mode) → &'static str extracted from
compose_system_prompt for the new composition path.

Cache discipline preserved: turns 2-N have byte-stable system prompt
(InjectionContext::baseline-equivalent after turn 1). Turn 1 has
FirstActTurnOnly blocks; one-time cache miss accepted per A4 spec §8.6."
```

---

## Task 4: Inject fragments into `build_dynamic_context`

- [ ] **Step 4.1: Extend `build_dynamic_context`**

In `dispatcher.rs::build_dynamic_context` (~line 675), after the
existing time + workspace + memory blocks:

```rust
fn build_dynamic_context(&self, messages: &[ChatMessage]) -> String {
    let mut block = String::new();
    // ... existing time + workspace block construction ...

    // ... existing memory_context block ...

    // B2: ContextManager-selected fragments
    let fragments = self.last_injected_fragments.lock().unwrap().clone();
    for art in &fragments {
        block.push_str(&format!(
            "\n<context_fragment id=\"{}\">\n{}\n</context_fragment>",
            // Adapt field names to actual ContextArtifact shape per Step 0.4
            art.ref_id, art.content
        ));
    }

    block
}
```

- [ ] **Step 4.2: Build + run**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(dispatcher): inject ContextManager fragments into build_dynamic_context

Fragments selected by ContextManager.for_prompt_with_injection on the
prior effective_system_prompt call are now rendered as
<context_fragment id=\"...\">...</context_fragment> blocks inside the
per-turn dynamic context block.

Per-turn placement (not system prompt) preserves cache_control:ephemeral
hits on the system prompt across turns."
```

---

## Task 5: Wrap `ContextToolSet::search` + `read` as builtin tools

- [ ] **Step 5.1: Create `context_tools_adapter.rs`**

```rust
// src-tauri/src/agent/tools/builtin/context_tools_adapter.rs
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::runtime::context_tools::ContextToolSet;

pub struct ContextSearchTool {
    toolset: Arc<RwLock<ContextToolSet>>,
}
impl ContextSearchTool {
    pub fn new(toolset: Arc<RwLock<ContextToolSet>>) -> Self { Self { toolset } }
}

#[async_trait]
impl Tool for ContextSearchTool {
    fn name(&self) -> &str { "context.search" }
    fn description(&self) -> &str {
        "Search available context fragments by topic tags. Returns matching ContextRef identifiers. Use context.read to fetch a specific ref's content. Use when you need supporting context (prior conversation, related files, memory recall) but want to avoid preloading everything."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "topics": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Topic tags to search (e.g., 'auth', 'database', 'frontend'). Multiple topics OR-combined."
                }
            },
            "required": ["topics"]
        })
    }
    fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let topics: Vec<String> = serde_json::from_value(params["topics"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("topics: {e}")))?;
        let ts = self.toolset.read().await;
        let refs = ts.search(&topics);
        let out = serde_json::to_string_pretty(&refs).unwrap();
        Ok(ToolOutput::success(&out, start.elapsed().as_millis() as u64))
    }
}

pub struct ContextReadTool {
    toolset: Arc<RwLock<ContextToolSet>>,
}
impl ContextReadTool {
    pub fn new(toolset: Arc<RwLock<ContextToolSet>>) -> Self { Self { toolset } }
}

#[async_trait]
impl Tool for ContextReadTool {
    fn name(&self) -> &str { "context.read" }
    fn description(&self) -> &str {
        "Materialize a context fragment by its ContextRef id (from context.search). Returns the fragment's content as a structured artifact."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "string",
                    "description": "ContextRef id from a prior context.search call"
                }
            },
            "required": ["ref"]
        })
    }
    fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }
    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let ref_id = params["ref"].as_str()
            .ok_or_else(|| ToolError::InvalidParams("ref is required".into()))?;
        let ts = self.toolset.read().await;
        let artifact = ts.read(ref_id).await
            .map_err(|e| ToolError::Other(e.to_string()))?;
        let out = serde_json::to_string_pretty(&artifact).unwrap();
        Ok(ToolOutput::success(&out, start.elapsed().as_millis() as u64))
    }
}
```

> Adapt method names (`ts.search`, `ts.read`) and field names
> (`art.ref_id`, `art.content`) per Step 0.4 discovery.

- [ ] **Step 5.2: Wire into `tools/builtin/mod.rs`**

```rust
pub mod context_tools_adapter;
pub use context_tools_adapter::{ContextSearchTool, ContextReadTool};
```

- [ ] **Step 5.3: Register at the tool registration site (Step 0.5)**

At the location where `EditTool::new(...)` and `ReadFileTool::new(...)`
are registered with the ToolRegistry:

```rust
let context_toolset = Arc::new(RwLock::new(ContextToolSet::new()));
registry.register(Box::new(ContextSearchTool::new(context_toolset.clone())));
registry.register(Box::new(ContextReadTool::new(context_toolset.clone())));
```

The `context_toolset` instance must be shared with the
`ChatDelegate::context_manager` such that fragments added to one are
visible to the other. If the architecture is "ChatDelegate owns
ContextManager which owns fragments" vs "ContextToolSet is separate" —
that's a pre-existing design tension. **For B2, both share the same
underlying fragment set**: either inject the same `Arc<Vec<...>>` into
both, or have one wrap the other. Pick the option closest to current
struct shapes; document in commit.

- [ ] **Step 5.4: Build + run**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
cd src-tauri && cargo test --lib agent::tools::builtin::context 2>&1 | tail -10
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tools/builtin/context): ContextSearchTool + ContextReadTool wrap ContextToolSet

Adds two builtin tools wrapping the M2-F ContextToolSet:

- context.search { topics: string[] } → ContextRef[]
- context.read { ref: string } → ContextArtifact

Registered alongside EditTool / ReadFileTool. Both use
ApprovalRequirement::Never — they're read-only context queries.

Only the 2 working ops are wrapped — fold/cite/compare/pin/release
remain Err(unimplemented) stubs in ContextToolSet and are NOT
exposed as tools (would confuse the LLM with stub failures).
Future M2-G/D PRs will implement + register them."
```

---

## Task 6: `get_compose_stats` Tauri command

- [ ] **Step 6.1: Add command in tauri_commands.rs**

```rust
#[tauri::command]
pub fn get_compose_stats(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<crate::agent::context_manager::ComposeStats, String> {
    // Adapt session lookup to the actual SessionManager API
    let sm = state.session_manager.read()
        .map_err(|e| format!("session lock: {e}"))?;
    let session = sm.get(&session_id)
        .ok_or_else(|| format!("session {session_id} not found"))?;
    let delegate = session.chat_delegate.read()
        .map_err(|e| format!("delegate lock: {e}"))?;
    let stats = delegate.last_compose_stats.lock().unwrap().clone();
    Ok(stats)
}
```

> Adapt `state.session_manager` / `session.chat_delegate` to the
> actual `AppState` shape per `app.rs`.

- [ ] **Step 6.2: Make `ComposeStats` Serialize for Tauri**

If `ComposeStats` doesn't already implement `serde::Serialize`, add:

```rust
// in context_manager/manager.rs
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ComposeStats { /* unchanged fields */ }
```

- [ ] **Step 6.3: Register in invoke_handler!**

In `main.rs::invoke_handler!` macro list:

```rust
tauri::generate_handler![
    // ... existing commands ...
    crate::tauri_commands::get_compose_stats,
    // ... rest ...
]
```

- [ ] **Step 6.4: Build**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head
```

**Commit checkpoint:**
```
git add -A
git commit -m "feat(tauri_commands): get_compose_stats command for M2-J UI consumption

Adds get_compose_stats(session_id) → ComposeStats Tauri command.
Reads ChatDelegate.last_compose_stats from the named session.

ComposeStats now derives serde::Serialize so it can cross the Tauri
boundary.

Frontend M2-J PR will consume this. B2 ships backend only."
```

---

## Task 7: Seven unit tests + one integration test

- [ ] **Step 7.1: `chat_delegate_builds_with_empty_context_manager`**

In `dispatcher.rs::tests`:

```rust
#[tokio::test]
async fn chat_delegate_builds_with_empty_context_manager() {
    let delegate = ChatDelegate::new(/* minimal fixture args */);
    let cm = delegate.context_manager.read().await;
    assert_eq!(cm.fragment_count(), 0);
}
```

- [ ] **Step 7.2: `effective_system_prompt_uses_injection_context_first_turn`**

Construct a `ChatDelegate`, register a test block with policy
`FirstActTurnOnly` (via `BaselineBlockRegistry::new_for_test` from A4),
call `effective_system_prompt(&SafetyMode::Supervised)`. Assert: output
contains the FirstActTurnOnly block content.

> If the production `BaselineBlockRegistry` is a singleton, this test
> may need a `with_test_registry` scope. A4 plan §3 Step 0.3 covered
> the test-registry constructor — verify it's present and usable.

- [ ] **Step 7.3: `effective_system_prompt_excludes_first_turn_blocks_after_first_turn`**

Same setup as 7.2. Call `effective_system_prompt` twice. Assert:
- First call output contains the FirstActTurnOnly block
- Second call output does NOT contain it
- `is_first_act_turn` is now `false`

- [ ] **Step 7.4: `compose_stats_populated_after_effective_system_prompt`**

Register 3 fragments in the ContextManager. Call
`effective_system_prompt`. Read `last_compose_stats`. Assert:
`fragments_available == 3`.

- [ ] **Step 7.5: `context_search_tool_returns_matching_refs`**

```rust
#[tokio::test]
async fn context_search_tool_returns_matching_refs() {
    let toolset = Arc::new(RwLock::new(ContextToolSet::new()));
    {
        let mut ts = toolset.write().await;
        ts.add(/* fragment with topic "rust" */);
        ts.add(/* fragment with topic "rust" */);
        ts.add(/* fragment with topic "frontend" */);
    }
    let tool = ContextSearchTool::new(toolset);
    let out = tool.execute(json!({"topics": ["rust"]})).await.unwrap();
    let refs: Vec<ContextRef> = serde_json::from_str(&out.output_text()).unwrap();
    assert_eq!(refs.len(), 2);
}
```

- [ ] **Step 7.6: `context_read_tool_returns_fragment_artifact`**

Similar fixture; call `context.read` with a known ref id; assert
returned artifact content matches expected.

- [ ] **Step 7.7: `get_compose_stats_command_round_trip`**

```rust
#[tokio::test]
async fn get_compose_stats_command_round_trip() {
    let app_state = build_test_app_state(/* ... */);
    let session_id = "test".to_string();
    // ... set up session with delegate that has populated stats ...
    let stats = get_compose_stats(session_id, tauri::State::from(&app_state)).unwrap();
    assert!(stats.fragments_available > 0);
}
```

If Tauri test harness is awkward for B2 scope, alternate: assert the
function compiles + signature is correct via `#[tauri::command]`
expansion. Real round-trip can land in a follow-up E2E test.

- [ ] **Step 7.8: Integration test — `tests/context_wireup_bench.rs`**

```rust
// src-tauri/tests/context_wireup_bench.rs
use uclaw::agent::context_manager::ContextManager;
// ... etc ...

#[tokio::test]
async fn fragments_selected_grows_over_long_session() {
    // Set up a fixture session with 5 registered fragments
    // Simulate 20 turns
    // After each turn, capture last_compose_stats.fragments_selected
    // Assert: at least 10 of the 20 turns had fragments_selected > 0
    //         (proves wire-up active, not all-zeros)
}
```

- [ ] **Step 7.9: Run all tests**

```bash
cd src-tauri && cargo test --lib agent::dispatcher 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::context_manager 2>&1 | tail -10
cd src-tauri && cargo test --lib agent::tools::builtin::context 2>&1 | tail -10
cd src-tauri && cargo test --test context_wireup_bench 2>&1 | tail -10
```

**Commit checkpoint:**
```
git add -A
git commit -m "test(dispatcher + context_manager + tools/builtin/context): 7 unit + 1 integration test

Unit (7):
- ChatDelegate builds with empty ContextManager
- effective_system_prompt includes FirstActTurnOnly block on turn 1
- effective_system_prompt excludes FirstActTurnOnly block from turn 2+
- last_compose_stats populated after effective_system_prompt call
- context.search tool returns refs matching topic filter
- context.read tool returns artifact for known ref
- get_compose_stats Tauri command round-trips ComposeStats

Integration (1):
- 20-turn fixture session: fragments_selected > 0 on at least
  half the turns — proves wire-up is live"
```

---

## Task 8: SSoT + PR

- [ ] **Step 8.1: Update MILESTONE_STATUS**

Under §M2 — Context Fabric:

```
| C2-Dirac-B2 | ContextManager wire-up + context.search/read tools + get_compose_stats command | #<PR-number> |
```

Update §M2 percentage from ~58% (post-A) to ~75% (post-B2 — major
closeout). Status changes: M2-B from "🟡 pilot, wire-up missing"
to "✅ wired".

- [ ] **Step 8.2: Drift + push + PR**

```bash
./scripts/milestone-drift-check.sh --since "1 week ago" 2>&1 | tail -10
git push -u origin claude/dirac-b2-context-manager-wireup

gh pr create \
  --title "[C2-Dirac-B2] feat(dispatcher + context): wire ContextManager into prompt + register context.* tools" \
  --body "..."
```

PR description includes:
- Summary
- Why (link research doc §7.2 B2 + ADR §2 "context as a tool")
- Commits (bisectable) — 7 commits
- Verification — cargo test outputs, integration bench output, cache discipline confirmation
- Spec link
- Closes (C2-Dirac-B2; closes M2-B; partially closes M2-F)
- **Depends on**: C1-Dirac-A4 merged (uses InjectionContext)

- [ ] **Step 8.3: Self-merge gate**

- [ ] CI green
- [ ] PR tag `[C2-Dirac-B2]`
- [ ] Cache-discipline test (or manual rollout check) passes — turns
  2-N have byte-stable system prompt
- [ ] Integration bench shows fragments_selected > 0 over the run
- [ ] MILESTONE_STATUS M2-B status updated to "✅ wired"

---

## Rollback procedure

```bash
git revert <merge-commit-sha>
git push
```

Restored state: ChatDelegate without ContextManager. effective_system_prompt
back to direct mode_prompts path. context.* tools de-registered.
get_compose_stats command gone. ComposeStats no longer Serialize (but
still derives Default/Clone, so internal use is unaffected). No data
corruption.

---

## Closes / unblocks

- C2-Dirac-B2 ✓
- Closes M2-B (ContextManager wire-up)
- Partially closes M2-F (2/7 context tools wired)
- Major M2 progress jump (~58% → ~75%)
- Unblocks M2-D (fragment lifecycle work)
- Unblocks M2-J full UI (backend Tauri command now exists)
- Pairs with B1 — B1 + B2 together = M2 closeout, prepares M3
  Capability Mesh

---

## Task A (autonomous mode only) — Self-verify + adversarial review + auto-merge

> Run only when invoked by the autonomous orchestrator (see
> [`docs/superpowers/protocols/autonomous-execution-protocol.md`](../protocols/autonomous-execution-protocol.md)).
>
> ⚠️ **C1 close + A4 + B1 dependency gate**: orchestrator MUST have
> verified C1 closed + A4 merged + B1 merged before this task starts.

- [ ] **Step A.1: Stage 2 self-verify (per protocol §3.2)**

```bash
cd src-tauri
cargo build 2>&1 | grep -E "^error" | head
cargo test --lib agent::dispatcher 2>&1 | tail -10
cargo test --lib agent::context_manager 2>&1 | tail -10
cargo test --lib agent::tools::builtin 2>&1 | tail -10
cargo test --test context_wireup_bench 2>&1 | tail -10
cargo clippy --lib -- -D warnings 2>&1 | tail -5

# Scope:
git diff --name-only main..HEAD | sort
# Expected:
#   src-tauri/src/agent/context_manager/manager.rs
#   src-tauri/src/agent/dispatcher.rs
#   src-tauri/src/agent/mode_prompts.rs                  (extract mode_addition_for)
#   src-tauri/src/agent/tools/builtin/context_tools_adapter.rs  (NEW)
#   src-tauri/src/agent/tools/builtin/mod.rs             (pub mod context_tools_adapter)
#   src-tauri/src/main.rs                                (register + invoke_handler)
#   src-tauri/src/tauri_commands.rs                      (get_compose_stats)
#   src-tauri/tests/context_wireup_bench.rs              (NEW)
#   docs/superpowers/MILESTONE_STATUS.md

# Cache discipline check — no stub tools registered
grep -rn "context\.fold\|context\.cite\|context\.compare\|context\.pin\|context\.release" src-tauri/src/ | grep -v "_test\|//"
# Expected: empty (stubs MUST NOT be wired)

# Fragments inject into dynamic context, NOT system prompt
grep -n "context_fragment" src-tauri/src/agent/dispatcher.rs
# Expected hits inside build_dynamic_context, NOT inside effective_system_prompt
```

- [ ] **Step A.2: Spawn adversarial reviewer (protocol §3.3)**

B2-specific CRITICAL focus from spec §12:
- Cache discipline — `effective_system_prompt` output should be
  byte-stable across consecutive `is_first_act_turn=false` calls
- `block_in_place` pattern OR documented channel fallback
- No stub tools registered
- Integration test asserts `fragments_selected > 0` over the run
- MILESTONE_STATUS edits change M2-B status to "wired"

- [ ] **Step A.3: Reconcile per protocol §3.4**

- [ ] **Step A.4: PR open + CI + auto-merge (protocol §3.5)**

```bash
git push -u origin claude/dirac-b2-context-manager-wireup
PR=$(gh pr create --title "[C2-Dirac-B2] feat(dispatcher + context): wire ContextManager into prompt + register context.* tools" --body-file ./pr-body.md --json number -q .number)
gh pr checks $PR --watch --interval 30 --required
gh pr merge $PR --merge --delete-branch
git checkout main && git pull
```

- [ ] **Step A.5: Log + return outcome (protocol §7)**

- [ ] **Step A.6: After B2 merge — write Phase B closeout report**

After B2 merges, orchestrator triggers the Phase B closeout step:

1. Pull main
2. Run drift check
3. Generate `docs/superpowers/specs/2026-05-25-phase-b-closeout.md`
   per the prompt in `docs/research/2026-05-25-dirac-phase-a-prompts.md` §"After Phase B closes"
4. Open + auto-merge a `[C2-Closeout]` PR
5. Update MILESTONE_STATUS.md: M2 percentage to ~75%, M2-B to closed,
   M2-F to partially-closed, optionally mark C2 closed if no other
   C2 work remains
6. Return final sequence-level outcome to user
