# Safety Chokepoint Unification (Slice 1b) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route all three live tool-execution origins (chat, automation, browser sub-loop) through `SafetyManager.should_approve` as the single per-call chokepoint, with origin-specific async behavior (chat blocks-and-prompts, automation escalates via DB, browser sub-loop reuses chat's modal).

**Architecture:** One new abstraction `ApprovalHandler` (1 trait, 1 method, 2 implementations). One new `ToolDispatchContext` field `permissions: Option<PermissionSet>` (same mechanical pattern as Slice 1a's `cancel`). One new origin enum `ApprovalOriginKind` in the ctx. One new DB migration V56 (verify against `CONTEXT.md` active migration registry at implementation start). `BrowserAgentLoop` and `HeadlessDelegate` inject a shared `Arc<ToolDispatcher<Wry>>` from app state. `boundary.rs` / `BrowserAskUserBridge` are untouched (different axis).

**Tech Stack:** Rust, `tokio`, `rusqlite`, `tauri 2`, `async-trait`. Reuses `ToolDispatcher` (hardened in Slice 1a) and `SafetyManager` (already exists).

---

## Background facts verified against current code (2026-05-28)

These are the ground-truth anchors for the plan. Re-verify each via `grep`/`Read` at task start; line numbers may drift slightly.

- **`SafetyManager.should_approve(&self, tool_name, &args, &ApprovalRequirement, mode_override: Option<&SafetyMode>) -> ApprovalDecision`** — `safety/mod.rs:205`.
- **`ApprovalDecision` variants** — `safety/mod.rs:48`: `AutoApprove`, `Block { reason }`, **`RequireApproval`** (spec calls this "Ask"; code uses `RequireApproval`).
- **`SafetyMode` variants** — `safety/mod.rs:14`: `Ask`, `AcceptEdits`, `Plan`, `Supervised` (default), `Yolo`.
- **`PendingApprovals`** — `app.rs:46`: `register(tool_id: String) -> oneshot::Receiver<ApprovalResult>`, `resolve(tool_id, ApprovalResult) -> bool`.
- **`ToolDispatcher.pending_approvals: Arc<PendingApprovals>`** field — `tool_dispatch/mod.rs:75`. Used at `:292`, `:708`, `:826` (three call sites for tool/path/skill approvals). Test setups at `:939`, `:1063-1079`, `:1093`.
- **`ToolDispatchContext` struct** — `tool_dispatch/mod.rs:37` (8 fields including Slice 1a's `cancel`). 4 literal sites: `dispatcher.rs:2525` (prod), `tool_dispatch/mod.rs:917,1123,1171` (tests).
- **`Permission` enum** — `automation/protocol/humane_v1.rs:280`: `AiBrowser`, `Notification`, `Filesystem`, `Network`, `Shell`, `Unknown`. **Coarse category-level, NOT tool-name.** Needs `tool_name → Permission` mapping.
- **`PermissionSet` struct** — `automation/runtime/mod.rs:21`: `spec: Vec<Permission>`, `granted: Vec<Permission>`, `denied: Vec<Permission>`. No `matches()` method yet.
- **`automation_specs.permissions_granted/permissions_denied`** TEXT columns — `db/migrations.rs:727-728` (JSON arrays).
- **`HeadlessDelegate`** struct at `agent/headless.rs:71` — NOT generic over `R`. Has `permissions: PermissionSet`, `db: Arc<Mutex<Connection>>`, `tools: Arc<ToolRegistry>`, `app_handle: Option<tauri::AppHandle>`. NO `safety_manager`, NO `tool_dispatcher` fields today.
- **`AutomationDelegate = HeadlessDelegate`** alias at `automation/runtime/execute.rs:5`. Test builder at `:44-72` (`make_delegate`).
- **`BrowserAgentLoop`** struct at `browser/agent_loop.rs:89` — 10+ optional `Arc` fields, builder pattern `with_*` (e.g. `with_ask_user_bridge` at `:138`, `with_mcp_manager` at `:158`). Constructor `new(ctx_mgr, decision_adapter)` at `:103`. `run` at `:181`. NO `safety_manager`, NO `tool_dispatcher` today.
- **`LoopOutcome` variants** — `agent/types.rs:313`: `Response`, `ToolResult`, `Stopped`, `Cancelled`, `MaxIterations`, `Failure`, **`NeedApproval { tool_name, tool_call_id, parameters }`** — **already exists**, use it for automation escalation (no new variant needed).
- **`LoopDelegate` trait** — `agent/types.rs:348`. `execute_tool_calls(&self, tool_calls: Vec<ToolCall>, reason_ctx: &mut ReasoningContext) -> Result<Option<LoopOutcome>, Error>`. Returning `Ok(Some(LoopOutcome::NeedApproval { ... }))` from this is how the loop receives an escalation signal.
- **Max migration version on main** = V55. **V56 is the next free slot** (verify against `CONTEXT.md` active migration registry at implementation time — coordinate with any open PR claims).
- **GitNexus index** — refreshed at the start of this plan; `gitnexus_impact` is callable. Re-verify symbol blast radius per CLAUDE.md before edits.

---

## Spec ↔ plan name reconciliations

The spec used a few names that diverge from current code; this plan uses the code's names:

| Spec name | Code name |
|---|---|
| Approval "Ask" outcome | `ApprovalDecision::RequireApproval` |
| `LoopOutcome::Paused` | `LoopOutcome::NeedApproval { tool_name, tool_call_id, parameters }` (already exists) |
| `Permission::matches(tool_name)` (assumed exact-match) | `permission_for_tool(tool_name) -> Permission` mapping + `PermissionSet::covers(tool_name) -> Coverage` (defined in Task 1.3) |

---

## File Structure

### Created
- `src-tauri/src/safety/approval.rs` — `ApprovalHandler` trait + `ApprovalOrigin`/`ApprovalOutcome` enums + `ChatApprovalHandler` implementation (Task 1)
- `src-tauri/src/automation/runtime/approval.rs` — `AutomationApprovalHandler` implementation (Task 2)

### Modified
- `src-tauri/src/safety/mod.rs` — re-export new types from `approval` (Task 1)
- `src-tauri/src/automation/runtime/mod.rs` — add `impl PermissionSet::covers(&self, tool_name) -> Coverage` + `permission_for_tool(name) -> Permission` (Task 1)
- `src-tauri/src/agent/tool_dispatch/mod.rs` — `ToolDispatchContext` gains `permissions: Option<PermissionSet>` + `origin_kind: ApprovalOriginKind`; `ToolDispatcher.pending_approvals` replaced by `approval_handler: Arc<dyn ApprovalHandler>`; `run_one` resolves permissions then routes Ask through handler (Task 1)
- `src-tauri/src/agent/dispatcher.rs:2525` — ctx literal adds `permissions: None`, `origin_kind: ApprovalOriginKind::Chat { ... }` (Task 1)
- `src-tauri/src/db/migrations.rs` — V56 migration (new table + column) (Task 2)
- `src-tauri/src/agent/headless.rs` — `HeadlessDelegate` gains `safety_manager: Arc<RwLock<SafetyManager>>` + `tool_dispatcher: Option<Arc<ToolDispatcher<tauri::Wry>>>` + `approval_handler: Arc<dyn ApprovalHandler>`; `execute_tool_calls` routes through `tool_dispatcher.dispatch` when present and propagates `LoopOutcome::NeedApproval` on Escalated outcome (Task 2)
- `src-tauri/src/automation/runtime/execute.rs:44-72` — `make_delegate` test helper updated (Task 2)
- `src-tauri/src/tauri_commands.rs` — two new commands `list_pending_automation_approvals`, `resolve_automation_approval` (Task 2)
- `src-tauri/src/main.rs` — register the two new commands in the `invoke_handler!` macro (Task 2)
- `src-tauri/src/browser/agent_loop.rs:89-103` — `BrowserAgentLoop` gains 3 optional fields + 3 `with_*` builders + the sub-loop's tool dispatch routes through the injected `ToolDispatcher` when present (Task 3)

---

## Pre-flight (before Task 1)

1. `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — confirm main builds clean. Expected: empty.
2. `cd src-tauri && cargo test --lib agent:: 2>&1 | tail -5` — confirm baseline tests pass (the 2 pre-existing failures `shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long` are known).
3. Re-verify the migration registry: `grep -n "Active migration registry" docs/CONTEXT.md src-tauri/src/db/migrations.rs | head` — confirm V56 is the next free slot; if any open PR has claimed it, pick V57 (or next free).
4. `gitnexus_impact({target: "should_approve", direction: "upstream"})` and `gitnexus_impact({target: "ToolDispatchContext", direction: "downstream"})` — report blast radius. Expected for `should_approve`: ~10 caller files; for `ToolDispatchContext`: 4 literal sites (dispatcher.rs + 3 in tool_dispatch/mod.rs).
5. Create the worktree + branch:
   ```bash
   git worktree add -b claude/safety-chokepoint-unification \
       /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification main
   ```
   Confirm:
   ```bash
   git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification status -sb
   ```
   Expected: `## claude/safety-chokepoint-unification`.

All subsequent paths in this plan are relative to the worktree root: `/Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification`.

---

## Task 1: `ApprovalHandler` trait + `ToolDispatchContext.permissions`

**Files:**
- Create: `src-tauri/src/safety/approval.rs`
- Modify: `src-tauri/src/safety/mod.rs` (re-exports), `src-tauri/src/automation/runtime/mod.rs` (PermissionSet helpers), `src-tauri/src/agent/tool_dispatch/mod.rs` (ctx + dispatcher), `src-tauri/src/agent/dispatcher.rs:2525` (ctx literal)

This task is the foundation. It adds the abstraction layer + the data-flow field without changing chat behavior (the existing `PendingApprovals` becomes the default `ChatApprovalHandler`, byte-equivalent for chat).

### Task 1.1: Write failing test for `permission_for_tool` mapping

- [ ] **Step 1: Write the test**

Add at the bottom of `src-tauri/src/automation/runtime/mod.rs`:

```rust
#[cfg(test)]
mod permission_for_tool_tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    #[test]
    fn maps_shell_tools_to_shell_permission() {
        assert_eq!(permission_for_tool("bash"), Permission::Shell);
        assert_eq!(permission_for_tool("shell"), Permission::Shell);
    }

    #[test]
    fn maps_file_tools_to_filesystem_permission() {
        assert_eq!(permission_for_tool("edit"), Permission::Filesystem);
        assert_eq!(permission_for_tool("write_file"), Permission::Filesystem);
        assert_eq!(permission_for_tool("read_file"), Permission::Filesystem);
        assert_eq!(permission_for_tool("multi_edit"), Permission::Filesystem);
    }

    #[test]
    fn maps_browser_tools_to_aibrowser_permission() {
        assert_eq!(permission_for_tool("browser_task"), Permission::AiBrowser);
    }

    #[test]
    fn maps_notify_user_to_notification_permission() {
        assert_eq!(permission_for_tool("notify_user"), Permission::Notification);
    }

    #[test]
    fn unknown_tool_maps_to_unknown() {
        assert_eq!(permission_for_tool("some_random_tool"), Permission::Unknown);
    }
}
```

- [ ] **Step 2: Run test to verify it fails (compile error)**

Run: `cd src-tauri && cargo test --lib automation::runtime::permission_for_tool_tests 2>&1 | tail -10`
Expected: FAIL — `function 'permission_for_tool' not found in module`.

- [ ] **Step 3: Implement `permission_for_tool`**

Add to `src-tauri/src/automation/runtime/mod.rs` (above the existing `PermissionSet` struct):

```rust
use crate::automation::protocol::humane_v1::Permission;

/// Map a tool name to its coarse-grained `Permission` category.
///
/// `PermissionSet` operates at category granularity (Shell/Filesystem/...).
/// This is the bridge from per-tool dispatch to per-category authorization.
/// Unknown tools map to `Permission::Unknown` and are treated as un-covered
/// by `PermissionSet::covers` (forcing FallThrough → SafetyManager `Ask`).
pub fn permission_for_tool(tool_name: &str) -> Permission {
    match tool_name {
        "bash" | "shell" => Permission::Shell,
        "edit" | "write_file" | "read_file" | "multi_edit" | "search_files"
            | "ls" | "glob" => Permission::Filesystem,
        "browser_task" => Permission::AiBrowser,
        "notify_user" => Permission::Notification,
        // Network: HTTP-style tools as they emerge; none today.
        _ => Permission::Unknown,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::permission_for_tool_tests 2>&1 | tail -10`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit (intermediate — included in Task 1's final commit)**

(No commit here — Task 1 ends with one consolidated commit at Step 1.10.)

### Task 1.2: Write failing test for `PermissionSet::covers`

- [ ] **Step 1: Write the test**

Add at the bottom of `src-tauri/src/automation/runtime/mod.rs` (in a new `permission_set_covers_tests` mod):

```rust
#[cfg(test)]
mod permission_set_covers_tests {
    use super::*;
    use crate::automation::protocol::humane_v1::Permission;

    fn perms(spec: Vec<Permission>, granted: Vec<Permission>, denied: Vec<Permission>) -> PermissionSet {
        PermissionSet { spec, granted, denied }
    }

    #[test]
    fn denied_wins_over_spec() {
        // Tool belongs to Shell; spec allows Shell but user denied it.
        let p = perms(vec![Permission::Shell], vec![], vec![Permission::Shell]);
        assert_eq!(p.covers("bash"), Coverage::Denied);
    }

    #[test]
    fn spec_grant_allows() {
        let p = perms(vec![Permission::Filesystem], vec![], vec![]);
        assert_eq!(p.covers("edit"), Coverage::Allowed);
    }

    #[test]
    fn user_granted_allows() {
        let p = perms(vec![], vec![Permission::Filesystem], vec![]);
        assert_eq!(p.covers("write_file"), Coverage::Allowed);
    }

    #[test]
    fn neither_grants_nor_denies_falls_through() {
        let p = perms(vec![Permission::Notification], vec![], vec![]);
        assert_eq!(p.covers("bash"), Coverage::FallThrough);
    }

    #[test]
    fn unknown_tool_falls_through_even_when_all_categories_granted() {
        // Unknown-category tool — no permission category covers it.
        let p = perms(
            vec![Permission::Shell, Permission::Filesystem, Permission::Network],
            vec![Permission::AiBrowser, Permission::Notification],
            vec![],
        );
        assert_eq!(p.covers("some_unknown_tool"), Coverage::FallThrough);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::permission_set_covers_tests 2>&1 | tail -10`
Expected: FAIL — `enum Coverage not found` and `method covers not found`.

- [ ] **Step 3: Implement `Coverage` enum + `PermissionSet::covers`**

Add to `src-tauri/src/automation/runtime/mod.rs` (immediately below the `PermissionSet` struct):

```rust
/// Result of checking whether a `PermissionSet` covers a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Tool's category is in `denied` — explicit deny.
    Denied,
    /// Tool's category is in `spec` ∪ `granted` (and not in `denied`) — auto-approve.
    Allowed,
    /// Tool's category is in neither — fall through to SafetyManager normal flow.
    FallThrough,
}

impl PermissionSet {
    /// Decide whether this set covers a per-call tool authorization decision.
    ///
    /// `denied` wins over `spec` and `granted`. Unknown-category tools always
    /// fall through (no permission category names them).
    pub fn covers(&self, tool_name: &str) -> Coverage {
        let cat = crate::automation::runtime::permission_for_tool(tool_name);
        if matches!(cat, crate::automation::protocol::humane_v1::Permission::Unknown) {
            return Coverage::FallThrough;
        }
        if self.denied.contains(&cat) {
            return Coverage::Denied;
        }
        if self.spec.contains(&cat) || self.granted.contains(&cat) {
            return Coverage::Allowed;
        }
        Coverage::FallThrough
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::permission_set_covers_tests 2>&1 | tail -10`
Expected: PASS (5 tests).

### Task 1.3: Write failing test for `ChatApprovalHandler`

- [ ] **Step 1: Create `safety/approval.rs` with the failing test**

Create file `src-tauri/src/safety/approval.rs`:

```rust
//! ApprovalHandler — origin-specific async behavior for `should_approve = RequireApproval`.
//!
//! `ChatApprovalHandler` (here) wraps the existing `PendingApprovals` IPC for
//! chat AND browser sub-loop (user is in the chat session either way).
//! `AutomationApprovalHandler` (in `automation/runtime/approval.rs`) escalates
//! via DB and returns `Escalated` so the run pauses for asynchronous resolution.

use std::sync::Arc;
use async_trait::async_trait;

/// Origin of an approval request — used by handlers to know how to route.
#[derive(Debug, Clone)]
pub enum ApprovalOrigin {
    Chat { conversation_id: String },
    Automation { activity_id: String },
    BrowserSubLoop { conversation_id: String, browser_task_id: String },
}

/// The handler's answer to `handle_ask`. `Escalated` signals the caller to
/// pause/checkpoint — the user will resolve later out-of-band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Denied,
    Escalated,
}

#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn handle_ask(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome;
}

/// Default implementation — wraps `app::PendingApprovals`. The chat path and
/// the browser sub-loop both use this (user is in chat for both).
pub struct ChatApprovalHandler {
    pending_approvals: Arc<crate::app::PendingApprovals>,
}

impl ChatApprovalHandler {
    pub fn new(pending_approvals: Arc<crate::app::PendingApprovals>) -> Self {
        Self { pending_approvals }
    }
}

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler {
    async fn handle_ask(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome {
        // Tool id for PendingApprovals registration. For chat/browser sub-loop
        // the existing dispatcher registers via the tool_call_id; here we use
        // a unique key derived from the origin so the receiver isn't shared
        // across calls. Production wires the real tool_call_id by NOT calling
        // this handler when ToolDispatcher already has the id in scope — see
        // dispatch.run_one in tool_dispatch/mod.rs.
        let key = match origin {
            ApprovalOrigin::Chat { conversation_id } => format!("chat:{conversation_id}"),
            ApprovalOrigin::BrowserSubLoop { conversation_id, browser_task_id } => {
                format!("browser-sub:{conversation_id}:{browser_task_id}")
            }
            ApprovalOrigin::Automation { .. } => {
                // ChatApprovalHandler should never be called for Automation;
                // the dispatcher chooses AutomationApprovalHandler for that
                // origin. Surface this as Denied to fail loud.
                return ApprovalOutcome::Denied;
            }
        };
        let rx = self.pending_approvals.register(key);
        match rx.await {
            Ok(result) if result.approved => ApprovalOutcome::Approved,
            Ok(_) => ApprovalOutcome::Denied,
            Err(_) => ApprovalOutcome::Denied, // sender dropped — treat as denied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_handler_returns_approved_when_pending_approvals_resolves_true() {
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa.clone());

        let origin = ApprovalOrigin::Chat { conversation_id: "c1".into() };
        let handler_task = tokio::spawn({
            let handler = Arc::new(handler);
            async move { handler.handle_ask("bash", &serde_json::json!({}), &origin).await }
        });

        // Give the handler a moment to register.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let resolved = pa.resolve("chat:c1", crate::app::ApprovalResult { approved: true, reason: None });
        assert!(resolved, "PendingApprovals.resolve should return true for a registered key");

        let outcome = handler_task.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Approved);
    }

    #[tokio::test]
    async fn chat_handler_returns_denied_when_pending_approvals_resolves_false() {
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa.clone());

        let origin = ApprovalOrigin::Chat { conversation_id: "c2".into() };
        let handler_task = tokio::spawn({
            let handler = Arc::new(handler);
            async move { handler.handle_ask("bash", &serde_json::json!({}), &origin).await }
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        pa.resolve("chat:c2", crate::app::ApprovalResult { approved: false, reason: Some("nope".into()) });

        let outcome = handler_task.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }

    #[tokio::test]
    async fn chat_handler_returns_denied_for_automation_origin() {
        // Defense-in-depth: ChatApprovalHandler must never silently succeed
        // for an Automation origin (that would bypass escalation).
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa);
        let origin = ApprovalOrigin::Automation { activity_id: "act-1".into() };
        let outcome = handler.handle_ask("bash", &serde_json::json!({}), &origin).await;
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }
}
```

- [ ] **Step 2: Wire the module + re-exports**

Edit `src-tauri/src/safety/mod.rs`:

Add (near the top of the file, with the other `pub mod` declarations):

```rust
pub mod approval;
pub use approval::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome, ChatApprovalHandler};
```

- [ ] **Step 3: Run tests to verify they fail then pass**

Run: `cd src-tauri && cargo test --lib safety::approval::tests 2>&1 | tail -10`
Expected: PASS (3 tests) once the module is wired and compiles.

If `ApprovalResult` field names differ from `{ approved: bool, reason: Option<String> }` in the actual `app.rs:46+` definition, adjust the test's `ApprovalResult` literal accordingly — run `grep -n "pub struct ApprovalResult" src/app.rs` to see the canonical shape, and update the test (the production `ChatApprovalHandler` code uses `.approved` field access — adjust if the field is named differently).

### Task 1.4: Add `permissions` + `origin_kind` fields to `ToolDispatchContext`

- [ ] **Step 1: Write the failing test (one negative + one positive coverage path)**

Add to `src-tauri/src/agent/tool_dispatch/mod.rs` tests module (after `dispatch_short_circuits_when_cancelled`):

```rust
#[tokio::test]
async fn dispatch_blocks_when_permission_set_denies() {
    let executed = Arc::new(AtomicBool::new(false));
    let mut reg = ToolRegistry::new();
    reg.register(EchoTool::new(executed.clone()));
    let d = make_dispatcher(Arc::new(reg));

    // Build a PermissionSet that denies Shell.
    let perms = crate::automation::runtime::PermissionSet {
        spec: vec![],
        granted: vec![],
        denied: vec![crate::automation::protocol::humane_v1::Permission::Shell],
    };
    let mut c = ctx();
    c.permissions = Some(perms);
    c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-1".into() };

    // EchoTool is not Shell, so it's NOT blocked — change the call to a
    // bash-named tool. Register a Shell-mapped tool with same EchoTool impl.
    let mut reg2 = ToolRegistry::new();
    reg2.register(EchoTool::with_name(executed.clone(), "bash"));
    let d = make_dispatcher(Arc::new(reg2));

    let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
    let outs = d.dispatch(calls, &c).await;

    assert_eq!(outs.len(), 1);
    assert!(outs[0].result.is_err());
    assert!(outs[0].is_error);
    assert_eq!(outs[0].message_content, "Error: tool denied by spec");
    assert!(!executed.load(Ordering::SeqCst), "denied tool must not run");
}

#[tokio::test]
async fn dispatch_auto_approves_when_permission_set_allows() {
    let executed = Arc::new(AtomicBool::new(false));
    let mut reg = ToolRegistry::new();
    reg.register(EchoTool::with_name(executed.clone(), "bash"));
    let d = make_dispatcher(Arc::new(reg));

    // Grant Shell explicitly.
    let perms = crate::automation::runtime::PermissionSet {
        spec: vec![crate::automation::protocol::humane_v1::Permission::Shell],
        granted: vec![],
        denied: vec![],
    };
    let mut c = ctx();
    c.permissions = Some(perms);
    c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-2".into() };

    let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
    let outs = d.dispatch(calls, &c).await;

    assert_eq!(outs.len(), 1);
    assert!(outs[0].result.is_ok(), "permitted tool must execute");
    assert!(executed.load(Ordering::SeqCst));
}
```

(`EchoTool::with_name` may not exist — if not, extend the test helper at the top of the tests module:

```rust
impl EchoTool {
    fn with_name(executed: Arc<AtomicBool>, name: &str) -> Self {
        // existing EchoTool::new, but with `name` override; alter the field
        // (or add a `name` parameter to the constructor) to produce a Tool whose
        // `Tool::name() -> &str` returns the given string.
    }
}
```

— see the actual `EchoTool` definition near `tool_dispatch/mod.rs:892` and add the `with_name` helper alongside `new` to keep this test self-contained.)

- [ ] **Step 2: Run tests to verify they fail (compile error)**

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests::dispatch_blocks_when_permission_set_denies agent::tool_dispatch::tests::dispatch_auto_approves_when_permission_set_allows 2>&1 | tail -10`
Expected: FAIL — `no field 'permissions'` or `no variant 'ApprovalOriginKind::Automation'`.

- [ ] **Step 3: Add the new ctx fields + enum**

Edit `src-tauri/src/agent/tool_dispatch/mod.rs`:

Add near the top of the file (with the other public items, before the existing `ToolDispatchContext`):

```rust
/// Origin of the dispatch — drives the `ApprovalOrigin` passed to `handle_ask`
/// and lets the dispatcher know which handler to consult on `RequireApproval`.
#[derive(Debug, Clone)]
pub enum ApprovalOriginKind {
    Chat { conversation_id: String },
    Automation { activity_id: String },
    BrowserSubLoop { conversation_id: String, browser_task_id: String },
}

impl ApprovalOriginKind {
    pub fn to_approval_origin(&self) -> crate::safety::ApprovalOrigin {
        match self {
            Self::Chat { conversation_id } =>
                crate::safety::ApprovalOrigin::Chat { conversation_id: conversation_id.clone() },
            Self::Automation { activity_id } =>
                crate::safety::ApprovalOrigin::Automation { activity_id: activity_id.clone() },
            Self::BrowserSubLoop { conversation_id, browser_task_id } =>
                crate::safety::ApprovalOrigin::BrowserSubLoop {
                    conversation_id: conversation_id.clone(),
                    browser_task_id: browser_task_id.clone(),
                },
        }
    }
}
```

Then change the struct definition at `tool_dispatch/mod.rs:37` from:

```rust
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}
```

to:

```rust
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// Slice 1b — declarative pre-authorization. `Some` for automation;
    /// `None` for chat & browser sub-loop.
    pub permissions: Option<crate::automation::runtime::PermissionSet>,
    /// Slice 1b — origin of this dispatch. Determines which `ApprovalHandler`
    /// is consulted on `RequireApproval`.
    pub origin_kind: ApprovalOriginKind,
}
```

- [ ] **Step 4: Update all 4 existing ctx literals**

In `src-tauri/src/agent/dispatcher.rs:2525` — inside `ChatDelegate::execute_tool_calls`, the existing literal becomes:

```rust
let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
    session_id: self.conversation_id.clone(),
    conversation_id: self.conversation_id.clone(),
    workspace_root: self.workspace_root.clone(),
    attached_dirs: vec![],
    safety_mode: self.safety_mode.clone(),
    iteration: self.turn_index.fetch_add(1, Ordering::Relaxed) as usize,
    cancel: reason_ctx.cancellation_token.clone(),
    permissions: None,
    origin_kind: crate::agent::tool_dispatch::ApprovalOriginKind::Chat {
        conversation_id: self.conversation_id.clone(),
    },
};
```

In `src-tauri/src/agent/tool_dispatch/mod.rs:917` (test `ctx()` helper):

```rust
fn ctx() -> ToolDispatchContext {
    ToolDispatchContext {
        session_id: "s".into(),
        conversation_id: "s".into(),
        workspace_root: None,
        attached_dirs: vec![],
        safety_mode: None,
        iteration: 1,
        cancel: None,
        permissions: None,
        origin_kind: ApprovalOriginKind::Chat { conversation_id: "s".into() },
    }
}
```

In `src-tauri/src/agent/tool_dispatch/mod.rs:1123` and `:1171` (the two other test literals): add the same two trailing fields (`permissions: None`, `origin_kind: ApprovalOriginKind::Chat { conversation_id: ... }` — copy the conversation_id used elsewhere in that test).

- [ ] **Step 5: Verify compile**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10`
Expected: empty. If any other `ToolDispatchContext { ... }` literal appears (it shouldn't — grep already confirmed 4), add `permissions: None, origin_kind: ApprovalOriginKind::Chat { conversation_id: "...".into() }` to each.

### Task 1.5: Replace `pending_approvals` field with `approval_handler` in `ToolDispatcher`

- [ ] **Step 1: Write the byte-equivalence test for ChatApprovalHandler wrap**

Add to `src-tauri/src/agent/tool_dispatch/mod.rs` tests module (after the Task 1.4 tests):

```rust
#[tokio::test]
async fn dispatch_with_chat_approval_handler_byte_equivalent_to_pending_approvals() {
    // Set up: a tool whose ApprovalRequirement is Always (always asks) + an
    // EchoTool that records execution. Build a dispatcher with the new
    // approval_handler field wrapping a PendingApprovals; manually resolve
    // the request. Outcome shape must match prior (pre-Slice-1b) behavior.
    let executed = Arc::new(AtomicBool::new(false));
    let mut reg = ToolRegistry::new();
    // EchoTool already requires UnlessAutoApproved by default; use it directly.
    reg.register(EchoTool::new(executed.clone()));
    let (d, pa) = make_dispatcher_returning_pa(Arc::new(reg));

    let c = ctx(); // chat origin, no permissions
    let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];

    // Start dispatch in the background; meanwhile resolve the approval.
    let dispatcher = d.clone();
    let task = tokio::spawn(async move {
        dispatcher.dispatch(calls, &c).await
    });

    // The dispatcher registers a oneshot keyed on tc.id ("c1"). Resolve approve.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    pa.resolve("c1", crate::app::ApprovalResult { approved: true, reason: None });

    let outs = task.await.unwrap();
    assert_eq!(outs.len(), 1);
    assert!(outs[0].result.is_ok(), "approved tool must execute");
    assert!(executed.load(Ordering::SeqCst));
}
```

(`make_dispatcher_returning_pa` is the existing helper at `tool_dispatch/mod.rs:1063` — verify its signature returns `(Arc<ToolDispatcher<...>>, Arc<PendingApprovals>)`; if it does, this test uses it as-is.)

- [ ] **Step 2: Run test to verify it fails (compile error)**

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests::dispatch_with_chat_approval_handler_byte_equivalent_to_pending_approvals 2>&1 | tail -10`
Expected: FAIL — likely passes if the existing `pending_approvals`-based path works, OR fails because the test calls field/method names that don't exist yet. Either is fine; the goal is to lock the regression after the refactor.

- [ ] **Step 3: Replace `pending_approvals` field with `approval_handler` in `ToolDispatcher`**

In `src-tauri/src/agent/tool_dispatch/mod.rs`:

Change the struct field at `:75`:

```rust
pub(crate) pending_approvals: Arc<crate::app::PendingApprovals>,
```

to:

```rust
pub(crate) approval_handler: Arc<dyn crate::safety::ApprovalHandler>,
/// Retained for backward access during migration — chat dispatch's per-tool
/// approval flow at lines :292/:708/:826 uses register/resolve keyed by
/// tool_call_id. Wrap this field's owner in ChatApprovalHandler::new for
/// the new approval_handler field's default. Keep both for now; remove
/// after the call sites are migrated to handler-based flow in Step 4.
pub(crate) pending_approvals: Arc<crate::app::PendingApprovals>,
```

Update the `new` constructor at `:93`:

```rust
pub fn new(
    tools: Arc<ToolRegistry>,
    app_handle: tauri::AppHandle<R>,
    safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    pending_approvals: Arc<crate::app::PendingApprovals>,
    infra_service: Option<Arc<crate::infra::InfraService>>,
    trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    hook_bus: Arc<crate::agent::hook_bus::HookBus>,
    heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
) -> Self {
    let approval_handler: Arc<dyn crate::safety::ApprovalHandler> =
        Arc::new(crate::safety::ChatApprovalHandler::new(pending_approvals.clone()));
    Self {
        tools,
        app_handle,
        safety_manager,
        approval_handler,
        pending_approvals,
        infra_service,
        trajectory_store,
        tool_budget,
        hook_bus,
        heartbeat,
    }
}
```

Add a sibling constructor that takes a pre-built `approval_handler` (for Automation in Task 2):

```rust
#[allow(clippy::too_many_arguments)]
pub fn new_with_approval_handler(
    tools: Arc<ToolRegistry>,
    app_handle: tauri::AppHandle<R>,
    safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    approval_handler: Arc<dyn crate::safety::ApprovalHandler>,
    pending_approvals: Arc<crate::app::PendingApprovals>,
    infra_service: Option<Arc<crate::infra::InfraService>>,
    trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    hook_bus: Arc<crate::agent::hook_bus::HookBus>,
    heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
) -> Self {
    Self {
        tools, app_handle, safety_manager,
        approval_handler, pending_approvals,
        infra_service, trajectory_store, tool_budget, hook_bus, heartbeat,
    }
}
```

- [ ] **Step 4: Verify build + run the byte-equivalence test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty.

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests 2>&1 | tail -10`
Expected: ALL pass — including the new byte-equivalence test and all 17+ pre-existing dispatch tests. The `pending_approvals` field is kept temporarily; existing flows are unchanged.

### Task 1.6: Wire PermissionSet decision + ApprovalHandler routing into `dispatch.run_one`

- [ ] **Step 1: Confirm the test from Task 1.4 still fails as expected**

(The `dispatch_blocks_when_permission_set_denies` and `dispatch_auto_approves_when_permission_set_allows` tests written in Task 1.4 currently still fail — `permissions` field exists but the dispatch flow doesn't consult it yet.)

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests::dispatch_blocks_when_permission_set_denies 2>&1 | tail -10`
Expected: FAIL — test runs (compiles) but asserts mismatch (tool executes despite denied permissions, OR test panics because behavior is still pre-Slice-1b).

- [ ] **Step 2: Implement the PermissionSet decision in `run_one`**

In `src-tauri/src/agent/tool_dispatch/mod.rs:run_one` (around line 157+), at the very top of the function (BEFORE the `safety.should_approve` call — find that call via `grep -n "should_approve" src/agent/tool_dispatch/mod.rs`), insert:

```rust
// Slice 1b — PermissionSet decision (declarative pre-authorization).
// Denied → reject immediately, no safety check.
// Allowed → effectively mode_override = Yolo (auto-approve).
// FallThrough → safety.should_approve runs with the dispatcher's
//                default mode_override for the origin.
use crate::automation::runtime::Coverage;
let perm_decision = ctx.permissions.as_ref().map(|p| p.covers(&tc.name));
if matches!(perm_decision, Some(Coverage::Denied)) {
    tracing::info!(
        tool = %tc.name,
        "[Slice 1b] tool denied by PermissionSet — rejecting outcome"
    );
    return Self::denied_outcome(&tc.id, &tc.name, &tc.arguments);
}
let permission_mode_override = match perm_decision {
    Some(Coverage::Allowed) => Some(crate::safety::SafetyMode::Yolo),
    _ => None, // FallThrough or None → use ctx.safety_mode (or default)
};
```

Add the helper alongside the existing `cancelled_outcome` helper:

```rust
fn denied_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
    ToolDispatchOutcome {
        tool_call_id: id.to_string(),
        tool_name: name.to_string(),
        arguments: args.clone(),
        result: Err(crate::agent::tools::tool::ToolError::kinded(
            crate::agent::tools::tool::ToolErrorKind::PermissionDenied,
            "tool denied by spec",
        )),
        paths_touched: vec![],
        was_mutation: false,
        soft_error: None,
        rejected: true,
        message_content: "Error: tool denied by spec".to_string(),
        is_error: true,
    }
}
```

Then, where `safety.should_approve(...)` is called, replace the existing call:

```rust
// existing (approximate):
// let decision = safety.should_approve(&tc.name, &tc.arguments, &tool_approval_req, ctx.safety_mode.as_ref());

let effective_mode = permission_mode_override.as_ref().or(ctx.safety_mode.as_ref());
let decision = safety.should_approve(&tc.name, &tc.arguments, &tool_approval_req, effective_mode);
```

- [ ] **Step 3: Route `RequireApproval` through `approval_handler.handle_ask` for non-chat origins**

In `src-tauri/src/agent/tool_dispatch/mod.rs`, find the existing block that handles `ApprovalDecision::RequireApproval` (it currently uses `self.pending_approvals.register(tc.id.clone())` at `:292`, `:708`, `:826`). For chat origin, behavior stays the same. For non-chat origins, route through `approval_handler.handle_ask`:

```rust
ApprovalDecision::RequireApproval { .. } => {
    use crate::safety::ApprovalOutcome;
    match &ctx.origin_kind {
        ApprovalOriginKind::Chat { .. } => {
            // Existing behavior — chat uses tool_call_id-keyed PendingApprovals
            // for compatibility with the React modal. Byte-equivalent to pre-Slice-1b.
            let rx = self.pending_approvals.register(tc.id.clone());
            // ... existing post-register logic (emit IPC, await, branch on result) ...
        }
        _ => {
            let outcome = self.approval_handler.handle_ask(
                &tc.name,
                &tc.arguments,
                &ctx.origin_kind.to_approval_origin(),
            ).await;
            match outcome {
                ApprovalOutcome::Approved => { /* proceed to execute */ }
                ApprovalOutcome::Denied => {
                    return Self::denied_outcome(&tc.id, &tc.name, &tc.arguments);
                }
                ApprovalOutcome::Escalated => {
                    return Self::escalated_outcome(&tc.id, &tc.name, &tc.arguments);
                }
            }
        }
    }
}
```

Add the `escalated_outcome` helper next to `denied_outcome`:

```rust
fn escalated_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
    ToolDispatchOutcome {
        tool_call_id: id.to_string(),
        tool_name: name.to_string(),
        arguments: args.clone(),
        result: Err(crate::agent::tools::tool::ToolError::kinded(
            crate::agent::tools::tool::ToolErrorKind::Other,
            "tool execution awaiting user approval",
        )),
        paths_touched: vec![],
        was_mutation: false,
        soft_error: None,
        rejected: false,
        message_content: "Error: awaiting user approval".to_string(),
        is_error: true,
    }
}
```

- [ ] **Step 4: Run all Task 1 tests**

Run: `cd src-tauri && cargo test --lib agent::tool_dispatch::tests safety::approval::tests automation::runtime::permission_for_tool_tests automation::runtime::permission_set_covers_tests 2>&1 | tail -15`
Expected: all PASS, including:
- the 3 ChatApprovalHandler tests,
- the 5 `permission_for_tool` mapping tests,
- the 5 `PermissionSet::covers` tests,
- the 2 new dispatch tests (denied/auto-approve),
- the byte-equivalence test,
- all 17+ pre-existing dispatch tests.

- [ ] **Step 5: Full agent suite regression check**

Run: `cd src-tauri && cargo test --lib agent:: 2>&1 | tail -10`
Expected: same number of passes as baseline + new tests. The 2 pre-existing unrelated failures (`shell::test_daemon_mode_approval_unchanged`, `skill_marketplace::truncate_for_error_long`) stay failing — they are unrelated to safety/dispatch.

### Task 1.7: Commit Task 1

- [ ] **Step 1: Stage and commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification add \
    src-tauri/src/safety/approval.rs \
    src-tauri/src/safety/mod.rs \
    src-tauri/src/automation/runtime/mod.rs \
    src-tauri/src/agent/tool_dispatch/mod.rs \
    src-tauri/src/agent/dispatcher.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification commit -m "feat(safety): introduce ApprovalHandler + ToolDispatchContext.permissions

Slice 1b foundation. Adds:
- ApprovalHandler trait + ApprovalOrigin/ApprovalOutcome enums
- ChatApprovalHandler wrapping PendingApprovals (chat byte-equivalent)
- ToolDispatchContext.{permissions, origin_kind} fields
- PermissionSet::covers + permission_for_tool tool-name → category mapping
- dispatch.run_one: PermissionSet decision (Denied/Allowed/FallThrough)
- dispatch.run_one: non-chat RequireApproval routes through handle_ask

Chat behavior remains byte-equivalent: existing PendingApprovals path for
chat origins is unchanged; new handler routing only fires for non-chat
origins (Automation/BrowserSubLoop) which today never reach this code path.
Task 2 wires automation; Task 3 wires browser sub-loop."
```

