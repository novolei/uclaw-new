# Browser Runtime CLI/MCP Route Policy

## Goal

Make Browser Runtime use official Playwright CLI as the default first provider,
retry through built-in Playwright MCP when CLI execution fails or an action needs
MCP-specific accessibility/locator/trace capability, and keep raw Playwright MCP
tools hidden from the LLM unless a developer explicitly enables them.

## Scope

- Default user-facing Browser Runtime provider config enables Playwright CLI and
  Playwright MCP, with priority `CLI > MCP > Local Chromium`.
- Keep raw Playwright MCP tools hidden by default.
- Add a developer toggle that exposes only uClaw's allowlisted Playwright MCP
  raw tools to the LLM tool registry.
- Add CLI failure route evidence and MCP retry evidence to provider action
  results.
- Use MCP-specific override for browser state actions that need accessibility
  snapshot semantics.

## ADR 18 Questions

1. Intent: make provider routing truthful and recoverable when the CLI lane
   fails or a task needs MCP-specific capability.
2. Autonomy boundary: Browser Runtime decides route override; the model does
   not bypass Browser Runtime unless the user enables the developer raw-tools
   toggle.
3. Truth source: `BrowserRuntimeProviderConfig`, Control Center report, and
   provider route evidence.
4. TaskEvent: route decisions continue to emit provider selected/degraded/
   rolled-back intents.
5. Context: no new prompt-wide context; route evidence is carried in tool
   observations.
6. Capability: CLI remains the fast lane; MCP is the secondary capability lane;
   Local Chromium remains fallback.
7. Hooks: no new hook surface.
8. Projection: Control Center exposes raw MCP exposure state and active route.
9. Harness: unit tests cover defaults, MCP raw exposure, route override, and CLI
   failure retry evidence.
10. Rollback: disable CLI/MCP in provider settings or turn off raw MCP exposure.
11. Non-ownership: this slice does not build a live WebContents preview or a
    full Playwright MCP UI; it only corrects routing and exposure policy.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib mcp::tests::playwright`
- `cd ui && npm test -- --run BrowserRuntimeSettings browser-runtime-control-center`
- `git diff --check`
- `npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/playwright-cli-short-session`
