# PR10 Ambient To Automation Mapping Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` task-by-task. Keep Rust tests in sibling `*_tests.rs` files.

**Goal:** Encode the jcode ambient-mode lessons as a uClaw automation/scheduled-worker mapping contract, without importing jcode's ambient runner, second scheduler, JSON state store, or ambient-only permission path.

**Architecture:** Add a pure Rust module under `src-tauri/src/automation/` that maps ambient work concepts onto existing uClaw primitives: `IntentOrigin::Automation/System`, `AutonomyLevel::ScheduledWorker`, `Subscription::Schedule`, `TriggerSource::Schedule`, `TaskEventSource::Automation`, explicit heartbeat/progress receipts, gbrain memory receipts, and human-boundary permission context.

**Tech Stack:** Rust, existing automation protocol/activity/runtime-contract types, sibling Rust tests, existing GitNexus workflow.

---

## Scope Anchors

- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr10-ambient-automation`
- Branch: `codex/agent-os-jcode-pr10-ambient-automation`
- Source docs:
  - `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
  - `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
  - `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`
  - `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
  - `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`
- jcode reference:
  - `/Users/ryanliu/Documents/jcode/docs/AMBIENT_MODE.md`
  - `/Users/ryanliu/Documents/jcode/src/ambient.rs`
  - `/Users/ryanliu/Documents/jcode/src/ambient/runner.rs`
  - `/Users/ryanliu/Documents/jcode/src/ambient/scheduler.rs`
  - `/Users/ryanliu/Documents/jcode/src/tool/ambient.rs`
  - `/Users/ryanliu/Documents/jcode/src/safety.rs`

## ADR Section 18 Answers

| Question | PR10 Answer |
|---|---|
| 1. What user intent does this support? | Users can schedule or receive background agent work without a hidden second control plane; agents can classify jcode-style ambient directives as uClaw automation/scheduled-worker work. |
| 2. What autonomy level can it run at? | L4 `ScheduledWorker` by default. Internal maintenance may use `IntentOrigin::System`, but the autonomy target remains bounded by risk policy and human-boundary yields. |
| 3. What is the source of truth? | Existing automation specs, schedule subscriptions, automation activity ledger, TaskEvent rollout traces, and gbrain receipts. No ambient JSON files or ambient session registry become canonical truth. |
| 4. Which TaskEvent does it emit? | PR10 emits none at runtime. The mapping requires future runs to use `TaskEventSource::Automation` and preserve `task_started`, `checkpoint`, `boundary_yield`, `memory_write`, `warning`, and `task_finished` semantics. |
| 5. What context does it read? | None in PR10. Future wiring may read automation spec config, active-user state, token/cost budget state, provider backoff, and gbrain receipts. |
| 6. What capability does it require? | Schedule/automation contract evaluation only. Future execution remains governed by automation permissions and capability mesh. |
| 7. Which policy hooks can block it? | User-active pause, token/cost headroom, provider rate-limit backoff, automation permissions, `request_escalation`, and `BoundaryYield` can block or defer execution. |
| 8. What world projection does the UI render? | Future UI should render scheduled-worker status, next wake, paused reason, queued directive count, heartbeat/progress receipt, permission boundary, and durable memory receipt status. |
| 9. What harness cases prove it works? | Model-free tests for schedule mapping, trigger/source provenance, no second scheduler, active-session soft-interrupt classification, permission review context, heartbeat requirements, and gbrain receipt requirements. |
| 10. What is the rollback path? | Remove `automation/ambient_mapping.rs`, `automation/ambient_mapping_tests.rs`, the module export, this plan, and status-ledger edits. Existing runtime behavior remains unchanged. |
| 11. What does this not own? | No ambient runner import, no DB migration, no `TriggerSource::Ambient`, no `ScheduleSource` change, no `AppRuntimeService` change, no heartbeat/proactive loop change, no Tauri command/UI wiring, and no permission bypass. |

## Numbering Note

`AGENT_OS_JCODE_UPGRADE_STATUS.md` is authoritative for this series: Ambient-to-automation mapping is PR-10. Older blueprint text may refer to later numbering.

