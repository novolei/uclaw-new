# PR-2: ToolContext Adapter

Status: in progress
Branch: `codex/agent-os-jcode-pr2-tool-context`
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr2-tool-context`
Base: stacked on `codex/agent-os-jcode-pr1-plan` because PR-2 consumes PR-1 type crates.

## Goal

Introduce a jcode-inspired tool execution context adapter without changing
current tool behavior.

This PR deliberately does not rewrite uClaw tools into a new trait shape. The
first slice creates a stable context seam so later PRs can migrate tools one by
one toward richer runtime contracts, TaskEvent correlation, soft interrupts,
stdin/request-user bridges, and capability profiles.

## Guardrail

GitNexus impact reports `Tool` as HIGH impact with 28 direct implementers.
Therefore PR-2 must avoid changing `Tool::execute(params)` in this slice.

The adapter shape is:

```rust
ToolExecutionContext -> execute_tool_with_context(tool, params, &ctx) -> Tool::execute(params)
```

The helper defaults to the old behavior. Future PRs can add an override trait or
move selected tools to context-aware execution after tests prove the seam.

## Allowed Files

- `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-23-pr2-tool-context-adapter.md`
- `src-tauri/src/agent/tools/tool.rs`
- `src-tauri/src/agent/tools/tool_tests.rs`
- `src-tauri/src/agent/dispatcher.rs`
- `src-tauri/src/agent/headless.rs`

Avoid `tauri_commands.rs`, `db/migrations.rs`, `memory_graph`, frontend files,
and root workspace metadata.

## ADR Section 18 Answers

1. User intent: make tool execution more reliable, inspectable, and ready for
   long-running agent work without disrupting current tools.
2. Autonomy level: L1-L3 in this PR because it affects agent tool execution; no
   new autonomous behavior is introduced.
3. Canonical truth source: existing tool execution path remains canonical;
   `ToolExecutionContext` is a structured adapter view, not a second state
   store.
4. TaskEvent entries: PR-2 does not emit new TaskEvents yet. It maps the future
   event fields for `ToolCall`, `ToolResult`, `PermissionRequested`, and
   `PermissionDecided`.
5. Context read/citation: reads session id, optional task/message ids,
   tool call id, workspace root, execution mode, and safety mode from existing
   dispatcher/headless fields.
6. Capability cards: consumes the existing tool registry; adds no new
   capability cards.
7. Policy hooks: existing `SafetyManager`, approval gates, and path policy
   remain the blocking hooks.
8. World projection: no UI projection changes in this PR; future projection can
   consume the context fields.
9. Harness cases: focused Rust tests for context path resolution, subcall
   derivation, adapter pass-through, and dispatcher/headless compile paths.
10. Rollback/disable path: revert the context/helper additions and switch
    dispatcher/headless calls back to direct `tool.execute(params)`.
11. Deliberately not owned: tool behavior changes, provider-native tool calls,
    soft interrupts, stdin bridge, browser provider status, automation semantics,
    and frontend event reducer work.

## Subagent Findings

- uClaw tool chain is `ToolDefinition -> provider tool call -> ToolCall ->
  ChatDelegate::execute_tool_calls -> ToolRegistry::get -> Tool::execute`.
- jcode `ToolContext` fields map cleanly to uClaw session/tool/workspace
  metadata, but stdin and graceful shutdown should wait for later PRs.
- Safety/path approval order must remain unchanged; path approval keeps its
  separate `"<tool-call>::path"` approval id.
- The safest PR-2 slice is a compatibility helper that preserves
  `Tool::execute(params)`.

## Implementation Tasks

### Task 1: Context Type And Adapter

Create `ToolExecutionMode`, `ToolExecutionContext`, and
`execute_tool_with_context` in `src-tauri/src/agent/tools/tool.rs`.

Requirements:

- keep `Tool::execute(params)` unchanged;
- context fields: `session_id`, `task_id`, `message_id`, `tool_call_id`,
  `workspace_root`, `execution_mode`, `safety_mode`, `capability_profile_id`;
- helper methods: `for_subcall`, `resolve_candidate_path`;
- `execute_tool_with_context` must currently forward to `tool.execute(params)`;
- do not add derived jcode code; reimplement the shape in uClaw terms.

Tests:

- move existing inline tests from `tool.rs` to `tool_tests.rs`;
- add tests for path resolution, subcall derivation, and pass-through execution.

### Task 2: ChatDelegate Adapter Use

In `ChatDelegate::execute_tool_calls`, construct `ToolExecutionContext` after
approval/path gates and before the panic-guarded spawn.

Requirements:

- preserve `_tool_call_id` injection exactly;
- preserve panic guard exactly;
- preserve frontend tool start/result events exactly;
- route the final execution through `execute_tool_with_context`.

### Task 3: Headless Adapter Use

In `HeadlessDelegate::execute_tool_calls`, use the same helper for base
registry tools.

Requirements:

- preserve automation permission check behavior;
- do not introduce approval UI in headless;
- use `ToolExecutionMode::AgentTurn`;
- set `session_id` from `HeadlessDelegate.session_id`.

### Task 4: Status And Review

Update `AGENT_OS_JCODE_UPGRADE_STATUS.md` after implementation with impact,
verification, residual risk, and reviewer notes.

## Verification

Minimum:

```bash
cargo test -p uclaw --lib agent::tools::tool
cd src-tauri && cargo test agent::dispatcher --lib
cd src-tauri && cargo test agent::headless --lib
cargo check -p uclaw --lib
git diff --check
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr2-tool-context
```

Expected:

- all focused tests pass;
- `cargo check -p uclaw --lib` passes with existing warnings only;
- no inline test modules added in touched Rust production files except the
  sibling `#[path = "tool_tests.rs"] mod tests;` shim.

## Risks

| Risk | Level | Mitigation |
|---|---|---|
| Directly changing `Tool::execute` breaks 28 implementers | High | Do not change signature in PR-2. |
| Context path resolver bypasses safety policy | High | Resolver is only a candidate helper; dispatcher still owns `SafetyManager::check_paths`. |
| Headless automation behavior drifts | Medium | Adapter forwards to old execution path; preserve permission gate. |
| Future tools assume context is already authoritative | Medium | Document that context is adapter metadata until TaskEvent projection PRs wire it. |
| jcode license/provenance confusion | Medium | Reimplement pattern; no copied source. |
