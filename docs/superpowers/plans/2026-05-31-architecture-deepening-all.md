# Architecture Deepening All — Implementation Plan

Status: completed
Owner: Codex
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/architecture-deepening-all`
Branch: `codex/architecture-deepening-all`
Source report: `/private/var/folders/h_/z21cg38x3xz6z1ppwjcz_8qc0000gn/T/architecture-review-20260531-115329.html`

## Objective

Implement all six deepening candidates from the local architecture review, removing the named shallow modules, action items, and the hidden design bugs they imply.

## Candidate Checklist

1. Make `BrowserRuntimeActionExecutor` the only Browser Runtime action seam.
2. Make browser memory policy executable instead of decorative.
3. Complete `AgentApi` toolset depth for deferred browser, memU, and MCP tools.
4. Extract an `AgentHarness`-shaped run assembly module so IPC stays thin.
5. Move Playwright MCP product policy out of generic `mcp.rs` and into Browser Runtime.
6. Split Browser Runtime frontend invoke/types/debug shape behind a focused frontend adapter.

## ADR §18 Answers

1. User intent: safer long-running browser/agent work by making runtime, memory, tool, MCP, and UI seams deeper and easier to verify.
2. Autonomy level: developer-time architecture refactor; product behavior remains at existing task autonomy levels.
3. Canonical truth source: Rust backend modules for runtime state, memory policy receipts, `AgentApi` registration, generic MCP transport, and Browser Runtime frontend adapter DTOs.
4. TaskEvent entries: no new event family in this plan; existing browser route evidence, memory policy receipts, hook events, and eval artifacts stay canonical. If implementation exposes a missing event, add the smallest existing-family emission.
5. Context read: `BEHAVIOR.md`, `CONTEXT.md`, Pi-lightweight ADR, gbrain freeze ADR, Browser Runtime ADR/specs, and current code. Cite via plan notes and tests, not runtime prompts.
6. Capability cards: consumes existing Browser Runtime providers, MCP server management, memory adapters, and `AgentApi` tool descriptors. No new end-user capability card is planned.
7. Policy hooks: existing `SafetyManager`, approval handlers, browser evaluate gate, memory_graph freeze guard, and MCP raw-tool exposure policy.
8. World projection: existing Browser Runtime status/control-center projection and startup/settings UI remain the projection. Frontend split must preserve visible fields.
9. Harness cases: focused Rust unit tests for runtime routing, memory policy execution, toolset assembly, MCP policy relocation; focused Vitest for frontend adapter/control-center/debug bridge.
10. Rollback/disable path: each slice is a narrow module-level refactor with unchanged public behavior; rollback is reverting the slice commit. Provider toggles, MCP raw exposure, and browser provider fallback remain existing disable paths.
11. Deliberately not owned: no schema migrations, no product redesign, no provider default change, no memory_graph revival, no new external runtime distribution model, and no broad UI redesign.

## Implementation Order

### Slice 1 — Browser Runtime action seam

- Run GitNexus impact before editing `BrowserRuntimeActionExecutor`, `route_options_from_runtime_status`, direct browser tool execution helpers, and the affected IPC command.
- Make the executor expose the needed direct-action interface for all old callers.
- Make route-option assembly private to `runtime_execution.rs` if possible.
- Verify by targeted Rust tests around runtime execution/provider execution.

### Slice 2 — Browser memory policy execution

- Run GitNexus impact before editing `BrowserLongTermMemoryAdapter`, `classify_browser_evidence`, and target adapters.
- Add a real memory policy execution module or function that executes target adapters and returns receipts.
- Remove direct gbrain MCP transport from `browser/memory_adapter.rs`.
- Preserve browser artifact/local memory behavior and gbrain promotion cooldown semantics if still needed.
- Verify with Rust tests and `UCLAW_MEMORY_GRAPH_PANIC_ON_WRITE=1` focused tests where practical.

### Slice 3 — AgentApi toolset depth

- Run GitNexus impact before editing `AgentApi`, `SessionContext`, `build_tool_registry`, and deferred tool constructors.
- Move browser, memU, and MCP construction behind registered descriptors/adapters or an AgentApi-owned session toolset interface.
- Keep tool behavior stable and avoid changing prompts/tool schemas except where required to preserve current behavior.
- Verify registry-building and representative tool definitions.

### Slice 4 — AgentHarness run assembly

- Run GitNexus impact before editing `tauri_commands` launcher symbols and agent run assembly modules.
- Extract launch wiring into a focused module under `agent/`.
- Keep `tauri_commands.rs` as request parsing, state lookup, and delegation.
- Verify chat/agent launch unit tests or compile coverage.

### Slice 5 — Playwright MCP policy locality

- Run GitNexus impact before editing `mcp.rs` Playwright-specific symbols and browser Playwright MCP adapters.
- Move Playwright MCP server id, allowlist, builtin config, and raw exposure policy to a browser-owned module.
- Keep generic MCP server lifecycle and call transport generic.
- Verify MCP/browser provider tests.

### Slice 6 — Frontend Browser Runtime adapter

- Move Browser Runtime invoke wrappers and DTO ownership out of the global bridge/startup module into `ui/src/lib/browser-runtime/`.
- Keep `BrowserRuntimeSettings`, startup, and debug bridge using the adapter.
- Verify focused Vitest for browser runtime settings/control-center/debug bridge and TypeScript build if dependencies are available.

## Verification Evidence

- `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_execution` — 7 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml browser::tools` — 16 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy_tests` — 5 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml memory_policy` — 12 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml browser::memory_adapter` — 1 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml agent::tools::builtin_descriptors` — 2 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml playwright_mcp` — 11 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml agent::harness` — compiled, 0 matching tests.
- `cd ui && npm test -- --run src/lib/browser-runtime` — 3 files / 17 tests passed.
- `git diff --check` — passed.
- `npx gitnexus analyze` — indexed this worktree successfully after non-fatal scope warnings in unrelated automation UI tests.
- GitNexus `detect_changes(scope=all)` on this worktree — medium risk, 37 changed symbols, 3 affected registry-build processes, no HIGH/CRITICAL gate.