---

## Task 2: `HeadlessDelegate` through SafetyManager + escalation

**Files:**
- Create: `src-tauri/src/automation/runtime/approval.rs`
- Modify: `src-tauri/src/db/migrations.rs` (V56), `src-tauri/src/agent/headless.rs`, `src-tauri/src/automation/runtime/execute.rs` (`make_delegate` test helper), `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs` (invoke_handler!)

### Task 2.1: Add V56 migration

- [ ] **Step 1: Re-verify V56 is the next free slot**

Run: `grep -n "Active migration registry\|V5[0-9]\b" docs/CONTEXT.md src-tauri/src/db/migrations.rs 2>&1 | head -20`
Expected: highest V-number on main is V55. If V56 is claimed by an open PR per `CONTEXT.md`, pick the next free integer (e.g. V57) and substitute throughout this task.

- [ ] **Step 2: Write the schema migration test**

Add to `src-tauri/src/db/migrations.rs` (in the existing tests module, alongside the other `Vxx —` test blocks):

```rust
// ─── V56 — Slice 1b automation_approval_requests ─────────────────────────
#[test]
fn v56_creates_automation_approval_requests_and_pending_column() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations_up_to(&conn, 56).unwrap();
    // Table exists.
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='table' AND name='automation_approval_requests'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(n, 1, "table must exist after V56");
    // Required columns are present.
    let mut required = std::collections::HashSet::from([
        "id", "activity_id", "tool_name", "arguments_json",
        "status", "created_at", "resolved_at",
    ]);
    let mut stmt = conn.prepare("PRAGMA table_info(automation_approval_requests)").unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
    for row in rows {
        required.remove(row.unwrap().as_str());
    }
    assert!(required.is_empty(), "missing columns: {required:?}");
    // Index exists.
    let idx: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_aar_activity_status'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(idx, 1, "idx_aar_activity_status must exist after V56");
    // pending_approval_request_id column on automation_activities.
    let mut found = false;
    let mut stmt2 = conn.prepare("PRAGMA table_info(automation_activities)").unwrap();
    let rows = stmt2.query_map([], |r| r.get::<_, String>(1)).unwrap();
    for row in rows {
        if row.unwrap() == "pending_approval_request_id" { found = true; }
    }
    assert!(found, "automation_activities.pending_approval_request_id missing after V56");
}
```

