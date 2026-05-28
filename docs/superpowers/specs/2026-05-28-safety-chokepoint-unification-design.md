# Safety Chokepoint Unification (Slice 1b) Design

**Date:** 2026-05-28
**Status:** Draft for implementation planning (brainstormed via `superpowers:brainstorming`, 7 decisions resolved)
**Source slice:** `docs/superpowers/specs/2026-05-27-pi-convergence-gap-audit.md` §4 阶段 1 Slice 1b
**Strategic baseline:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md`
**Related code:** `src-tauri/src/safety/`, `src-tauri/src/agent/tool_dispatch/`, `src-tauri/src/agent/dispatcher.rs`, `src-tauri/src/automation/runtime/`, `src-tauri/src/browser/agent_loop.rs`, `src-tauri/src/browser/boundary.rs`
**Related ADR sections (carry forward from north-star, still in force):** §10 Safety/Policy Hook matrix, §17 risk register

---

## § 1 · Goal

Make `SafetyManager.should_approve` the **single per-call tool-gating chokepoint** for all three live tool-execution origins (chat, automation, browser sub-loop), closing audit §1.8 CRITICAL ("three independent safety models") together with audit §1.7 (browser nested loop entanglement). Closes the audit's阶段 1 — the safety/correctness gate for the multi-domain product target — without redesigning UX or eliminating the nested browser loop (both deferred to later slices).

### Reframe (foundational)

The three systems in audit §1.8 are **three layers of different concerns**, not three redundant implementations of the same concern. This slice unifies the **per-call chokepoint** layer only:

| Layer | Code | Concern | This slice |
|---|---|---|---|
| **A — declarative pre-authorization** | `automation::PermissionSet` (`automation/runtime/mod.rs:21`) | "What is this run *upfront* allowed to do?" — YAML spec + DB grants/denies | KEEP, integrate as overlay |
| **B — per-call gating chokepoint** | `safety::SafetyManager.should_approve` (`safety/mod.rs:205`) | "Should this specific tool call proceed *right now*?" — interactive decision | **THIS SLICE: route all 3 origins through it** |
| **C — domain user-attention signal** | `browser::BrowserBoundaryKind` (`browser/boundary.rs:5`) | "Does this page state need a human?" — login wall / CAPTCHA / payment etc. | KEEP, unchanged — different axis from B |

What "unification" means here: **the entry point** to per-call gating. Not the vocabularies of A and C, which are domain-correct as-is.

---

## § 2 · Three-origin data flow (target state)

```
┌──────────────────────────────────────────────────────────────────┐
│  ① chat path (unchanged behavior, refactored internals)          │
│   user → ChatDelegate → ToolDispatcher → SafetyManager  ─┐        │
│                                          (singleton)     │        │
├──────────────────────────────────────────────────────────┼────────┤
│  ② automation path (NEW: routed through SafetyManager)   │        │
│   spec.yaml + DB grants ─→ PermissionSet  (Layer A)      │        │
│   HeadlessDelegate → ToolDispatcher → SafetyManager ←────┤        │
│      ctx.permissions = Some(PermissionSet)  ──┐          │        │
│      (covered tools auto-approve; uncovered → │          │        │
│       AutomationApprovalHandler → activity_   │          │        │
│       pause + DB approval-request)            │          │        │
├──────────────────────────────────────────────────────────┼────────┤
│  ③ browser sub-loop (NEW: shares outer SafetyManager)    │        │
│   browser_task → BrowserAgentLoop → ToolDispatcher ──────┘        │
│                  (same SafetyManager instance from app state)     │
│                  (ChatApprovalHandler — user is in chat)          │
│                                                                   │
│   boundary.rs (Layer C) → BrowserAskUserBridge (UNCHANGED)        │
│   — separate axis, separate IPC                                   │
└──────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                  **single should_approve chokepoint**
```

---

## § 3 · Architecture change summary

| Module | Change | Nature |
|---|---|---|
| `src-tauri/src/safety/approval.rs` (new) | Define `ApprovalHandler` trait + `ApprovalOrigin`/`ApprovalOutcome` enums; provide `ChatApprovalHandler` wrapping today's `PendingApprovals` | Additive (1 new file, 1 new abstraction — the only new abstraction in this slice) |
| `src-tauri/src/agent/tool_dispatch/mod.rs` | `ToolDispatchContext` gains `permissions: Option<PermissionSet>`; `ToolDispatcher.pending_approvals` field replaced by `approval_handler: Arc<dyn ApprovalHandler>`; `run_one` resolves PermissionSet → Block/AutoApprove/FallThrough before `should_approve` | Sweeping but mechanical (same pattern as Slice 1a's `cancel` field) |
| `src-tauri/src/automation/runtime/approval.rs` (new) | `AutomationApprovalHandler` implementation that writes DB + transitions activity + returns `Escalated` | Additive |
| `src-tauri/src/automation/runtime/execute.rs` (`HeadlessDelegate`) | Inject `safety_manager`, `tool_dispatcher`, `automation_approval_handler`; retire bespoke tool dispatch; handle `Escalated`-shaped terminate outcome → `LoopOutcome::Paused` | Medium-blast refactor |
| `src-tauri/src/browser/agent_loop.rs` (`BrowserAgentLoop`) | Inject `safety_manager`, `tool_dispatcher`, `approval_handler` (the `ChatApprovalHandler` instance); retire bespoke sub-loop tool dispatch; `boundary.rs` path unchanged | Medium-blast refactor |
| `src-tauri/src/db/migrations.rs` | New migration **V56** (next free; re-verify against `CONTEXT.md` active migration registry at implementation time): create `automation_approval_requests` table + add `pending_approval_request_id INTEGER NULL` column to `automation_activities` + recognize new status enum values `'paused_pending_approval'`, `'cancelled_user_denied'` | Single small migration |
| `src-tauri/src/tauri_commands.rs` | Two new commands: `list_pending_automation_approvals(activity_id?)`, `resolve_automation_approval(request_id, decision: 'approve'\|'deny')` | Additive |

---

## § 4 · Core contracts

### 4.1 `ToolDispatchContext.permissions: Option<PermissionSet>`

Pattern mirrors Slice 1a's `cancel` field. Automation passes `Some(perms)`; chat & browser sub-loop pass `None`. Compiler enforces all existing literals add `permissions: None`.

### 4.2 PermissionSet decision (in `dispatch.run_one`, before `should_approve`)

```rust
enum PermDecision { Block, AutoApprove, FallThrough }

