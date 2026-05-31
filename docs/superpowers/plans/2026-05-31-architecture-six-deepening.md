# Architecture Six-Deepening Implementation Plan

Date: 2026-05-31
Branch: `codex/architecture-six-deepening`
Report: `/private/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/architecture-review-20260531-131810.html`
Status: implemented

## Goal

Implement all six deepening opportunities from the architecture review as real
code changes, not just documentation:

1. Deepen the Agent run assembly seam.
2. Deepen the per-turn tool surface.
3. Make Browser provider adapters real.
4. Collapse safety decisions into one module.
5. Turn frontend IPC adapters into run sessions.
6. Give plugins a lifecycle owner.

## Strategic Fit

- Product baseline: Pi-lightweight kernel, optional layers above the loop, one
  `AgentApi` handle, and small interfaces with high leverage.
- Runtime rule: keep `tauri_commands.rs` and `agentic_loop.rs` as orchestration
  shims; new behaviour belongs in focused modules.
- Safety rule: one decision module should own policy, origin, audit, and approval
  result instead of scattering policy knowledge across callers.
- Browser rule: Browser Runtime owns product policy; generic MCP owns generic
  transport, not Playwright product semantics.

## Implemented Slices

### 1. Agent Run Assembly

- Added `agent::run_assembly` as the focused run lifecycle module.
- Kept `agent::harness` as a compatibility shim while moving timeout,
  cancellation, TaskStart/TaskEnd hooks, and outcome projection into the new
  module.
- Added unit coverage for completed and cancelled run outcomes.

### 2. Per-Turn Tool Surface

- Added `agent::tool_shaping::surface::PerTurnToolSurface` to centralize
  model-visible definition shaping, policy filtering, schema normalization, and
  definition hashing.
- Migrated chat turn execution and headless LLM calls to consume the shared
  surface.
- Added unit coverage for hidden tool filtering.

### 3. Browser Provider Adapters

- Added `PlaywrightMcpProviderAdapter` so Browser Runtime owns Playwright MCP
  action mapping, manager calls, evidence extraction, and failure normalization.
- Reduced `BrowserProviderActionExecutor` to orchestration for this provider
  path.
- Reused the existing Playwright adapter test suite as focused evidence.

### 4. Safety Decision Module

- Added `safety::decision` as the single decision interface over legacy policy,
  database rules, and automation permission coverage.
- Kept `SafetyManager` as the state holder while routing chat/tool dispatch,
  automation coverage, and Browser Evaluate through the shared decision call.
- Added unit coverage for permission-denied, permission-allowed, and legacy
  fallthrough paths.

### 5. Frontend IPC Run Sessions

- Added `ui/src/lib/run-session.ts` with shared refresh scheduling and active
  run polling helpers.
- Migrated `SpecRunSurface` away from inline polling and refresh-after-write
  timers.
- Reused the existing `SpecRunSurface` Vitest coverage.

### 6. Plugin Lifecycle Owner

- Added `plugins::lifecycle::PluginLifecycleOwner` to own discovery and
  registration reporting.
- Kept `AgentApi` as the contribution receiver and moved boot-time lifecycle
  logging out of `AppState::new`.
- Added unit coverage for missing plugin directories producing an empty
  successful report.

## Sequence

1. Safety decision module first where it unblocks browser and automation
   consistency.
2. Agent run assembly next, using the unified safety decision shape for cleanup
   and outcome projection.
3. Tool surface after run assembly, so per-turn origin/preset can be explicit.
4. Browser provider adapters, with Playwright MCP policy moved out of generic
   MCP.
5. Frontend run sessions, reducing UI protocol leakage after backend seams
   stabilize.
6. Plugin lifecycle owner, using the finalized `AgentApi` and tool surface.

## Verification Ledger

- Done: GitNexus analyze refreshed the worktree index before edits.
- Done: GitNexus impact was run for the edited existing symbols that GitNexus
  could resolve: `run_agent_harness`, `build_session_registry`,
  `BrowserProviderActionExecutor`, `PluginRegistrar.register`,
  `ChatDelegate.call_llm`, `HeadlessDelegate.call_llm`, and
  `list_definitions`. Resolved impacts were LOW; `AppState::new` was not
  resolvable by GitNexus, so that boot-path edit stayed deliberately narrow.
- Done: `cargo test --manifest-path src-tauri/Cargo.toml safety::decision`
  passed.
- Done: `cargo test --manifest-path src-tauri/Cargo.toml agent::run_assembly`
  passed.
- Done: focused Rust loop passed for `safety::decision`,
  `agent::tool_shaping::surface`, `plugins::lifecycle`, and
  `browser::playwright_mcp_adapter`.
- Done: `cd ui && npm test -- --run SpecRunSurface --passWithNoTests` passed.
- Pending: `git diff --check`.
- Pending: GitNexus detect-changes before commit.