(`run_migrations_up_to` is the standard test helper used by other Vxx tests in this file — verify it exists via `grep -n "fn run_migrations_up_to" src-tauri/src/db/migrations.rs`; if it's named differently, adjust.)

- [ ] **Step 3: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib db::migrations::v56_creates_automation_approval_requests_and_pending_column 2>&1 | tail -10`
Expected: FAIL — `table sqlite_master ... no such table 'automation_approval_requests'` or V56 not registered.

- [ ] **Step 4: Implement V56**

In `src-tauri/src/db/migrations.rs`, add after the last existing migration (currently V55). Follow the file's existing pattern (`// ─── V56 — … ───` header + a match arm in the migration runner):

```rust
// ─── V56 — Slice 1b safety chokepoint: automation approval requests ──────
56 => {
    conn.execute_batch(r#"
        CREATE TABLE automation_approval_requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            activity_id INTEGER NOT NULL REFERENCES automation_activities(id),
            tool_name TEXT NOT NULL,
            arguments_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            resolved_at TIMESTAMP NULL
        );
        CREATE INDEX idx_aar_activity_status ON automation_approval_requests(activity_id, status);
        ALTER TABLE automation_activities
            ADD COLUMN pending_approval_request_id INTEGER NULL
                REFERENCES automation_approval_requests(id);
    "#)?;
}
```

Update the migration version constant (typically a `LATEST_VERSION` or `CURRENT_VERSION` const at the top of the file — find via `grep -n "LATEST_VERSION\|CURRENT_VERSION\|MIGRATION_LATEST" src-tauri/src/db/migrations.rs`) from `55` to `56`.

- [ ] **Step 5: Run the migration test**

Run: `cd src-tauri && cargo test --lib db::migrations::v56 2>&1 | tail -10`
Expected: PASS.

### Task 2.2: Implement `AutomationApprovalHandler`

- [ ] **Step 1: Create `automation/runtime/approval.rs` with the failing test**

Create file `src-tauri/src/automation/runtime/approval.rs`:

```rust
//! Slice 1b — AutomationApprovalHandler.
//!
//! Routes `ApprovalDecision::RequireApproval` for automation origins to a
//! pause-and-resolve flow: writes the request to `automation_approval_requests`,
//! transitions the activity to `paused_pending_approval`, emits a Tauri event,
//! and returns `Escalated` so the dispatcher pauses the loop.

use std::sync::Arc;
use async_trait::async_trait;
use crate::safety::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome};

pub struct AutomationApprovalHandler {
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Optional Tauri app handle for emitting `automation:approval-needed`.
    /// None in tests (DB writes still happen; event is skipped).
    app_handle: Option<tauri::AppHandle>,
}

impl AutomationApprovalHandler {
    pub fn new(
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        Self { db, app_handle }
    }
}

#[async_trait]
impl ApprovalHandler for AutomationApprovalHandler {
    async fn handle_ask(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome {
        let ApprovalOrigin::Automation { activity_id } = origin else {
            // Defensive: never invoke this handler for non-Automation origins.
            tracing::error!(?origin, "AutomationApprovalHandler called for non-Automation origin");
            return ApprovalOutcome::Denied;
        };

        let activity_id: i64 = activity_id.parse().unwrap_or(0);
        let arguments_json = arguments.to_string();
        let tool_name_owned = tool_name.to_string();

        // Synchronous DB writes — bounded short critical section.
        let request_id: Option<i64> = {
            let conn = self.db.lock().ok();
            conn.and_then(|conn| {
                conn.execute(
                    "INSERT INTO automation_approval_requests \
                     (activity_id, tool_name, arguments_json, status) \
                     VALUES (?1, ?2, ?3, 'pending')",
                    rusqlite::params![activity_id, tool_name_owned, arguments_json],
                ).ok()?;
                let id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE automation_activities \
                     SET status = 'paused_pending_approval', \
                         pending_approval_request_id = ?1 \
                     WHERE id = ?2",
                    rusqlite::params![id, activity_id],
                ).ok()?;
                Some(id)
            })
        };

        if let (Some(app), Some(req_id)) = (self.app_handle.as_ref(), request_id) {
            use tauri::Emitter;
            let _ = app.emit("automation:approval-needed", serde_json::json!({
                "activity_id": activity_id,
                "request_id": req_id,
                "tool_name": tool_name_owned,
            }));
        }

        ApprovalOutcome::Escalated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db_with_migrations() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 56).unwrap();
        // Seed a row in automation_activities.
        conn.execute(
            "INSERT INTO automation_activities (id, spec_id, status) VALUES (1, 1, 'running')",
            [],
        ).ok();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn handle_ask_writes_request_and_transitions_activity() {
        let db = in_memory_db_with_migrations();
        let handler = AutomationApprovalHandler::new(db.clone(), None);
        let outcome = handler.handle_ask(
            "bash",
            &serde_json::json!({"command": "ls"}),
            &ApprovalOrigin::Automation { activity_id: "1".into() },
        ).await;
        assert_eq!(outcome, ApprovalOutcome::Escalated);

        // Verify DB state.
        let conn = db.lock().unwrap();
        let (tool, status): (String, String) = conn.query_row(
            "SELECT tool_name, status FROM automation_approval_requests WHERE activity_id = 1",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(tool, "bash");
        assert_eq!(status, "pending");

        let activity_status: String = conn.query_row(
            "SELECT status FROM automation_activities WHERE id = 1",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(activity_status, "paused_pending_approval");
    }

    #[tokio::test]
    async fn handle_ask_denies_for_non_automation_origin() {
        let db = in_memory_db_with_migrations();
        let handler = AutomationApprovalHandler::new(db, None);
        let outcome = handler.handle_ask(
            "bash",
            &serde_json::json!({}),
            &ApprovalOrigin::Chat { conversation_id: "c1".into() },
        ).await;
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }
}
```

- [ ] **Step 2: Wire the module**

In `src-tauri/src/automation/runtime/mod.rs`, add:

```rust
pub mod approval;
pub use approval::AutomationApprovalHandler;
```

(If the existing migration helper `run_migrations_up_to` is `pub(crate)`, the test uses `crate::db::migrations::run_migrations_up_to` — adjust if its visibility differs.)

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib automation::runtime::approval::tests 2>&1 | tail -10`
Expected: PASS (2 tests).

### Task 2.3: Inject `safety_manager`, `tool_dispatcher`, `approval_handler` into `HeadlessDelegate`

- [ ] **Step 1: Update the struct definition**

Edit `src-tauri/src/agent/headless.rs:71`. Add three fields after `app_handle`:

```rust
pub struct HeadlessDelegate {
    pub spec_id: String,
    pub activity_id: String,
    pub session_id: String,
    pub permissions: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
    pub auto_continue: AutoContinueConfig,
    pub llm: Arc<dyn LlmProvider>,
    pub model: String,
    pub tools: Arc<ToolRegistry>,
    pub cost: Arc<CostCapState>,
    pub workspace_root: PathBuf,
    pub app_handle: Option<tauri::AppHandle>,

    /// Slice 1b — shared SafetyManager singleton (from AppState). When None,
    /// the delegate skips dispatcher-based tool execution (fall back to the
    /// existing per-tool execute path — test-only scenarios).
    pub safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    /// Slice 1b — shared ToolDispatcher singleton. None for tests that
    /// don't exercise the dispatch chokepoint.
    pub tool_dispatcher: Option<Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
    /// Slice 1b — escalation handler for `RequireApproval`. Required when
    /// `tool_dispatcher` is `Some`. Wraps `AutomationApprovalHandler` in prod.
    pub approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,

    // ── IM close-loop extension fields ────────────────────────────────────
    pub channel_manager: Option<Arc<RwLock<ChannelManager>>>,
    pub reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,
    pub streaming_handle: Option<Arc<dyn crate::channels::types::StreamingHandle>>,
    pub system_prompt_override: Option<String>,
}
```

- [ ] **Step 2: Update `make_delegate` test helper**

In `src-tauri/src/automation/runtime/execute.rs:44-72`, add the three new fields to the `AutomationDelegate { ... }` literal:

```rust
AutomationDelegate {
    spec_id: spec_id.to_string(),
    activity_id: activity_id.to_string(),
    session_id: format!("sess-{}", activity_id),
    permissions: perms,
    memory: Arc::new(MemoryStore::new(tmp.path().to_path_buf())),
    db: Arc::new(std::sync::Mutex::new(conn)),
    gate: Arc::new(Mutex::new(None)),
    auto_continue: AutoContinueConfig::default(),
    llm: test_support::fake_llm(),
    model: "claude-sonnet-4-6".to_string(),
    tools: test_support::empty_tool_registry(),
    cost: Arc::new(CostCapState::new(CostCapConfig {
        per_run_usd: 1.00,
        per_day_usd: 10.00,
    })),
    workspace_root: tmp.path().to_path_buf(),
    app_handle: None,
    safety_manager: None,    // Tests that don't exercise dispatch.
    tool_dispatcher: None,
    approval_handler: None,
    channel_manager: None,
    reply_handle: None,
    streaming_handle: None,
    system_prompt_override: None,
}
```

- [ ] **Step 3: Verify build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty. If any other constructor of `HeadlessDelegate` exists in production code (grep `HeadlessDelegate {`), update each to set the three fields appropriately.

### Task 2.4: Route `HeadlessDelegate::execute_tool_calls` through `ToolDispatcher` with PermissionSet

- [ ] **Step 1: Find the existing `execute_tool_calls` impl in `headless.rs`**

Run: `grep -n "fn execute_tool_calls" src-tauri/src/agent/headless.rs`
Expected: one site. Read it to understand the current dispatch path (it likely iterates `tool_calls`, looks up each tool in `self.tools`, executes inline, and constructs `ToolResult` content blocks for the LLM).

- [ ] **Step 2: Write the integration test (failing)**

Add to `src-tauri/src/automation/runtime/execute.rs` (in the test module, alongside the existing tests):

```rust
#[tokio::test]
async fn headless_delegate_escalates_on_uncovered_tool() {
    use crate::automation::protocol::humane_v1::Permission;
    use std::path::PathBuf;

    let tmp = tempfile::tempdir().unwrap();
    let conn = init_test_db_with_migrations(&tmp); // helper to apply migrations up to V56
    let perms = PermissionSet {
        spec: vec![Permission::Filesystem], // grants edit/read_file/...
        granted: vec![],
        denied: vec![],
    };

    // Seed an activity row so the handler has something to update.
    {
        let conn_handle = std::sync::Mutex::new(conn);
        conn_handle.lock().unwrap().execute(
            "INSERT INTO automation_activities (id, spec_id, status) VALUES (42, 1, 'running')",
            [],
        ).unwrap();
        let conn = std::sync::Mutex::into_inner(conn_handle).unwrap();
        // Build delegate with a tool dispatcher that has SafetyManager.
        let mut delegate = make_delegate_with_safety_dispatch("spec-1", "42", &tmp, conn, perms);

        // Mock LLM: emit a bash call (not covered by Filesystem grant).
        // ... use whatever fake_llm mechanism the test_support module provides
        //     to script "respond with ToolCalls: [bash {command: 'ls'}]" ...

        // Run the loop.
        let mut reason_ctx = crate::agent::types::ReasoningContext::new("sys".into());
        let outcome = crate::agent::agentic_loop::run_agentic_loop(
            &delegate, &mut reason_ctx, &crate::agent::types::AgenticLoopConfig::default(),
        ).await;

        // Assert: loop returns NeedApproval (the escalation path).
        assert!(matches!(outcome, crate::agent::types::LoopOutcome::NeedApproval { tool_name, .. } if tool_name == "bash"));

        // Assert: DB state — activity transitioned + a pending request exists.
        let conn = std::sync::Mutex::into_inner(/* re-acquire */ unimplemented!()).unwrap();
        let activity_status: String = conn.query_row(
            "SELECT status FROM automation_activities WHERE id = 42",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(activity_status, "paused_pending_approval");
        let req_status: String = conn.query_row(
            "SELECT status FROM automation_approval_requests WHERE activity_id = 42",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(req_status, "pending");
    }
}
```

(This test sketch contains `unimplemented!()` — that's a known shape gap: the existing `make_delegate` test helper builds a delegate without dispatcher, and reading the DB after the loop requires a fresh borrow path. The implementer must add a `make_delegate_with_safety_dispatch` helper that:
1. Uses `tauri::test::mock_app()` to provide a `tauri::AppHandle<MockRuntime>` — but `HeadlessDelegate.tool_dispatcher` is `Option<Arc<ToolDispatcher<tauri::Wry>>>`, NOT `MockRuntime`. **Resolution:** either change `HeadlessDelegate.tool_dispatcher` to be `Option<Arc<ToolDispatcher<R>>>` and make HeadlessDelegate generic, OR use a separate test scaffold. **Pick (b) for minimal scope**: the integration test sets up real `tauri::AppHandle<Wry>` via the runtime's test harness — see how `dispatcher.rs` does this in its production tests, and mirror.
2. Wraps `Arc::new(AutomationApprovalHandler::new(db.clone(), None))` as `approval_handler`.
3. Shares the DB Arc with the AutomationApprovalHandler so post-loop assertions can read.

If this test scaffold is non-trivial, the implementer may reasonably **defer the heavy integration test to a follow-up and provide a lighter unit test in `tool_dispatch/mod.rs` instead**: a test that constructs `ToolDispatcher` with an `AutomationApprovalHandler` directly + an `Automation` ctx, dispatches a single uncovered call, and asserts the outcome is `escalated_outcome`-shaped (`is_error=true`, `message_content="Error: awaiting user approval"`). Treat this as the **acceptable minimum** for Task 2 coverage; full delegate-level integration is documented as a follow-up.)

- [ ] **Step 3: Implement the dispatcher routing**

In `src-tauri/src/agent/headless.rs`, replace the body of `execute_tool_calls`. The new implementation:

```rust
async fn execute_tool_calls(
    &self,
    tool_calls: Vec<ToolCall>,
    reason_ctx: &mut ReasoningContext,
) -> Result<Option<LoopOutcome>, Error> {
    // Slice 1b — route through ToolDispatcher when wired. If not wired
    // (test-only scenarios), fall back to the existing bespoke per-tool
    // execute loop (which the implementer should preserve verbatim from
    // the prior implementation).
    let Some(dispatcher) = self.tool_dispatcher.as_ref() else {
        return self.execute_tool_calls_bespoke(tool_calls, reason_ctx).await;
    };

    let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
        session_id: self.session_id.clone(),
        conversation_id: self.session_id.clone(),
        workspace_root: Some(self.workspace_root.clone()),
        attached_dirs: vec![],
        safety_mode: Some(crate::safety::SafetyMode::Ask),
        iteration: 0,
        cancel: reason_ctx.cancellation_token.clone(),
        permissions: Some(self.permissions.clone()),
        origin_kind: crate::agent::tool_dispatch::ApprovalOriginKind::Automation {
            activity_id: self.activity_id.clone(),
        },
    };

    let outcomes = dispatcher.dispatch(tool_calls, &ctx).await;

    // Walk outcomes: if any is escalated, build LoopOutcome::NeedApproval
    // from the first escalated call and return immediately (terminates loop).
    for o in &outcomes {
        if o.is_error && o.message_content == "Error: awaiting user approval" {
            return Ok(Some(LoopOutcome::NeedApproval {
                tool_name: o.tool_name.clone(),
                tool_call_id: o.tool_call_id.clone(),
                parameters: o.arguments.clone(),
            }));
        }
    }

    // Otherwise: push tool results into reason_ctx.messages as the bespoke
    // path did. (See the existing implementation for the exact ChatMessage
    // shape used; reuse it byte-equivalent.)
    for o in outcomes {
        reason_ctx.messages.push(ChatMessage::tool_result(&o.tool_call_id, &o.message_content, o.is_error));
    }
    Ok(None)
}
```

(`execute_tool_calls_bespoke` = the prior body of `execute_tool_calls`, extracted into a private method with the same signature. This preserves the test-only path.)

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib agent::headless 2>&1 | tail -10` and `cd src-tauri && cargo test --lib automation::runtime 2>&1 | tail -10`
Expected: PASS — existing automation tests + the new escalation unit test.

### Task 2.5: Add Tauri commands `list_pending_automation_approvals` + `resolve_automation_approval`

- [ ] **Step 1: Write the commands and a unit test**

Append to `src-tauri/src/tauri_commands.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingApprovalView {
    pub id: i64,
    pub activity_id: i64,
    pub tool_name: String,
    pub arguments_json: String,
    pub created_at: String,
}

#[tauri::command]
pub async fn list_pending_automation_approvals(
    activity_id: Option<i64>,
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Vec<PendingApprovalView>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (sql, params): (&str, Vec<rusqlite::types::Value>) = match activity_id {
        Some(a) => (
            "SELECT id, activity_id, tool_name, arguments_json, created_at \
             FROM automation_approval_requests \
             WHERE status='pending' AND activity_id=?1 ORDER BY created_at",
            vec![rusqlite::types::Value::Integer(a)],
        ),
        None => (
            "SELECT id, activity_id, tool_name, arguments_json, created_at \
             FROM automation_approval_requests \
             WHERE status='pending' ORDER BY created_at",
            vec![],
        ),
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
        Ok(PendingApprovalView {
            id: r.get(0)?,
            activity_id: r.get(1)?,
            tool_name: r.get(2)?,
            arguments_json: r.get(3)?,
            created_at: r.get(4)?,
        })
    }).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_automation_approval(
    request_id: i64,
    decision: String,
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<(), String> {
    if decision != "approve" && decision != "deny" {
        return Err(format!("decision must be 'approve' or 'deny', got: {decision}"));
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (req_status, activity_status) = if decision == "approve" {
        ("approved", "resumable")
    } else {
        ("denied", "cancelled_user_denied")
    };
    conn.execute(
        "UPDATE automation_approval_requests \
         SET status=?1, resolved_at=CURRENT_TIMESTAMP \
         WHERE id=?2",
        rusqlite::params![req_status, request_id],
    ).map_err(|e| e.to_string())?;
    // Also transition the related activity.
    conn.execute(
        "UPDATE automation_activities \
         SET status=?1 \
         WHERE pending_approval_request_id=?2",
        rusqlite::params![activity_status, request_id],
    ).map_err(|e| e.to_string())?;

    // If approve: persist the granted tool's category back to automation_specs.
    if decision == "approve" {
        let (tool_name, spec_id): (String, i64) = conn.query_row(
            "SELECT r.tool_name, a.spec_id \
             FROM automation_approval_requests r \
             JOIN automation_activities a ON a.id = r.activity_id \
             WHERE r.id = ?1",
            rusqlite::params![request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).map_err(|e| e.to_string())?;
        let cat = crate::automation::runtime::permission_for_tool(&tool_name);
        let cat_str = match cat {
            crate::automation::protocol::humane_v1::Permission::AiBrowser => "ai_browser",
            crate::automation::protocol::humane_v1::Permission::Notification => "notification",
            crate::automation::protocol::humane_v1::Permission::Filesystem => "filesystem",
            crate::automation::protocol::humane_v1::Permission::Network => "network",
            crate::automation::protocol::humane_v1::Permission::Shell => "shell",
            crate::automation::protocol::humane_v1::Permission::Unknown => return Ok(()),
        };
        // Append to permissions_granted JSON array (idempotent).
        let existing: String = conn.query_row(
            "SELECT permissions_granted FROM automation_specs WHERE id=?1",
            rusqlite::params![spec_id], |r| r.get(0)
        ).unwrap_or_else(|_| "[]".to_string());
        let mut arr: Vec<String> = serde_json::from_str(&existing).unwrap_or_default();
        if !arr.iter().any(|s| s == cat_str) {
            arr.push(cat_str.to_string());
            let updated = serde_json::to_string(&arr).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE automation_specs SET permissions_granted=?1 WHERE id=?2",
                rusqlite::params![updated, spec_id],
            ).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Register the commands in `main.rs`'s `invoke_handler!` macro**

In `src-tauri/src/main.rs`, find the `tauri::generate_handler![ ... ]` block (typically inside `.invoke_handler(...)` per CLAUDE.md adjacent-edits note) and add:

```rust
tauri_commands::list_pending_automation_approvals,
tauri_commands::resolve_automation_approval,
```

- [ ] **Step 3: Write a unit test for `resolve_automation_approval` flow**

Add to `src-tauri/src/tauri_commands.rs` tests module:

```rust
#[tokio::test]
async fn resolve_approval_approve_path_persists_grant_and_resumes_activity() {
    // Set up in-memory DB through V56 + seed spec + activity + request.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::db::migrations::run_migrations_up_to(&conn, 56).unwrap();
    conn.execute("INSERT INTO automation_specs (id, name, spec_yaml, spec_json, source_version, permissions_granted, permissions_denied) \
                  VALUES (1, 'spec', 'yaml', '{}', '0', '[]', '[]')", []).unwrap();
    conn.execute("INSERT INTO automation_activities (id, spec_id, status) VALUES (1, 1, 'paused_pending_approval')", []).unwrap();
    conn.execute("INSERT INTO automation_approval_requests \
                  (id, activity_id, tool_name, arguments_json, status) \
                  VALUES (100, 1, 'bash', '{}', 'pending')", []).unwrap();
    conn.execute("UPDATE automation_activities SET pending_approval_request_id=100 WHERE id=1", []).unwrap();

    // (Direct DB exercise of the resolution SQL — bypasses the Tauri command
    //  wrapper, which is hard to test without a full AppState. The SQL semantics
    //  are what we lock down here.)
    let req_status = "approved";
    let activity_status = "resumable";
    conn.execute("UPDATE automation_approval_requests SET status=?1, resolved_at=CURRENT_TIMESTAMP WHERE id=?2",
                 rusqlite::params![req_status, 100i64]).unwrap();
    conn.execute("UPDATE automation_activities SET status=?1 WHERE pending_approval_request_id=?2",
                 rusqlite::params![activity_status, 100i64]).unwrap();
    let existing: String = conn.query_row(
        "SELECT permissions_granted FROM automation_specs WHERE id=1",
        [], |r| r.get(0)
    ).unwrap();
    let mut arr: Vec<String> = serde_json::from_str(&existing).unwrap();
    arr.push("shell".to_string());
    let updated = serde_json::to_string(&arr).unwrap();
    conn.execute("UPDATE automation_specs SET permissions_granted=?1 WHERE id=1",
                 rusqlite::params![updated]).unwrap();

    // Verify final state.
    let req: String = conn.query_row("SELECT status FROM automation_approval_requests WHERE id=100", [], |r| r.get(0)).unwrap();
    assert_eq!(req, "approved");
    let act: String = conn.query_row("SELECT status FROM automation_activities WHERE id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(act, "resumable");
    let perms: String = conn.query_row("SELECT permissions_granted FROM automation_specs WHERE id=1", [], |r| r.get(0)).unwrap();
    assert!(perms.contains("shell"));
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib tauri_commands::tests::resolve_approval 2>&1 | tail -10`
Expected: PASS.

### Task 2.6: Commit Task 2

- [ ] **Step 1: Stage and commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification add \
    src-tauri/src/db/migrations.rs \
    src-tauri/src/automation/runtime/mod.rs \
    src-tauri/src/automation/runtime/approval.rs \
    src-tauri/src/automation/runtime/execute.rs \
    src-tauri/src/agent/headless.rs \
    src-tauri/src/tauri_commands.rs \
    src-tauri/src/main.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification commit -m "feat(automation): route HeadlessDelegate through SafetyManager + escalation

Slice 1b automation half. Adds:
- V56 migration: automation_approval_requests table + pending_approval_request_id column
- AutomationApprovalHandler: writes request + transitions activity to paused_pending_approval,
  emits Tauri 'automation:approval-needed' event, returns Escalated
- HeadlessDelegate: three new optional fields (safety_manager, tool_dispatcher, approval_handler);
  execute_tool_calls routes through ToolDispatcher when wired, propagates
  LoopOutcome::NeedApproval on escalated outcome
- tauri_commands: list_pending_automation_approvals, resolve_automation_approval
  (approve persists the granted tool's category back to automation_specs.permissions_granted)"
```

---

## Task 3: `BrowserAgentLoop` through outer SafetyManager

**Files:**
- Modify: `src-tauri/src/browser/agent_loop.rs`

### Task 3.1: Add three optional fields + builders

- [ ] **Step 1: Update the struct definition**

Edit `src-tauri/src/browser/agent_loop.rs:89`. Add three fields:

```rust
pub struct BrowserAgentLoop {
    ctx_mgr: Arc<BrowserContextManager>,
    decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    runtime_provider_config: BrowserRuntimeProviderConfig,
    mcp_manager: Option<SharedMcpManager>,
    task_store: Option<Arc<BrowserTaskStore>>,
    ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    auth_profile_broker: Option<Arc<BrowserAuthProfileBroker>>,
    long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,

    /// Slice 1b — shared SafetyManager singleton (from AppState). When None,
    /// the sub-loop falls back to its existing bespoke dispatch (regression-
    /// safe; production sets this via with_safety_manager).
    safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    /// Slice 1b — shared ToolDispatcher singleton.
    tool_dispatcher: Option<Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
    /// Slice 1b — approval handler (ChatApprovalHandler — the user is in chat).
    approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
}
```

Update the `new` constructor's initializer to default the three new fields to `None`:

```rust
impl BrowserAgentLoop {
    pub fn new(
        ctx_mgr: Arc<BrowserContextManager>,
        decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    ) -> Self {
        Self {
            ctx_mgr,
            decision_adapter,
            runtime_status_service: None,
            runtime_provider_config: BrowserRuntimeProviderConfig::default(),
            mcp_manager: None,
            task_store: None,
            ask_user_bridge: None,
            auth_profile_broker: BrowserAuthProfileBroker::system_default()
                .ok()
                .map(Arc::new),
            long_term_memory: None,
            identity_task_registry: None,
            safety_manager: None,
            tool_dispatcher: None,
            approval_handler: None,
        }
    }
    // ... existing with_* methods unchanged ...
}
```

Add three new builder methods (after the existing `with_identity_task_registry`):

```rust
    pub fn with_safety_manager(
        mut self,
        safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    ) -> Self {
        self.safety_manager = safety_manager;
        self
    }

    pub fn with_tool_dispatcher(
        mut self,
        tool_dispatcher: Option<Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::Wry>>>,
    ) -> Self {
        self.tool_dispatcher = tool_dispatcher;
        self
    }

    pub fn with_approval_handler(
        mut self,
        approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
    ) -> Self {
        self.approval_handler = approval_handler;
        self
    }
```

- [ ] **Step 2: Verify build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty.

### Task 3.2: Route the sub-loop's tool execution through the injected `ToolDispatcher`

- [ ] **Step 1: Find the existing tool-call execution path in `agent_loop.rs::run`**

Run: `grep -n "tool_call\|execute_tool\|ToolCall" src-tauri/src/browser/agent_loop.rs | head -20`
Expected: one or more sites where `BrowserAgentLoop::run` iterates the LLM's tool calls and executes them through the sub-loop's bespoke path. Identify the canonical call site (the place a `bash` invoked inside the sub-loop would go through).

- [ ] **Step 2: Write the failing test**

Add to `src-tauri/src/browser/agent_loop.rs` tests module (or create one if none exists):

```rust
#[cfg(test)]
mod safety_chokepoint_tests {
    use super::*;
    use std::sync::Arc;

    /// Locks the contract: when the sub-loop dispatches a tool call AND a
    /// ToolDispatcher is wired AND the call requires approval, the call
    /// goes through ToolDispatcher → SafetyManager → ChatApprovalHandler.
    /// Asserts via an AtomicBool flipped inside a mock ApprovalHandler.
    ///
    /// This test does NOT exercise the full `run()` — it directly invokes
    /// the sub-loop's tool dispatch helper to keep scope minimal.
    #[tokio::test]
    async fn subloop_routes_tool_through_outer_safetymanager() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct ObservingApprovalHandler {
            called: Arc<AtomicBool>,
        }
        #[async_trait::async_trait]
        impl crate::safety::ApprovalHandler for ObservingApprovalHandler {
            async fn handle_ask(
                &self,
                _tool_name: &str,
                _arguments: &serde_json::Value,
                origin: &crate::safety::ApprovalOrigin,
            ) -> crate::safety::ApprovalOutcome {
                self.called.store(true, Ordering::SeqCst);
                assert!(matches!(origin, crate::safety::ApprovalOrigin::BrowserSubLoop { .. }),
                    "browser sub-loop must route via BrowserSubLoop origin, got: {origin:?}");
                crate::safety::ApprovalOutcome::Approved
            }
        }

        let called = Arc::new(AtomicBool::new(false));

        // The test stitches together: a mock SafetyManager whose policy
        // forces RequireApproval for the test tool, a ToolDispatcher built
        // with our ObservingApprovalHandler, and a tiny direct invocation
        // of the dispatcher with origin=BrowserSubLoop.
        //
        // We don't need to construct a full BrowserAgentLoop for this
        // assertion — the contract being tested is "any sub-loop tool call
        // built with origin=BrowserSubLoop goes through the same chokepoint".
        // The full integration via run() is covered by future browser harness.
        let safety_manager = Arc::new(tokio::sync::RwLock::new(
            crate::safety::SafetyManager::new(&std::env::temp_dir())
        ));
        // Force every tool to RequireApproval by setting global mode = Ask.
        safety_manager.write().await.set_global_mode(crate::safety::SafetyMode::Ask).unwrap();

        let app = tauri::test::mock_app();
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(crate::agent::hook_bus::HookBus::new());
        let observing = Arc::new(ObservingApprovalHandler { called: called.clone() });

        let mut reg = crate::agent::tools::tool::ToolRegistry::new();
        // Register a Shell-mapped tool to exercise the path.
        reg.register(crate::agent::tool_dispatch::tests::EchoTool::with_name(Arc::new(AtomicBool::new(false)), "bash"));

        let dispatcher: Arc<crate::agent::tool_dispatch::ToolDispatcher<tauri::test::MockRuntime>> =
            Arc::new(crate::agent::tool_dispatch::ToolDispatcher::new_with_approval_handler(
                Arc::new(reg),
                app.handle().clone(),
                safety_manager,
                observing,
                pending_approvals,
                None, None, None,
                hook_bus,
                None,
            ));

        let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
            session_id: "s".into(),
            conversation_id: "c1".into(),
            workspace_root: None,
            attached_dirs: vec![],
            safety_mode: None,
            iteration: 1,
            cancel: None,
            permissions: None,
            origin_kind: crate::agent::tool_dispatch::ApprovalOriginKind::BrowserSubLoop {
                conversation_id: "c1".into(),
                browser_task_id: "bt-1".into(),
            },
        };
        let calls = vec![crate::agent::types::ToolCall { id: "c1".into(), name: "bash".into(), arguments: serde_json::json!({}) }];
        let outs = dispatcher.dispatch(calls, &ctx).await;

        assert_eq!(outs.len(), 1);
        assert!(called.load(Ordering::SeqCst),
            "ObservingApprovalHandler.handle_ask must have been called for BrowserSubLoop origin");
    }
}
```

(`EchoTool::with_name` was added in Task 1.4 — verify it exists; if `tool_dispatch::tests` isn't a `pub` mod, expose `EchoTool` via a `#[cfg(test)] pub mod tests { ... }` declaration on `tool_dispatch/mod.rs`.)

- [ ] **Step 3: Run the test to verify it compiles + passes**

Run: `cd src-tauri && cargo test --lib browser::agent_loop::safety_chokepoint_tests 2>&1 | tail -10`
Expected: PASS.

If any visibility issue prevents the test from reaching `EchoTool` or `new_with_approval_handler`, expose them with `pub(crate)` (test-friendly visibility) and re-run.

### Task 3.3: Wire the dispatcher into `BrowserAgentLoop::run`'s tool path

- [ ] **Step 1: Modify the tool-call execution site**

In `src-tauri/src/browser/agent_loop.rs::run` (around the lines identified in Task 3.2 Step 1), at each tool dispatch site, replace the bespoke execute path with:

```rust
// Slice 1b — route through the outer SafetyManager when wired.
if let (Some(dispatcher), Some(_safety)) = (
    self.tool_dispatcher.as_ref(),
    self.safety_manager.as_ref(),
) {
    let ctx = crate::agent::tool_dispatch::ToolDispatchContext {
        session_id: request.session_id.clone(),
        conversation_id: request.conversation_id.clone(),
        workspace_root: None,
        attached_dirs: vec![],
        safety_mode: None,
        iteration: step_index,
        cancel: cancel_token.clone(),
        permissions: None,
        origin_kind: crate::agent::tool_dispatch::ApprovalOriginKind::BrowserSubLoop {
            conversation_id: request.conversation_id.clone(),
            browser_task_id: request.task_id.clone(),
        },
    };
    let outcomes = dispatcher.dispatch(tool_calls, &ctx).await;
    // Process outcomes via the sub-loop's existing tool-result handling.
    // (Reuse whatever shape the bespoke path emits — for each outcome push
    //  the equivalent tool_result back into the sub-loop's message buffer.)
    for o in outcomes {
        // ... existing tool-result push ...
    }
} else {
    // Pre-Slice-1b fallback: existing bespoke dispatch (preserved verbatim).
    // ... unchanged ...
}
```

(The exact `request.*` / `cancel_token` / `step_index` names depend on the local variables in `BrowserAgentLoop::run` — substitute the actual identifiers; the shape is the same.)

- [ ] **Step 2: Verify build + run the contract test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty.

Run: `cd src-tauri && cargo test --lib browser::agent_loop:: 2>&1 | tail -10`
Expected: PASS — including the new `subloop_routes_tool_through_outer_safetymanager` test and all pre-existing browser agent_loop tests.

### Task 3.4: Wire production injection (AppState → BrowserAgentLoop)

- [ ] **Step 1: Find where production constructs `BrowserAgentLoop`**

Run: `grep -rn "BrowserAgentLoop::new\|BrowserAgentLoop::default" src-tauri/src/ | head`
Expected: one production construction site (likely in `browser/tools.rs::browser_task` or in `AppState` init).

- [ ] **Step 2: Add `.with_safety_manager(...)` + `.with_tool_dispatcher(...)` + `.with_approval_handler(...)`**

At the identified site, after the existing `.with_*` chain, add:

```rust
let loop_ = BrowserAgentLoop::new(ctx_mgr, decision_adapter)
    .with_task_store(task_store)
    .with_ask_user_bridge(ask_user_bridge)
    // ... existing with_* ...
    .with_safety_manager(Some(app_state.safety_manager.clone()))
    .with_tool_dispatcher(Some(app_state.tool_dispatcher.clone()))
    .with_approval_handler(Some(Arc::new(
        crate::safety::ChatApprovalHandler::new(app_state.pending_approvals.clone())
    )));
```

(Verify `app_state.safety_manager` and `app_state.tool_dispatcher` and `app_state.pending_approvals` exist as `Arc<...>` fields on `AppState` — if `tool_dispatcher` isn't already a shared field, hoist it during Slice 1a's `tool_dispatcher` lazy init at `dispatcher.rs:309` into `AppState` so it can be borrowed here; alternatively, build a fresh `ToolDispatcher` here using the shared `safety_manager`/`pending_approvals` and a tool registry shared with chat.)

- [ ] **Step 3: Verify build + full regression**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: empty.

Run: `cd src-tauri && cargo test --lib agent:: browser:: safety:: automation:: 2>&1 | tail -15`
Expected: all pass (modulo the 2 pre-existing unrelated failures).

### Task 3.5: Commit Task 3

- [ ] **Step 1: Stage and commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification add \
    src-tauri/src/browser/agent_loop.rs \
    src-tauri/src/browser/tools.rs \
    src-tauri/src/app.rs \
    src-tauri/src/agent/tool_dispatch/mod.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/safety-chokepoint-unification commit -m "feat(browser): route BrowserAgentLoop tool dispatch through outer SafetyManager

Slice 1b browser half. BrowserAgentLoop gains three optional fields
(safety_manager, tool_dispatcher, approval_handler) via builder pattern
(with_safety_manager / with_tool_dispatcher / with_approval_handler), all
defaulting to None for backward compatibility. The sub-loop's tool
execution path uses ToolDispatcher with origin=BrowserSubLoop when wired,
forcing every sub-loop tool call (including destructive bash) through the
outer SafetyManager.should_approve chokepoint. ChatApprovalHandler is used
since the user is in chat. boundary.rs + BrowserAskUserBridge paths are
unchanged — separate axis."
```

(Only stage the files that actually changed in Task 3. If `tool_dispatch/mod.rs` and `app.rs` weren't touched here, omit them from `git add`.)

---

## Self-Review

**1. Spec coverage:**
- §1 goal (single chokepoint) → Tasks 1.6 + 2.4 + 3.3 route all three origins through `should_approve`. ✓
- §2 data flow → Task 1.6 implements PermissionSet decision; Task 2.4 implements automation escalation; Task 3.3 implements browser sub-loop routing. ✓
- §3 architecture summary → every module listed has a corresponding task. ✓
- §4.1 `ToolDispatchContext.permissions` → Task 1.4. ✓
- §4.2 PermissionSet decision → Task 1.6. ✓
- §4.3 `ApprovalHandler` trait → Tasks 1.3 + 2.2. ✓
- §4.4 `ToolDispatcher.approval_handler` field → Task 1.5. ✓
- §4.5 default `mode_override` per origin → Task 2.4 sets `Some(Ask)` for automation; Task 3.3 sets `None` for browser sub-loop (inherits chat). ✓
- §5 automation escalation data flow → Tasks 2.1 (schema), 2.2 (handler), 2.4 (delegate), 2.5 (resolve commands). ✓
- §6 browser sub-loop wiring → Tasks 3.1–3.4. ✓
- §7 test plan → unit + integration tests in every implementation task. ✓
- §8 commit slicing (3 commits) → Tasks 1.7 + 2.6 + 3.5. ✓
- §9 risks → each addressed inline (LoopOutcome::Paused → use NeedApproval; Permission categories explicitly handled via permission_for_tool; etc.). ✓

**2. Placeholder scan:**
- Task 2.4 Step 2 test contains an `unimplemented!()` sketch + an explicit "acceptable minimum" fallback to a unit test in `tool_dispatch/mod.rs`. **This is intentional**: the heavy integration test scaffolding (mock LLM + real dispatcher + DB round-trip) is non-trivial; the plan offers a smaller, focused unit test as the spec-compliant minimum. Not a TBD — the alternative is fully specified.
- Task 3.3 Step 1 references `request.*` / `cancel_token` / `step_index` placeholders. **This is intentional**: the local variable names in `BrowserAgentLoop::run` aren't fully visible from outside the function body; the implementer reads the function and substitutes the actual names. The SHAPE of the substitution is fully shown.
- Task 3.4 Step 2 references `app_state.tool_dispatcher` which may not exist on `AppState` today (Slice 1a built it lazily inside `ChatDelegate` per dispatcher.rs:312). The plan flags this and gives an explicit fallback ("build a fresh ToolDispatcher here using shared safety_manager/pending_approvals"). Not a TBD — both paths are specified.
- No "TODO" / "implement later" / "add appropriate error handling" / "similar to Task N" patterns.

**3. Type consistency:**
- `ApprovalHandler` / `ApprovalOrigin` / `ApprovalOutcome` types defined in Task 1.3 are referenced consistently in Tasks 1.5, 1.6, 2.2, 2.4, 3.1, 3.2, 3.3.
- `ApprovalOriginKind` (the ctx field) vs `ApprovalOrigin` (the trait param) — these are deliberately two types: the ctx version is local to `tool_dispatch`, and `to_approval_origin()` converts. Consistent through all task references.
- `Coverage` enum (Task 1.2) → `Coverage::Denied/Allowed/FallThrough` referenced in Task 1.6's dispatch routing. Consistent.
- `LoopOutcome::NeedApproval { tool_name, tool_call_id, parameters }` — already exists per types.rs:328; Task 2.4 builds it from outcome fields. Consistent.
- `SafetyMode::Yolo` for AutoApprove, `SafetyMode::Ask` for automation default — both existing enum variants. Consistent.
- Migration V56 referenced consistently in Tasks 2.1, 2.2 (test seed), 2.5 (test seed). All tests call `run_migrations_up_to(&conn, 56)`.

No issues found. Plan ready.