fn resolve_permissions(perms: Option<&PermissionSet>, tool_name: &str) -> PermDecision {
    let Some(p) = perms else { return PermDecision::FallThrough };
    if p.denied.iter().any(|d| d.matches(tool_name)) {
        PermDecision::Block
    } else if p.spec.iter().chain(p.granted.iter()).any(|g| g.matches(tool_name)) {
        PermDecision::AutoApprove
    } else {
        PermDecision::FallThrough
    }
}
```

Then:

- `Block` → construct a `denied_outcome` (similar shape to Slice 1a's `cancelled_outcome` — `is_error=true`, `message_content="Error: tool denied by spec"`, `terminate=false` — the loop continues to the next tool result so the model can react).
- `AutoApprove` → call `should_approve` with `mode_override = Some(&SafetyMode::Yolo)`. Reuses existing logic; no new mode.
- `FallThrough` → call `should_approve` with the dispatcher's default `mode_override` (for automation: `Some(&SafetyMode::Ask)` — see §4.4).

`Permission::matches(tool_name)` is **exact match by tool name** in this slice; finer-grained parameter matching is a Layer A internal evolution and out of scope.

### 4.3 `ApprovalHandler` trait (the only new abstraction)

```rust
// src-tauri/src/safety/approval.rs (new file)

#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    /// Called when `should_approve` returns `Ask`. The handler decides what
    /// to do given the origin: block-and-prompt (chat), escalate-and-pause
    /// (automation), etc.
    async fn handle_ask(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome;
}

pub enum ApprovalOrigin {
    Chat { conversation_id: String },
    Automation { activity_id: String },
    BrowserSubLoop { conversation_id: String, browser_task_id: String },
}

pub enum ApprovalOutcome {
    Approved,    // synchronous answer (user clicked approve, or auto-policy)
    Denied,      // synchronous decline
    Escalated,   // asynchronous; caller must pause/checkpoint, user resolves later
}

// Default implementation wrapping today's PendingApprovals — for chat AND
// browser sub-loop (user is in the chat session either way).
pub struct ChatApprovalHandler {
    pending_approvals: Arc<crate::app::PendingApprovals>,
}

