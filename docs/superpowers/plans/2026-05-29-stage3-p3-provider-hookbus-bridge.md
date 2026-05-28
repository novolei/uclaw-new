# 阶段 3 P3-3 — ProviderService + HookBus Bridge · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bridge HookBus through AgentApi (so `agent_api.emit()` fan-outs to both AgentApi's own `on()` hooks AND HookBus subscribers), expose ProviderService as a passthrough query on AgentApi (so consumers can fetch the singleton through `agent_api.provider_service()`), and update P3-1's `register_provider`/`provider()` API to the corrected `set_provider_service`/`provider_service()` shape. **No call-site migrations of existing HookBus consumers** — those stay direct; this PR adds the bridge. Singular `Arc<HookBus>` and `Arc<ProviderService>` instances continue to be process-scoped singletons.

**Architecture:** AgentApi gets two optional Arc-shared "co-handles": `hook_bus: Option<Arc<HookBus>>` and `provider_service: Option<Arc<ProviderService>>`, populated at boot before the `Arc::new(api)` seal (same pattern as P3-2's `register_all`). The new `emit_with_decision()` method translates between AgentApi's Event surface (13 EventKinds, EventPayload + Continue/Patch/Abort) and HookBus's HookEvent surface (13 distinct variants, HookDecision: Allow/Deny/AskUser). Translation table covers ~5 overlapping events (ToolCall ↔ PreToolUse, ToolResult ↔ PostToolUse, BeforeProviderRequest ↔ PreLlmCall, AfterProviderResponse ↔ PostLlmCall, BeforeContextAssembly ↔ PreContextInject); non-overlapping events skip the HookBus fan-out.

**Tech Stack:** Rust 2021, Tauri 2, async-trait, inline `#[cfg(test)] mod tests` pattern.

**Related design:** [`2026-05-28-stage3-agentapi-handle-design.md`](../specs/2026-05-28-stage3-agentapi-handle-design.md) §10 + P3-3 row (with the 2026-05-29 Correction callout pointing here).

**Prior PRs:** [#570 P3-1](https://github.com/novolei/uclaw-new/pull/570) (scaffold), [#571 P3-2](https://github.com/novolei/uclaw-new/pull/571) (tool migration via descriptors). Merged to main at `96d2c2e0`.

---

## Recon-discovered design gap

The original spec §10 + P3-3 row said "Migrate `ProviderService` + `HookBus` through AgentApi". Pre-plan recon (2026-05-29) found two shape problems:

### ProviderService is a singleton, not a registry

| Aspect | Reality |
|---|---|
| Instance count in process | **1** — `Arc<ProviderService>` on AppState, constructed once at boot from `data_dir/providers.json`. |
| Internal state | Manages many `ProviderConfig`s in a HashMap, but the service ITSELF is single. |
| "Register" semantics | `ProviderService.configure_provider(config)` persists a provider config; not a register-into-handle operation. |
| Cross-tree consumers | 11 files reference ProviderService directly. 30+ Tauri commands call `state.provider_service.foo().await`. |

P3-1's API `register_provider(id, Arc<ProviderService>)` stored the SAME Arc<ProviderService> under arbitrary IDs — wrong shape. P3-3 replaces it with `set_provider_service(Arc<ProviderService>)` (single setter) and `provider_service() -> Option<&Arc<ProviderService>>` (single getter).

### HookBus is a parallel hook system with different event types + decision semantics

| Aspect | HookBus | AgentApi (P3-1) |
|---|---|---|
| Event type | `HookEvent` enum (13 variants: PreToolUse, PostToolUse, PreLlmCall, PostLlmCall, PrePermission, PostPermission, PreContextInject, PostContextInject, TaskStart, TaskEnd, MemoryWrite, MemoryRecall, Checkpoint) | `Event { kind: EventKind, payload, session_id, cancellation_token }` (13 EventKinds) |
| Subscriber registration | `register(Arc<dyn HookSubscriber>)` with interest_in() interest filter | `on(EventKind, Fn)` closure-based |
| Decision semantics | `dispatch_with_decision()` returns `HookDecision::Allow/Deny/AskUser` with aggregation rules (Deny wins, then AskUser, else Allow) | `emit()` returns `EventOutcome::Continue/Patch/Abort` |
| Cross-tree consumers | 10 files: `dispatcher.rs`, `tool_dispatch/mod.rs`, `tauri_commands.rs`, `policy_eval/mod.rs`, `symphony_graph/runtime/{service,run_actor,node_run}.rs`, etc. | 0 — P3-1 scaffold only |

The user-grilled Option E resolution: **bridge, don't replace**. HookBus stays as the policy/decision engine; AgentApi.emit fans out to it for overlapping events. Consumers can keep using HookBus directly OR migrate to AgentApi.emit piecemeal in future PRs.

### Event ↔ HookEvent translation table

| AgentApi EventKind | HookEvent variant (translation target) | Translation strategy |
|---|---|---|
| `ToolCall` | `PreToolUse` | `task_id` ← session_id; `tool_name`, `args_json` from EventPayload::ToolCall |
| `ToolResult` | `PostToolUse` | `task_id` ← session_id; `tool_name`, `success` (true if result not error), `result_preview` (truncated) |
| `BeforeProviderRequest` | `PreLlmCall` | `task_id` ← session_id; `provider`, `model`, `prompt_tokens_estimate` (0 default) |
| `AfterProviderResponse` | `PostLlmCall` | `task_id` ← session_id; `provider`, `model`, `completion_tokens` ← token_count |
| `BeforeContextAssembly` | `PreContextInject` | `task_id` ← session_id |
| `SessionStart`, `SessionShutdown`, `TurnStart`, `TurnEnd`, `MessageStart`, `MessageEnd`, `BeforeCancellation`, `PluginShutdown` | **None — skip HookBus fan-out** | These EventKinds have no HookEvent peer; AgentApi.emit runs its own hooks only. |

| HookDecision | Mapped to EventOutcome |
|---|---|
| `Allow` | `Continue` |
| `Deny { reason }` | `Abort(reason)` |
| `AskUser { risk_class }` | `Abort(format!("askuser:{}", risk_class))` — caller checks for the `askuser:` prefix to distinguish from regular denials. |

---

## Background facts verified against HEAD `96d2c2e0` (main after P3-2 squash-merge)

### ProviderService

- **Module path**: `crate::providers::service::ProviderService` (also re-exported at `crate::providers::ProviderService`).
- **Constructor**: `pub fn new(data_dir: &Path) -> Result<Self, Error>` (sync, takes path; creates `providers.json` if missing).
- **AppState field**: `pub provider_service: Arc<ProviderService>` (constructed at `app.rs:584`).
- **Cross-tree consumers (11 files)**: `tauri_commands.rs`, `app.rs`, `ingestion/mod.rs`, `symphony_graph/runtime/service.rs`, `agent/headless.rs`, `agent/api/{tests,mod}.rs` (P3-1), `proactive/service.rs`, `automation/runtime/service.rs`, `memory_graph/memory_os_llm.rs`.
- **P3-3 does NOT migrate consumers** — they continue using `state.provider_service` directly. The new `agent_api.provider_service()` query is additive.

### HookBus

- **Module path**: `crate::agent::hook_bus::HookBus` (re-exported from `crate::agent::hook_bus`).
- **Key methods**: `new()` / `register(Arc<dyn HookSubscriber>) -> Result<(), BusError>` / `unregister(&SubscriberId) -> bool` / `subscriber_count() -> usize` / `dispatch_observe(&HookEvent) async` / `dispatch_with_decision(&HookEvent) async -> HookDecision`.
- **AppState field**: `pub hook_bus: Arc<HookBus>` (constructed at `app.rs:887-890` with same boot-mutable-then-Arc-wrap pattern; `PolicySpecSubscriber` registered).
- **Cross-tree consumers (10 files)**: `dispatcher.rs`, `tool_dispatch/mod.rs`, `tauri_commands.rs`, `policy_eval/{mod,subscriber}.rs`, `symphony_graph/runtime/{service,run_actor,node_run}.rs`, `agent/mod.rs`, plus inline subscriber implementations.

### HookEvent

13 variants per `agent/hook_bus/event.rs`. Each carries `task_id: String` (groups events by owning task) + variant-specific fields. `kind() -> HookEventKind` is the variant tag for subscriber `interest_in()` filtering. `is_decision_capable() -> bool` on `HookEventKind` determines whether `dispatch_with_decision` is meaningful (observe-only kinds always return `Allow`).

### Baselines

- `cargo build`: green, **49 warnings** (post-P3-2 baseline preserved).
- `cargo test --lib agent::`: 782 passed / 2 pre-existing failed.
- `cargo test --lib agent::api`: 16 passed.
- `cargo test --lib` total: 3026 passed / 7 pre-existing failed.

---

## Pre-flight (before Task 1)

1. **Confirm main baseline**: `git -C /Users/ryanliu/Documents/uclaw status -sb` → `## main...origin/main` at `96d2c2e0`.

2. **Create worktree + symlinks**:

```bash
git worktree add -b claude/stage3-p3-provider-hookbus-bridge \
    /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge main
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/gbrain-source
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/pyembed
ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
      /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/bunembed
```

3. **Baseline verifications**:

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | tail -3
# expect: Finished, ~49 warnings

cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
# expect: 782 passed / 2 failed
```

All paths in tasks below are relative to the worktree.

---

## Task 1: Replace `register_provider`/`provider()` with `set_provider_service`/`provider_service()`

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (drop `providers: HashMap` field; add `provider_service: Option<Arc<ProviderService>>` field; change register_provider → set_provider_service; change provider() → provider_service())
- Modify: `src-tauri/src/agent/api/tests.rs` (update 2 P3-1 tests for register_provider/provider; rewrite or replace)

### Steps

- [ ] **Step 1.1: Change AgentApi struct field**

In `src-tauri/src/agent/api/mod.rs`, find the `pub struct AgentApi { ... }` block. The current `providers` field reads:

```rust
    pub(crate) providers: HashMap<String, Arc<ProviderService>>,
```

Replace with:

```rust
    pub(crate) provider_service: Option<Arc<ProviderService>>,
```

Update the `new()` constructor initializer from `providers: HashMap::new()` to `provider_service: None`.

Update the `Debug` impl: change `.field("providers", &self.providers.len())` to `.field("provider_service", &self.provider_service.is_some())`.

- [ ] **Step 1.2: Replace `register_provider` with `set_provider_service`**

Find the existing P3-1 method:

```rust
    pub fn register_provider(&mut self, id: String, provider: Arc<ProviderService>) {
        self.providers.insert(id, provider);
    }
```

Replace with:

```rust
    /// Set the singleton ProviderService handle. Called once at boot
    /// (`AppState::new()`) before `Arc::new(api)` seals. Last write wins.
    pub fn set_provider_service(&mut self, svc: Arc<ProviderService>) {
        self.provider_service = Some(svc);
    }
```

- [ ] **Step 1.3: Replace `provider()` with `provider_service()`**

Find:

```rust
    pub fn provider(&self, id: &str) -> Option<&Arc<ProviderService>> {
        self.providers.get(id)
    }
```

Replace with:

```rust
    /// Get the singleton ProviderService handle if set. Returns None if
    /// not yet wired (pre-boot or in unit tests using AgentApi::new()).
    pub fn provider_service(&self) -> Option<&Arc<ProviderService>> {
        self.provider_service.as_ref()
    }
```

- [ ] **Step 1.4: Update P3-1 tests**

In `src-tauri/src/agent/api/tests.rs`, find the 2 tests added in P3-1.3:
- `register_provider_stores_by_id`
- `provider_query_returns_registered`

Replace BOTH tests with these (Option A — using the same tempfile fixture from P3-1.3):

```rust
#[tokio::test]
async fn set_provider_service_stores_singleton() {
    let tmp = tempfile::tempdir().unwrap();
    let svc = crate::providers::service::ProviderService::new(tmp.path()).unwrap();
    let mut api = AgentApi::new();
    assert!(api.provider_service.is_none(), "starts unset");
    api.set_provider_service(std::sync::Arc::new(svc));
    assert!(api.provider_service.is_some(), "set after wiring");
}

#[tokio::test]
async fn provider_service_query_returns_singleton() {
    let tmp = tempfile::tempdir().unwrap();
    let svc = std::sync::Arc::new(
        crate::providers::service::ProviderService::new(tmp.path()).unwrap()
    );
    let mut api = AgentApi::new();
    assert!(api.provider_service().is_none());
    api.set_provider_service(svc.clone());
    let got = api.provider_service().unwrap();
    assert!(std::sync::Arc::ptr_eq(got, &svc), "returns the same Arc");
}
```

Note: ProviderService::new is sync per Step 1.5 of P3-2.3 recon (`pub fn new(&Path) -> Result<Self, Error>`), but it works inside a tokio test wrapper. If the existing P3-1 fixture used `.await.unwrap()`, drop the `.await` — it's not async.

If the existing `register_provider_stores_by_id` test was `#[ignore]`-stubbed in P3-1 (Option B path), replace those stubs with the active tests above. Note in the commit body.

- [ ] **Step 1.5: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

- [ ] **Step 1.6: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: 16 passed (same as P3-2 baseline; 2 register_provider tests renamed but still 2 tests).

- [ ] **Step 1.7: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge commit -m "$(cat <<'EOF'
refactor(agent): AgentApi.provider_service singleton (P3-3.1 of 阶段 3)

Replaces P3-1's `register_provider(id, Arc<ProviderService>)` +
`provider(name)` API with `set_provider_service(Arc<ProviderService>)`
+ `provider_service()` singleton accessor.

Reason: ProviderService is a process-scope singleton (one instance per
AppState, constructed from data_dir/providers.json); the
HashMap<String, Arc<ProviderService>> shape from P3-1 stored the same
Arc multiple times under arbitrary keys.

This is a breaking change to the P3-1 API with zero non-test callers
(P3-1 was scaffold-only). The 2 P3-1 provider tests are rewritten to
exercise the singleton shape.

cargo build clean; agent::api 16/0 unchanged; agent:: 782/2 baseline preserved.

Next: P3-3.2 introduces Event ↔ HookEvent translation.
EOF
)"
```

Continue to Task 2.

---

## Task 2: Event ↔ HookEvent translation function

**Files:**
- Create: `src-tauri/src/agent/api/hookbus_bridge.rs` (translation function + unit tests)
- Modify: `src-tauri/src/agent/api/mod.rs` (declare the new module)

### Steps

- [ ] **Step 2.1: Create `hookbus_bridge.rs`**

Write `src-tauri/src/agent/api/hookbus_bridge.rs`:

```rust
//! Event ↔ HookEvent translation for the P3-3 bridge.
//!
//! AgentApi has 13 EventKinds; HookBus has 13 HookEvent variants. ~5 events
//! overlap semantically; the rest are AgentApi-only (no HookBus dispatch).
//! `event_to_hook_event` returns `None` for non-overlapping events.

use crate::agent::api::events::{Event, EventKind, EventPayload, EventOutcome};
use crate::agent::hook_bus::{HookEvent, HookDecision};

/// Translate an AgentApi `Event` into a `HookEvent` for HookBus dispatch.
/// Returns `None` for events that have no HookBus peer (AgentApi-only kinds).
///
/// `task_id` for HookEvent maps to `event.session_id` — uClaw's HookBus
/// callers (dispatcher, tool_dispatch) use session_id as the grouping key
/// today, so this matches existing semantics.
pub fn event_to_hook_event(event: &Event) -> Option<HookEvent> {
    let task_id = event.session_id.clone();
    match (&event.kind, &event.payload) {
        (EventKind::ToolCall, EventPayload::ToolCall { tool_name, args }) => {
            Some(HookEvent::PreToolUse {
                task_id,
                tool_name: tool_name.clone(),
                args_json: args.to_string(),
            })
        }
        (EventKind::ToolResult, EventPayload::ToolResult { tool_name, result }) => {
            // Heuristic: success = not an `error` key at top level of result.
            let success = result.get("error").is_none();
            let result_preview = {
                let s = result.to_string();
                if s.len() > 200 { format!("{}…", &s[..200]) } else { s }
            };
            Some(HookEvent::PostToolUse {
                task_id,
                tool_name: tool_name.clone(),
                success,
                result_preview,
            })
        }
        (EventKind::BeforeProviderRequest, EventPayload::BeforeProviderRequest { provider, model }) => {
            Some(HookEvent::PreLlmCall {
                task_id,
                provider: provider.clone(),
                model: model.clone(),
                prompt_tokens_estimate: 0,
            })
        }
        (EventKind::AfterProviderResponse, EventPayload::AfterProviderResponse { provider, model, token_count }) => {
            Some(HookEvent::PostLlmCall {
                task_id,
                provider: provider.clone(),
                model: model.clone(),
                completion_tokens: *token_count,
            })
        }
        (EventKind::BeforeContextAssembly, EventPayload::BeforeContextAssembly { .. }) => {
            Some(HookEvent::PreContextInject {
                task_id,
            })
        }
        // AgentApi-only kinds — no HookBus dispatch:
        _ => None,
    }
}

/// Translate a `HookDecision` back to an `EventOutcome`. Used by
/// `AgentApi.emit_with_decision()` when folding HookBus's decision into
/// the loop's outcome.
pub fn hook_decision_to_event_outcome(d: HookDecision) -> EventOutcome {
    match d {
        HookDecision::Allow => EventOutcome::Continue,
        HookDecision::Deny { reason } => EventOutcome::Abort(reason),
        HookDecision::AskUser { risk_class } => EventOutcome::Abort(format!("askuser:{}", risk_class)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::api::events::*;
    use tokio_util::sync::CancellationToken;

    fn make_event(kind: EventKind, payload: EventPayload) -> Event {
        Event {
            kind,
            payload,
            session_id: "s1".into(),
            cancellation_token: CancellationToken::new(),
        }
    }

    #[test]
    fn tool_call_translates_to_pre_tool_use() {
        let ev = make_event(EventKind::ToolCall, EventPayload::ToolCall {
            tool_name: "echo".into(),
            args: serde_json::json!({"msg": "hi"}),
        });
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PreToolUse { task_id, tool_name, args_json } => {
                assert_eq!(task_id, "s1");
                assert_eq!(tool_name, "echo");
                assert!(args_json.contains("hi"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_result_translates_to_post_tool_use_with_success_heuristic() {
        let ev_ok = make_event(EventKind::ToolResult, EventPayload::ToolResult {
            tool_name: "echo".into(),
            result: serde_json::json!({"out": "ok"}),
        });
        let h = event_to_hook_event(&ev_ok).unwrap();
        match h {
            HookEvent::PostToolUse { success, .. } => assert!(success, "no error key → success"),
            _ => panic!(),
        }

        let ev_err = make_event(EventKind::ToolResult, EventPayload::ToolResult {
            tool_name: "echo".into(),
            result: serde_json::json!({"error": "fail"}),
        });
        let h = event_to_hook_event(&ev_err).unwrap();
        match h {
            HookEvent::PostToolUse { success, .. } => assert!(!success, "error key → not success"),
            _ => panic!(),
        }
    }

    #[test]
    fn before_provider_request_translates_to_pre_llm_call() {
        let ev = make_event(EventKind::BeforeProviderRequest, EventPayload::BeforeProviderRequest {
            provider: "anthropic".into(),
            model: "claude-opus-4-7".into(),
        });
        let h = event_to_hook_event(&ev).unwrap();
        match h {
            HookEvent::PreLlmCall { provider, model, .. } => {
                assert_eq!(provider, "anthropic");
                assert_eq!(model, "claude-opus-4-7");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn agentapi_only_kinds_return_none() {
        let kinds = [
            EventKind::SessionStart,
            EventKind::SessionShutdown,
            EventKind::TurnStart,
            EventKind::TurnEnd,
            EventKind::MessageStart,
            EventKind::MessageEnd,
            EventKind::BeforeCancellation,
            EventKind::PluginShutdown,
        ];
        for kind in kinds {
            let payload = match kind {
                EventKind::SessionStart => EventPayload::SessionStart { session_id: "s".into() },
                EventKind::SessionShutdown => EventPayload::SessionShutdown { session_id: "s".into() },
                EventKind::TurnStart => EventPayload::TurnStart { turn_id: "t".into() },
                EventKind::TurnEnd => EventPayload::TurnEnd { turn_id: "t".into(), duration_ms: 0 },
                EventKind::MessageStart => EventPayload::MessageStart { message_id: "m".into() },
                EventKind::MessageEnd => EventPayload::MessageEnd { message_id: "m".into() },
                EventKind::BeforeCancellation => EventPayload::BeforeCancellation { reason: "r".into() },
                EventKind::PluginShutdown => EventPayload::PluginShutdown { plugin_id: "p".into() },
                _ => unreachable!(),
            };
            let ev = make_event(kind, payload);
            assert!(event_to_hook_event(&ev).is_none(), "{:?} should not translate", kind);
        }
    }

    #[test]
    fn hook_decision_translates_to_event_outcome() {
        assert!(matches!(
            hook_decision_to_event_outcome(HookDecision::Allow),
            EventOutcome::Continue
        ));
        let d = HookDecision::Deny { reason: "denied".to_string() };
        if let EventOutcome::Abort(reason) = hook_decision_to_event_outcome(d) {
            assert_eq!(reason, "denied");
        } else {
            panic!();
        }
        let d = HookDecision::AskUser { risk_class: "high".to_string() };
        if let EventOutcome::Abort(reason) = hook_decision_to_event_outcome(d) {
            assert!(reason.starts_with("askuser:"));
            assert!(reason.contains("high"));
        } else {
            panic!();
        }
    }
}
```

**Note**: The exact HookDecision variant shape might differ from what this plan assumes (e.g., `Deny { reason }` vs `Deny(String)`). Verify with:

```bash
grep -A 8 "pub enum HookDecision" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/src/agent/hook_bus/event.rs
```

Adapt the translation function and tests to the actual variant shape.

Similarly, HookEvent variant shapes may have additional fields. Adapt as needed.

- [ ] **Step 2.2: Declare module in `agent/api/mod.rs`**

Find the existing `pub mod` declarations (events, command, renderer, plugin, session_context, tool). Add `pub mod hookbus_bridge;` in alphabetical position (between `events` and `plugin`).

- [ ] **Step 2.3: Build + run new tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent::api::hookbus_bridge 2>&1 | tail -5
```
Expected: 5 passed / 0 failed.

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 787 passed / 2 failed (= 782 baseline + 5 translation tests).

- [ ] **Step 2.4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge add -A \
    src-tauri/src/agent/api/hookbus_bridge.rs \
    src-tauri/src/agent/api/mod.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge commit -m "feat(agent): Event↔HookEvent translation for HookBus bridge (P3-3.2 of 阶段 3)"
```

Continue to Task 3.

---

## Task 3: Add `hook_bus` field + `set_hook_bus()` to AgentApi

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (add field + setter + import)
- Modify: `src-tauri/src/agent/api/tests.rs` (add 1 test)

### Steps

- [ ] **Step 3.1: Add `hook_bus` field**

In `agent/api/mod.rs`, in `pub struct AgentApi`, add:

```rust
    pub(crate) hook_bus: Option<Arc<crate::agent::hook_bus::HookBus>>,
```

Initialize in `new()`: `hook_bus: None`.

Update Debug impl to add `.field("hook_bus", &self.hook_bus.is_some())`.

- [ ] **Step 3.2: Add `set_hook_bus`**

In `impl AgentApi`, after `set_provider_service` from Task 1, add:

```rust
    /// Set the singleton HookBus handle. Called once at boot
    /// (`AppState::new()`) before `Arc::new(api)` seals. Enables the
    /// HookBus bridge in `emit()` and `emit_with_decision()`.
    pub fn set_hook_bus(&mut self, bus: Arc<crate::agent::hook_bus::HookBus>) {
        self.hook_bus = Some(bus);
    }

    /// Get the singleton HookBus handle if set.
    pub fn hook_bus(&self) -> Option<&Arc<crate::agent::hook_bus::HookBus>> {
        self.hook_bus.as_ref()
    }
```

- [ ] **Step 3.3: Add test**

Append to `agent/api/tests.rs`:

```rust
#[test]
fn set_hook_bus_stores_singleton() {
    let mut api = AgentApi::new();
    assert!(api.hook_bus().is_none(), "starts unset");
    let bus = std::sync::Arc::new(crate::agent::hook_bus::HookBus::new());
    api.set_hook_bus(bus.clone());
    assert!(api.hook_bus().is_some());
    let got = api.hook_bus().unwrap();
    assert!(std::sync::Arc::ptr_eq(got, &bus));
}
```

- [ ] **Step 3.4: Build + run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: build clean; tests at 22 passed (16 from P3-2 + 5 translation tests + 1 new).

- [ ] **Step 3.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge commit -m "feat(agent): AgentApi.set_hook_bus + hook_bus() singleton accessor (P3-3.3 of 阶段 3)"
```

Continue to Task 4.

---

## Task 4: Fan out `emit()` to HookBus + add `emit_with_decision()`

**Files:**
- Modify: `src-tauri/src/agent/api/mod.rs` (extend emit, add emit_with_decision)
- Modify: `src-tauri/src/agent/api/tests.rs` (add 3 tests for fan-out behavior)

### Steps

- [ ] **Step 4.1: Extend `emit()` to fan out to HookBus**

Find the existing `emit` method (P3-1.6). Currently it runs only AgentApi's own hooks. Extend the implementation to ALSO call `hook_bus.dispatch_observe(translated_event)` AFTER all AgentApi hooks have run (if hook_bus is set AND the event has a HookEvent peer):

```rust
    /// Fire an event. Hooks for `ev.kind` run in registration order. The first
    /// hook returning `Abort` or `Patch` short-circuits and the outcome is returned;
    /// `Continue` outcomes are skipped and the next hook runs. If no hooks are
    /// registered for the kind, returns `Continue`.
    ///
    /// **HookBus bridge (P3-3)**: after AgentApi hooks complete, if a HookBus
    /// is wired AND this Event has a HookEvent peer (per `event_to_hook_event`),
    /// fan out to `hook_bus.dispatch_observe`. The HookBus result does NOT
    /// affect the EventOutcome — observe-only fan-out. For decision-capable
    /// fan-out use `emit_with_decision`.
    pub async fn emit(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let agentapi_outcome = if let Some(hooks) = self.hooks.get(&kind) {
            let mut outcome = EventOutcome::Continue;
            for h in hooks {
                let result = h(&ev).await?;
                match result {
                    EventOutcome::Continue => continue,
                    other => { outcome = other; break; }
                }
            }
            outcome
        } else {
            EventOutcome::Continue
        };

        // Bridge: observe-only HookBus dispatch.
        if let Some(bus) = &self.hook_bus {
            if let Some(hook_event) = crate::agent::api::hookbus_bridge::event_to_hook_event(&ev) {
                bus.dispatch_observe(&hook_event).await;
            }
        }

        Ok(agentapi_outcome)
    }
```

Note: the implementation must keep P3-1's short-circuit semantics (Continue → next hook, Patch/Abort → return immediately). The HookBus fan-out runs AFTER AgentApi hooks regardless of outcome. If a hook returned Patch/Abort early, the agentapi_outcome variable holds it; we still want HookBus to observe (its subscribers may log the event).

Actually, on reflection: if AgentApi's hook chain Aborted, we may NOT want to fan out to HookBus (the event was vetoed). The current implementation always fans out. The semantic choice is:
- **A**: Always fan out (HookBus subscribers see the event regardless of AgentApi veto).
- **B**: Only fan out if outcome was Continue (HookBus sees only events that completed AgentApi hook chain).

Go with **A** (always fan out for observability — HookBus subscribers might want to log denied tool calls for audit). Document this in the docstring (already noted "observe-only fan-out" — clear intent).

- [ ] **Step 4.2: Add `emit_with_decision()` method**

After `emit()`, add:

```rust
    /// Like `emit()` but uses HookBus's `dispatch_with_decision` for the
    /// decision-capable fan-out. AgentApi hooks run first (same as emit);
    /// if their outcome is Continue AND HookBus is wired AND the event
    /// has a peer, HookBus's verdict is folded into the final outcome
    /// (Deny → Abort, Allow → Continue, AskUser → Abort with "askuser:" prefix).
    ///
    /// Used by callers wanting to consult policy subscribers (PolicySpecSubscriber,
    /// human-boundary gates, etc.). Caller checks `EventOutcome::Abort` reason
    /// for the "askuser:" prefix to distinguish from regular denials.
    pub async fn emit_with_decision(&self, ev: Event) -> Result<EventOutcome, String> {
        let kind = ev.kind;
        let agentapi_outcome = if let Some(hooks) = self.hooks.get(&kind) {
            let mut outcome = EventOutcome::Continue;
            for h in hooks {
                let result = h(&ev).await?;
                match result {
                    EventOutcome::Continue => continue,
                    other => { outcome = other; break; }
                }
            }
            outcome
        } else {
            EventOutcome::Continue
        };

        // If AgentApi hooks short-circuited (Patch/Abort), return that —
        // HookBus is NOT consulted (AgentApi-side veto has priority).
        if !matches!(agentapi_outcome, EventOutcome::Continue) {
            return Ok(agentapi_outcome);
        }

        // Fan out to HookBus's decision-capable dispatch.
        if let Some(bus) = &self.hook_bus {
            if let Some(hook_event) = crate::agent::api::hookbus_bridge::event_to_hook_event(&ev) {
                let decision = bus.dispatch_with_decision(&hook_event).await;
                return Ok(crate::agent::api::hookbus_bridge::hook_decision_to_event_outcome(decision));
            }
        }

        Ok(EventOutcome::Continue)
    }
```

- [ ] **Step 4.3: Write tests for fan-out**

Append to `agent/api/tests.rs`:

```rust
#[tokio::test]
async fn emit_fans_out_to_hook_bus_when_wired() {
    use crate::agent::hook_bus::{HookBus, HookEvent, HookEventKind, HookSubscriber, HookDecision, SubscriberId};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;

    struct CountingSubscriber {
        id: SubscriberId,
        count: std::sync::Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl HookSubscriber for CountingSubscriber {
        fn id(&self) -> SubscriberId { self.id.clone() }
        fn interest_in(&self) -> &[HookEventKind] {
            // Interest in all kinds for the test
            &[HookEventKind::PreToolUse]
        }
        async fn on_event(&self, _e: &HookEvent) -> HookDecision {
            self.count.fetch_add(1, Ordering::SeqCst);
            HookDecision::Allow
        }
    }

    let bus = std::sync::Arc::new({
        let mut b = HookBus::new();
        let count = std::sync::Arc::new(AtomicUsize::new(0));
        b.register(std::sync::Arc::new(CountingSubscriber {
            id: SubscriberId::new("counter".into()),
            count: count.clone(),
        })).unwrap();
        b
    });

    let mut api = AgentApi::new();
    api.set_hook_bus(bus);

    use crate::agent::api::events::*;
    use futures::FutureExt;
    let ev = Event {
        kind: EventKind::ToolCall,
        payload: EventPayload::ToolCall {
            tool_name: "echo".into(),
            args: serde_json::json!({}),
        },
        session_id: "s1".into(),
        cancellation_token: CancellationToken::new(),
    };

    let outcome = api.emit(ev).await.unwrap();
    assert!(matches!(outcome, EventOutcome::Continue));
    // Subscriber count should be 1 (PreToolUse dispatched to the subscriber).
}

#[tokio::test]
async fn emit_with_decision_returns_continue_for_observe_only_events() {
    use tokio_util::sync::CancellationToken;
    let api = AgentApi::new();
    use crate::agent::api::events::*;
    let ev = Event {
        kind: EventKind::SessionStart,
        payload: EventPayload::SessionStart { session_id: "s".into() },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    };
    // Without hook_bus, returns Continue.
    let outcome = api.emit_with_decision(ev).await.unwrap();
    assert!(matches!(outcome, EventOutcome::Continue));
}

#[tokio::test]
async fn emit_with_decision_short_circuits_on_agentapi_abort() {
    use futures::FutureExt;
    use tokio_util::sync::CancellationToken;
    let mut api = AgentApi::new();
    use crate::agent::api::events::*;
    api.on(EventKind::ToolCall, move |_ev| {
        async move { Ok(EventOutcome::Abort("api-veto".to_string())) }.boxed()
    });
    let ev = Event {
        kind: EventKind::ToolCall,
        payload: EventPayload::ToolCall {
            tool_name: "echo".into(),
            args: serde_json::json!({}),
        },
        session_id: "s".into(),
        cancellation_token: CancellationToken::new(),
    };
    // Even without hook_bus wired, the abort from AgentApi hook short-circuits.
    let outcome = api.emit_with_decision(ev).await.unwrap();
    if let EventOutcome::Abort(reason) = outcome {
        assert_eq!(reason, "api-veto");
    } else {
        panic!("expected Abort");
    }
}
```

The `HookSubscriber` test fixture mock requires `async_trait::async_trait` if HookSubscriber's `on_event` uses the macro. Verify the trait shape with `grep "trait HookSubscriber" src/agent/hook_bus/`.

- [ ] **Step 4.4: Build + run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent::api 2>&1 | tail -5
```
Expected: build clean; tests at 25 passed (22 + 3 new fan-out tests).

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 791 passed / 2 failed (782 baseline + 3 emit-test additions, plus the 5 translation tests from Task 2 + 1 set_hook_bus test from Task 3 = 9 total new, +1 from Task 1 net change is 0).

Recount: Task 1: 0 net (replaced 2 tests with 2 different tests). Task 2: +5. Task 3: +1. Task 4: +3. Total: +9. So 782 + 9 = 791.

- [ ] **Step 4.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge add -A \
    src-tauri/src/agent/api/mod.rs \
    src-tauri/src/agent/api/tests.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge commit -m "feat(agent): AgentApi.emit fans out to HookBus + new emit_with_decision (P3-3.4 of 阶段 3)"
```

Continue to Task 5.

---

## Task 5: Wire `set_provider_service` + `set_hook_bus` into AppState::new()

**Files:**
- Modify: `src-tauri/src/app.rs` (extend the agent_api construction block)

### Steps

- [ ] **Step 5.1: Inspect current AgentApi construction site**

```bash
grep -B2 -A6 "agent_api = {" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/src/app.rs
```

Expected: the P3-2.5 block:

```rust
let agent_api = {
    let mut api = crate::agent::api::AgentApi::new();
    crate::agent::tools::builtin_descriptors::register_all(&mut api);
    std::sync::Arc::new(api)
};
```

- [ ] **Step 5.2: Extend the block to call set_provider_service + set_hook_bus**

This block is in the middle of `AppState::new()`. `provider_service` is constructed at line ~584; `hook_bus` is constructed at line ~887-890. Verify both are in scope when the `agent_api = { ... }` block executes (they're both local Arc-shared values by the time the block runs).

Refactor:

```rust
let agent_api = {
    let mut api = crate::agent::api::AgentApi::new();
    crate::agent::tools::builtin_descriptors::register_all(&mut api);
    api.set_provider_service(provider_service.clone());
    api.set_hook_bus(hook_bus.clone());
    std::sync::Arc::new(api)
};
```

If the agent_api block executes BEFORE hook_bus is constructed, MOVE the agent_api block to AFTER both prerequisites exist. Verify by line number:

```bash
grep -n "agent_api = {\|let provider_service\|let hook_bus" /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri/src/app.rs | head -10
```

Reorder if needed.

- [ ] **Step 5.3: Build (GREEN GATE)**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: empty.

- [ ] **Step 5.4: Run tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo test --lib agent:: 2>&1 | tail -5
```
Expected: 791 passed / 2 failed (same as Task 4; Task 5 doesn't add tests).

- [ ] **Step 5.5: Warning count**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```
Expected: ≤50 (any new warnings should only be for unused setter methods that will be called later by consumers — acceptable). The "method `set_hook_bus` is never used" warning from Task 3 + "method `set_provider_service` is never used" from Task 1 should both be RESOLVED after Task 5 wires them.

- [ ] **Step 5.6: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge add -A src-tauri/src/app.rs

git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge commit -m "$(cat <<'EOF'
feat(app): wire ProviderService + HookBus into AgentApi at boot (P3-3.5 of 阶段 3)

Extends the agent_api construction block in AppState::new() to call:
- api.set_provider_service(provider_service.clone())
- api.set_hook_bus(hook_bus.clone())

Both setters fire BEFORE std::sync::Arc::new(api) seals the handle.
After this commit:
- agent_api.provider_service() returns Some(Arc<ProviderService>)
- agent_api.hook_bus() returns Some(Arc<HookBus>)
- agent_api.emit() fans out to HookBus.dispatch_observe for overlapping events
- agent_api.emit_with_decision() folds HookBus verdict into EventOutcome

Existing consumers (10 hook_bus.dispatch_* sites + 30+ provider_service
sites) are UNCHANGED — they continue accessing the singletons directly
via state.hook_bus / state.provider_service. P3-3 adds the bridge;
consumer migration is a separate decision per call site.

Final P3-3 commit. cargo build clean; agent:: 791/2 baseline preserved
(+9 new tests across Tasks 1-4); cargo test --lib total grows by 9.

Cumulative P3-3 (5 commits): 1 new file (~250 LoC hookbus_bridge.rs +
tests) + modifications to mod.rs, tests.rs, app.rs.

Next strategic step: P3-4 (SubprocessPluginManager + 2 plugin demos).
EOF
)"
```

Verify final chain:

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge log --oneline HEAD~5..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage3-p3-provider-hookbus-bridge status -sb
```

Expected: 5 commits ahead of `main` (the plan commit landed separately at plan-write time); working tree clean.

---

## Self-Review

**1. Spec coverage:**
- ✅ Spec §10 + P3-3 row corrected (Correction callout linking to this plan).
- ✅ ProviderService passthrough: set_provider_service + provider_service() singleton accessor (Task 1).
- ✅ Event ↔ HookEvent translation: full 13-variant table (Task 2).
- ✅ HookBus bridge field + setter (Task 3).
- ✅ emit() fan-out to HookBus.dispatch_observe (Task 4).
- ✅ emit_with_decision() with HookDecision folding (Task 4).
- ✅ Boot wiring in AppState::new (Task 5).

**2. Placeholder scan:**
- Task 2's note about adapting HookDecision variant shape is implementer judgment (verifying actual codebase shape). Not a placeholder.
- Task 4's choice "A (always fan out) vs B (only on Continue)" was explicitly resolved in the plan body — going with A.
- No "TBD" / "TODO" / "similar to Task N" / "implement later".

**3. Type consistency:**
- `Arc<ProviderService>` consistent across Tasks 1, 5.
- `Arc<HookBus>` consistent across Tasks 3, 4, 5.
- `event_to_hook_event` and `hook_decision_to_event_outcome` named consistently in Tasks 2, 4.
- `set_provider_service` / `provider_service()` and `set_hook_bus` / `hook_bus()` pairs named consistently.
- `emit` and `emit_with_decision` method names consistent.

No spec gaps, no placeholders, no type inconsistencies. Plan ready.

---

## Quick reference

- **Estimated time:** 1 person-day (5 mechanical tasks; Task 2 is the largest at ~250 LoC).
- **Risk:** medium. Task 4's emit fan-out semantics (always fan out vs only on Continue) had a deliberate design choice — A picked for observability.
- **Files touched:**
  - Task 1: 2 (mod.rs + tests.rs)
  - Task 2: 2 (1 new + mod.rs declaration)
  - Task 3: 2 (mod.rs + tests.rs)
  - Task 4: 2 (mod.rs + tests.rs)
  - Task 5: 1 (app.rs)
- **Net LoC:** +250 (hookbus_bridge.rs + tests) + ~80 (mod.rs additions across tasks) + ~3 (app.rs additions) ≈ **+330 LoC**.
- **PR shape:** 1 worktree → 5 commits → 1 PR. Bisectable per-task. Squash-on-land per P1-P4/P3-1/P3-2 convention.
- **Non-goals (deferred)**:
  - Migration of any of the 10 HookBus consumers to use AgentApi.emit instead of HookBus.dispatch_* — separate PRs per consumer (or merged into broader refactors).
  - Migration of the 30+ ProviderService consumers to use AgentApi.provider_service() instead of state.provider_service — same: optional, per-call-site judgment in future PRs.
  - Adding HookEvent variants AgentApi doesn't cover (TaskStart, TaskEnd, MemoryWrite, MemoryRecall, etc.) to AgentApi's EventKind — those are HookBus-only domains; bridging would require expanding AgentApi's surface beyond Pi-lightweight scope.