## Allowed Files

- Create: `src-tauri/src/automation/ambient_mapping.rs`
- Create: `src-tauri/src/automation/ambient_mapping_tests.rs`
- Modify: `src-tauri/src/automation/mod.rs`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Create/modify: this plan file

## Explicit Non-Goals

- Do not create a second scheduler or ambient runner.
- Do not add `TriggerSource::Ambient`.
- Do not modify `src-tauri/src/automation/sources/schedule.rs`.
- Do not modify `src-tauri/src/automation/runtime/service.rs`.
- Do not modify `src-tauri/src/automation/permissions.rs`.
- Do not modify `src-tauri/src/agent/heartbeat.rs`.
- Do not modify `src-tauri/src/proactive/service.rs`.
- Do not modify `src-tauri/src/tauri_commands.rs`.
- Do not modify `src-tauri/src/db/migrations.rs`.
- Do not modify root `Cargo.toml`.
- Do not write to frozen `memory_graph`.

## Impact Notes

- `ScheduleSource`: LOW from GitNexus; PR10 does not edit it.
- `permissions::check`: MEDIUM from GitNexus and only tests direct callers in the stale index; PR10 does not edit it.
- `activity_to_events`: MEDIUM per explorer review; PR10 does not edit it.
- `run_agentic_loop`: HIGH per earlier roadmap work; PR10 avoids it.
- `AppRuntimeService::execute_run`: core runtime pipeline; PR10 avoids it even where stale index reports lower risk.
- `automation/mod.rs`: module declaration only.

## Task 1: Add Ambient Mapping Contract

**Files:**
- Create: `src-tauri/src/automation/ambient_mapping.rs`
- Create: `src-tauri/src/automation/ambient_mapping_tests.rs`
- Modify: `src-tauri/src/automation/mod.rs`

- [x] **Step 1: Write sibling tests first**

Tests should cover:

- External/background ambient work maps to `IntentOrigin::Automation`, `AutonomyLevel::ScheduledWorker`, `TaskEventSource::Automation`, and `TriggerSource::Schedule`.
- Internal maintenance work may map to `IntentOrigin::System` while still using scheduled-worker policy.
- Schedule mapping produces existing `Subscription::Schedule`, not a new ambient subscription type.
- Delivery mapping distinguishes active-session soft interrupt from queued scheduled worker and new automation run.
- Permission context requires rationale, planned steps, risks, rollback, expected outcome, and `BoundaryYield`.
- Policy carries user-active pause, token headroom reserve, rate-limit backoff, heartbeat receipt, and gbrain memory receipt requirements.
- Forbidden surfaces explicitly include second scheduler, ambient trigger source, ambient JSON canonical truth, ambient session registry, and permission bypass.

- [x] **Step 2: Add pure mapping module**

Define:

- `AmbientWorkKind`
- `AmbientDeliveryMode`
- `AmbientScheduleInput`
- `AmbientSchedulePolicy`
- `AmbientPermissionContext`
- `AmbientAutomationMapping`
- `AmbientMappingError`
- `ambient_to_automation_mapping(kind, schedule)`
- `classify_delivery(active_session, directive_due, spawn_requested)`
- `validate_ambient_mapping(mapping)`

Keep all inputs explicit and test-controlled. Do not read files, query DB, spawn tasks, or call runtime services.

- [x] **Step 3: Export module**

Add `pub mod ambient_mapping;` to `automation/mod.rs`.

## Task 2: Update Status Ledger

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [x] **Step 1: Mark PR10 in progress**

Set current phase to PR10 in progress, owner `Codex`, and record worktree/branch.

## Task 3: Verify And Commit

- [x] **Step 1: Run focused tests**

```bash
rustfmt --edition 2021 --check src-tauri/src/automation/ambient_mapping.rs src-tauri/src/automation/ambient_mapping_tests.rs
cargo test --manifest-path src-tauri/Cargo.toml --lib automation::ambient_mapping
```

- [x] **Step 2: Check staged scope**

```bash
git diff --cached --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr10-ambient-automation
```

- [x] **Step 3: Commit**

Commit body must include verification commands and expected output.