impl ChatApprovalHandler {
    pub fn new(pending_approvals: Arc<crate::app::PendingApprovals>) -> Self {
        Self { pending_approvals }
    }
}

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler { /* ... wraps PendingApprovals.await_decision(...) ... */ }
```

### 4.4 `ToolDispatcher` adjustment

- Field rename: `pending_approvals: Arc<PendingApprovals>` → `approval_handler: Arc<dyn ApprovalHandler>`. Default constructor wraps existing `PendingApprovals` in `ChatApprovalHandler` — byte-equivalent behavior for chat path.
- `dispatch.run_one` after PermissionSet decision: if `should_approve` returns `Ask`, call `approval_handler.handle_ask(name, args, &origin)`:
  - `Approved` → proceed to tool execute (existing path).
  - `Denied` → construct rejected outcome with `message_content="Error: tool denied by user"`.
  - `Escalated` → construct `escalated_outcome` (similar shape to `cancelled_outcome` — `is_error=true`, `message_content="Error: awaiting user approval"`, **`terminate=true`** to signal the loop should pause).
- The `origin: ApprovalOrigin` is constructed from `ToolDispatchContext` fields (the dispatcher knows its own context kind via a new `origin_kind: ApprovalOriginKind` ctx field, e.g. enum `Chat | Automation | BrowserSubLoop`).

### 4.5 Default `mode_override` per origin

- Chat: `None` (use global `SafetyMode` from policy — default `Supervised`).
- Automation: `Some(SafetyMode::Ask)` — every uncovered tool prompts; combined with PermissionSet AutoApprove overlay this means "only PermissionSet-covered tools auto-run; anything else escalates."
- Browser sub-loop: `None` (inherits chat's effective mode — same user, same conversation context).

---

## § 5 · Automation escalation data flow

```
1. spec.yaml + DB grants → PermissionSet
2. HeadlessDelegate runs run_agentic_loop; each tool call enters ToolDispatcher
3. dispatch.run_one:
   ┌─ PermissionSet check ─┬─ Block       → rejected outcome (terminate=false)
   │                       ├─ AutoApprove → should_approve(mode=Yolo) → AutoApprove
   │                       └─ FallThrough → should_approve(mode=Ask)  → Ask
   └─ Ask → AutomationApprovalHandler.handle_ask:
            a) INSERT INTO automation_approval_requests
               (id, activity_id, tool_name, arguments_json,
                status='pending', created_at)
            b) UPDATE automation_activities
               SET status='paused_pending_approval',
                   pending_approval_request_id = <new_id>
            c) emit Tauri event 'automation:approval-needed'
               { activity_id, request_id, tool_name }
            d) return ApprovalOutcome::Escalated
4. dispatcher: Escalated → escalated_outcome (terminate=true)
5. HeadlessDelegate sees Some(LoopOutcome) signal via terminate path → exits loop
   with LoopOutcome::Paused (or a status-bearing variant — see §9 risks)
6. Worker process exits gracefully; activity persists in 'paused_pending_approval'
7. User resolves via UI (tauri_commands.rs::resolve_automation_approval):
   ┌─ approve: automation_approval_requests.status='approved', resolved_at=now;
   │          automation_specs.permissions_granted += tool_name
   │              (spec-level — persists across future runs of this spec);
   │          automation_activities.status='resumable'
   └─ deny:    automation_approval_requests.status='denied', resolved_at=now;
              automation_activities.status='cancelled_user_denied'
   (Note: grant is spec-level per the existing `PermissionSet` model where
   `granted` lives in `automation_specs.permissions_granted`. If per-activity
   one-shot grants are desired, that's a Permission model extension — out of
   scope for Slice 1b; flag during spec review.)
8. User clicks "Resume" → re-runs HeadlessDelegate; this time PermissionSet
   includes the new grant → AutoApprove → tool runs.
```

### 5.1 New DB schema (migration V56)

```sql
CREATE TABLE automation_approval_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id INTEGER NOT NULL REFERENCES automation_activities(id),
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending | approved | denied
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMP NULL
);
CREATE INDEX idx_aar_activity_status ON automation_approval_requests(activity_id, status);

ALTER TABLE automation_activities
    ADD COLUMN pending_approval_request_id INTEGER NULL
        REFERENCES automation_approval_requests(id);

