# Browser Runtime Phase 7F - MCP Artifact/Error Routing

## Context

PR #472 merged Phase 7E and added a real supervised MCP stdio boundary:
fixed uClaw actions initialize the app-managed MCP sidecar and call a small
`tools/call` surface without exposing raw MCP tools. ADR Phase 7 still requires
MCP artifacts and errors to route through the same provider/supervisor evidence
model before Phase 8 unifies provider selection.

This phase adds a provider-level result adapter for Playwright MCP. It keeps the
sidecar boundary real, but stops before task routing, provider promotion, UI,
IPC, DB migrations, or TaskEvent emission.

## ADR Section 18 Questions

1. **Intent:** Convert MCP sidecar results and runner errors into a stable
   provider-level execution result with status, artifact refs, error code,
   retryability, and event/artifact metadata.
2. **Autonomy:** No new autonomy. The adapter is a pure result conversion
   boundary for future supervisor/task routing.
3. **Truth source:** uClaw remains the truth source. MCP raw results stay
   provider evidence, not product state.
4. **TaskEvent:** No TaskEvents are emitted in this PR. The adapter carries the
   browser event name that a later supervisor/task integration can emit.
5. **Context:** Consumes only `PlaywrightMcpSidecarActionResult` and
   `PlaywrightMcpSidecarRunnerError`; it does not read browser pages or user
   profiles.
6. **Capability:** Keeps MCP behind `browser.playwright_mcp` and the existing
   uClaw action envelope. No raw MCP tools are surfaced to the model.
7. **Hooks:** No policy hook mutation. Future routing must still check flags,
   runtime readiness, identity/profile policy, and action permissions before
   execution.
8. **Projection:** No projection write in this PR. The provider result includes
   artifact/event metadata so a later projection can cite the evidence.
9. **Harness:** Focused unit tests cover success artifact routing, MCP
   JSON-RPC error classification, timeout/retryable classification, protocol
   faults, and raw tool exposure blocks. Default browser runtime regressions
   still run.
10. **Rollback:** Revert this PR. Phase 7E stdio execution remains available,
    but MCP lacks the provider-level routing adapter.
11. **Does not own:** Provider selection, Phase 8 parity harness, live task
    routing, TaskEvent emission, Settings/UI, Tauri IPC, DB migration, global
    npm fallback, hosted providers, or raw MCP tool exposure.

## Allowed Files

- `src-tauri/src/browser/playwright_mcp.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase7f-mcp-artifact-error-routing.md`

## Non-Goals

- Do not promote Playwright MCP over Playwright CLI.
- Do not add provider router/default-selection logic.
- Do not emit TaskEvents or write artifact files.
- Do not touch `agentic_loop.rs`, `tauri_commands.rs`, Settings UI, IPC, or DB
  migrations.
- Do not introduce global npm or user-installed Playwright as a production path.

## Impact Targets

- GitNexus impact for `PlaywrightMcpAction`: LOW, 0 direct callers, 0 affected
  processes.
- GitNexus impact for `PlaywrightMcpSidecarActionResult`: LOW, 1 direct caller,
  0 affected processes.
- GitNexus impact for `PlaywrightMcpSidecarRunnerError`: LOW, 0 direct callers,
  0 affected processes.
- GitNexus impact for `playwright_mcp_provider_status`: LOW, 3 direct test
  callers, 0 affected processes.

## Implementation

- Add `PlaywrightMcpProviderExecutionStatus`, error DTO, artifact DTO, and
  provider result DTO.
- Add conversion helpers from successful sidecar results and sidecar runner
  errors into provider-level execution results.
- Preserve `provider_id`, `request_id`, action kind, raw-tool-hidden state, and
  sidecar artifact refs.
- Attach ADR Phase 7 event metadata without emitting the event in this PR.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`

## Risk / Rollback

Risk is low because this is additive provider-result conversion with focused
tests. Rollback is a single PR revert; no data migration, user file mutation, or
provider selection state changes are involved.