-- Status values extended (string-typed column, no enum migration needed):
--   existing: pending, running, paused, completed, failed, cancelled
--   new:      paused_pending_approval, cancelled_user_denied, resumable
```

### 5.2 New Tauri commands

```rust
#[tauri::command]
async fn list_pending_automation_approvals(
    activity_id: Option<i64>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PendingApprovalView>, String>;

#[tauri::command]
async fn resolve_automation_approval(
    request_id: i64,
    decision: String,  // 'approve' | 'deny'
    state: tauri::State<'_, AppState>,
) -> Result<(), String>;
```

UI wiring of these commands is out of scope for Slice 1b (the commands exist + are testable via direct invoke; UI surface is a follow-up).

---

## § 6 · Browser sub-loop wiring

`BrowserAgentLoop::new` adds three fields, all sourced from the outer `ChatDelegate`'s app state (singleton instances):

```rust
pub struct BrowserAgentLoop<R: tauri::Runtime> {
    // ... existing fields ...
    safety_manager: Arc<RwLock<SafetyManager>>,
    approval_handler: Arc<dyn ApprovalHandler>,  // ChatApprovalHandler — user is in chat
    tool_dispatcher: Arc<ToolDispatcher<R>>,     // shared with outer dispatcher
}
```

The sub-loop's existing bespoke tool execution path is retired; all tool calls go through `self.tool_dispatcher.dispatch(calls, ctx)`. The `ctx`:

- `permissions: None` (browser has no PermissionSet model — pre-authorization is "the user approved `browser_task` in chat" at the outer level)
- `origin_kind: ApprovalOriginKind::BrowserSubLoop { conversation_id, browser_task_id }`
- `safety_mode: None` (inherits global from policy)
- `cancel: <outer reason_ctx's token>` (chained from the outer loop — already present from Slice 1a)

`boundary.rs` and `BrowserAskUserBridge` are **untouched**. `BrowserBoundaryKind` events (LoginRequired/Captcha/Payment/Totp2fa/PrivacySensitive/AuthProfileStale) continue to fire through the existing browser-specific UX path. The two axes — "is this page state a user-attention event?" vs "is this tool call gated?" — are now cleanly separated, both going through clear (different) handlers.

---

## § 7 · Test plan

### 7.1 Unit tests

**`safety::approval` module:**
- `chat_approval_handler_blocks_until_response` — `ChatApprovalHandler` returns `Approved` when wrapped `PendingApprovals.await_decision` resolves true; returns `Denied` when false.

**`tool_dispatch::permissions_resolution`:**
- `permission_set_denied_blocks_tool` — `denied` list match → `PermDecision::Block` → outcome is rejected without invoking `should_approve` (mock observer verifies `should_approve` was not called).
- `permission_set_granted_auto_approves` — tool in `granted` (or `spec`), not in `denied` → `should_approve` called with `mode_override=Some(Yolo)` → outcome OK.
- `permission_set_uncovered_falls_through` — neither in `granted` nor `denied` → `should_approve` called with `mode_override=Some(Ask)` → routes to `approval_handler.handle_ask`.

**`automation::approval`:**
- `automation_approval_handler_escalates_and_writes_db` — set up in-memory SQLite, call `handle_ask(tool="bash", args=json!({"command":"ls"}), origin=Automation{activity_id:"act-1"})`, assert: (1) one row in `automation_approval_requests` with `status='pending'`, (2) `automation_activities` row has `status='paused_pending_approval'` and `pending_approval_request_id` set, (3) return value is `ApprovalOutcome::Escalated`.

### 7.2 Integration tests

- **`automation_run_with_partial_permissions`** — PermissionSet grants `read_file`, no grant for `bash`:
  - Build `HeadlessDelegate` with mock `LlmProvider` that emits `read_file` then `bash` tool calls.
  - Run `run_agentic_loop`.
  - Assert: `read_file` outcome `result.is_ok()`; `bash` outcome is escalated (`is_error=true`, `message_content="Error: awaiting user approval"`, `terminate=true`).
  - Assert: loop exits with `LoopOutcome::Paused`.
  - Assert: DB state — `automation_activities.status='paused_pending_approval'`, `pending_approval_request_id IS NOT NULL`; one row in `automation_approval_requests` with `status='pending'`, `tool_name='bash'`.

- **`automation_resume_after_approval`** — continuation:
  - Simulate user approval by calling `resolve_automation_approval(request_id, 'approve')`.
  - Assert: request row `status='approved'`, `resolved_at IS NOT NULL`; activity row `status='resumable'`; `permissions_granted` includes `bash`.
  - Re-run `HeadlessDelegate` from checkpoint.
  - Assert: `bash` AutoApproves this run + loop completes with `LoopOutcome::Completed`.

- **`browser_subloop_destructive_call_routes_through_safetymanager`** — `BrowserAgentLoop` mock:
  - Inject mock `SafetyManager` + `ChatApprovalHandler` wrapping mock `PendingApprovals` (configured to immediately approve).
  - Sub-loop's mock LLM emits a `bash` tool call with a destructive command (e.g., `rm -rf /tmp/x`).
  - Assert (via mock observer): `should_approve` called exactly once with `tool_name="bash"`.
  - Assert: `ChatApprovalHandler.handle_ask` called exactly once with `origin=BrowserSubLoop{..}`.
  - Assert: tool executes after approval; outcome OK.

- **`browser_boundary_signal_unchanged`** — regression:
  - Simulate CAPTCHA detection via existing boundary.rs entry point.
  - Assert: `BrowserAskUserBridge` invoked.
  - Assert (via mock observer): `SafetyManager.should_approve` **NOT** called. (Two axes stay separate.)

### 7.3 Regression

Every existing test in:
- `agent::dispatcher::tests::*`
- `agent::tool_dispatch::tests::*`
- `agent::agentic_loop::tests::*`
- `safety::*::tests::*`

must pass **byte-equivalent**. This is the hard backward-compatibility constraint. The `ChatApprovalHandler` wrap of `PendingApprovals` is the bridge that preserves chat behavior identically.

---

## § 8 · Commit slicing (bisectable)

Per `CLAUDE.md` "one branch per plan, one commit per plan task". 3 logical commits, each compiles + tests green:

### Commit 1 — `feat(safety): introduce ApprovalHandler + ToolDispatchContext.permissions`

- New file `src-tauri/src/safety/approval.rs` — trait + enums + `ChatApprovalHandler`.
- `safety/mod.rs` re-exports new types.
- `agent/tool_dispatch/mod.rs`:
  - `ToolDispatchContext.permissions: Option<PermissionSet>` (compiler enforces all literals update — same `cancel`-field pattern as Slice 1a).
  - `ToolDispatchContext.origin_kind: ApprovalOriginKind` (new enum).
  - `ToolDispatcher.pending_approvals` field → `approval_handler: Arc<dyn ApprovalHandler>`. All existing constructors wrap as `Arc::new(ChatApprovalHandler::new(pending_approvals))`.
  - `dispatch.run_one`: PermissionSet decision → branch (Block/AutoApprove/FallThrough); `should_approve` Ask → `approval_handler.handle_ask` → branch (Approved/Denied/Escalated → escalated_outcome with terminate=true).
- Tests: `chat_approval_handler_*` + `permission_set_*` unit tests; all existing `tool_dispatch::tests::*` pass byte-equivalent.

### Commit 2 — `feat(automation): route HeadlessDelegate through SafetyManager + escalation`

- Migration V56 (verify next-free against `CONTEXT.md` registry first).
- New `src-tauri/src/automation/runtime/approval.rs` — `AutomationApprovalHandler` impl.
- `HeadlessDelegate::new` adds `safety_manager`, `automation_approval_handler`, `tool_dispatcher` fields; constructed from `AppState`.
- `HeadlessDelegate::execute_tool_calls` retires bespoke dispatch, calls `self.tool_dispatcher.dispatch(calls, ctx)` with `ctx.permissions = Some(self.perms.clone())`, `ctx.origin_kind = Automation { activity_id }`, `ctx.safety_mode = Some(SafetyMode::Ask)`.
- `HeadlessDelegate` handles `terminate=true` escalated outcome → returns `LoopOutcome::Paused`.
- New `tauri_commands.rs` entries: `list_pending_automation_approvals`, `resolve_automation_approval`.
- Tests: `automation_approval_handler_escalates_and_writes_db` unit + `automation_run_with_partial_permissions` + `automation_resume_after_approval` integration.

### Commit 3 — `feat(browser): route BrowserAgentLoop tool dispatch through outer SafetyManager`

- `BrowserAgentLoop::new` adds `safety_manager`, `approval_handler` (ChatApprovalHandler instance — same singleton as outer chat), `tool_dispatcher` (shared `Arc` with outer `ChatDelegate`).
- Sub-loop's bespoke tool execution path retired; calls `tool_dispatcher.dispatch(calls, ctx)` with `ctx.permissions = None`, `ctx.origin_kind = BrowserSubLoop { conversation_id, browser_task_id }`.
- `boundary.rs` / `BrowserAskUserBridge` paths unchanged.
- Tests: `browser_subloop_destructive_call_routes_through_safetymanager` + `browser_boundary_signal_unchanged`.

(Optional Commit 4 — `chore` cleanup of any residual dead `PendingApprovals` direct uses after Commit 1's refactor; only if compiler shows dead-code warnings.)

---

## § 9 · Risks / Non-goals

### Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| `ApprovalHandler` trait introduces a new abstraction (violates Slice 1a's "no new abstractions" style) | Concept growth in the safety layer | Trait has exactly 1 method (`handle_ask`), 3 narrow implementations. The shape aligns with ADR §10's planned `PolicyHook` matrix — this is its precursor. Justified by the 3 origins genuinely needing different async behaviors. |
| `BrowserAgentLoop`'s bespoke dispatch may have subtle behaviors (streaming stdout, retry, timeout classification) not yet captured by `ToolDispatcher` | Behavior regression in browser sub-loop | Before Commit 3, audit all `select!`/`spawn`/per-call logic in current `BrowserAgentLoop` tool path; assert `ToolDispatcher` covers each (Slice 1a hardened ToolDispatcher with streaming + cancel + parallel — most should be covered). |
| `automation_activities.status` enum extension may break UI code that reads the column | Frontend rendering bugs | grep `automation_activities.status` UI references; provide string-typed fallback ("unknown status → render as 'paused'"). |
| Pending approval requests pile up if user never responds | Activity queue bloat | Out of scope for this slice; track as follow-up issue: "7-day-old pending request auto-cancel job." |
| `PermissionSet::matches()` semantics underspecified (prefix? glob? exact?) | Subtle authorization bugs | This slice: **exact `tool_name` match only**. Permission's finer-grained argument filtering is Layer A internal evolution — independent slice. |
| `LoopOutcome::Paused` variant doesn't exist today; need to add | Type plumbing | Either add the variant (`LoopOutcome::Paused { reason: PauseReason }`) or reuse existing `Stopped` with a status field via `ReasoningContext.thread_state`. Implementation plan picks the lower-friction option. |
| `ApprovalOriginKind` ctx field touches every `ToolDispatchContext` literal | Mechanical churn (lots of compiler errors during migration) | Same compiler-driven mechanical updates as Slice 1a's `cancel` field — well-understood, low-risk; all tests + literals updated in Commit 1. |

### Non-goals

- **Approval IPC unification** — chat banner, browser modal, automation inbox stay separate. The chokepoint unification is at the **decision** layer (`should_approve`), not the **presentation** layer.
- **Eliminating the nested browser loop** (audit §1.7 full close) — `BrowserAgentLoop` remains an intentional sub-agent; only its tool gating is routed through the outer SafetyManager. Sub-agent elimination is a Pi multi-domain / WorkerRole slice.
- **Per-call argument-level PermissionSet matching** — e.g., "allow `bash` but deny any command containing `rm`" — Layer A stays coarse-grained (tool name). Argument-level risk gating lives in `SafetyManager.assess_command_risk` (already exists) and `blocked_patterns`, independent of PermissionSet.
- **Cross-origin SafetyMode sharing** — SafetyMode remains per-conversation/per-call. Automation gets `Ask` as default override; not a new persistent mode.
- **Approval UX redesign** — chat's existing modal, browser's existing bridge, automation's new pending list — all rendered separately, no unified inbox in this slice.
- **Status-driven scheduling** — `'resumable'` activity status is a state marker only; a "background auto-resume worker" that picks up resumable activities is out of scope (user manually clicks Resume).

---

## § 10 · Implementation handoff

After spec approval, invoke `superpowers:writing-plans` to produce a bisectable TDD plan over the 3 commits in §8. Plan-time pre-flight per CLAUDE.md (gitnexus_impact on each touched symbol + verify file:line — gitnexus index requires `npx gitnexus analyze` refresh; we've been on a stale snapshot since `2131366`). High blast radius → full subagent-driven-development with per-task spec-review + code-quality-review.
