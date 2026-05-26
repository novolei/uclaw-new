# Playwright CLI Close Loop

Date: 2026-05-26
Branch: `codex/playwright-cli-close-loop`

## Goal

Make `browser.playwright_cli` a real executable Browser Runtime provider instead
of a route-evidence stub. Direct `browser_navigate` and `browser_task` must be
able to route through the official global `playwright-cli`, receive a stable tab
id, and observe URL/title/page state from the selected provider.

## Scope

- Route selected CLI provider actions to official `playwright-cli -s=<session>`
  commands.
- Parse official CLI stdout for page URL, page title, snapshot path, and page
  text.
- Return `tab_id` and provider observation evidence to direct browser tools.
- Let `browser_task` reuse provider-returned observation state instead of
  reading stale local Chromium state after a CLI action.
- Gate provider readiness on real official Playwright discovery in runtime
  status.

## ADR Questions

1. Intent: finish the user-visible CLI first-priority provider loop.
2. Autonomy: no autonomous installs or elevated commands; execution assumes the
   official setup path has made `playwright-cli` available.
3. Truth source: Browser Runtime status and actual CLI command output.
4. TaskEvent: provider route evidence remains attached to action results.
5. Context: per-uClaw session CLI session names isolate browser state.
6. Capability: promotes CLI navigate/snapshot/click/type/screenshot from
   planned capability to executable adapter.
7. Hooks: no new hooks.
8. Projection: provider observation bridges URL/title/page text into
   `BrowserObservation`.
9. Harness: focused Rust unit tests cover command mapping, stdout parsing,
   selected route execution, readiness gating, and observation bridge.
10. Rollback: disable Playwright CLI provider or revert this PR.
11. Non-goals: no raw shell passthrough, no MCP sidecar change, no automatic
    Node installation, no full multi-tab parity.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib playwright_discovery_tests -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib playwright_setup_tests -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools::tests::direct_browser_tool_route_options_from_status_uses_config_backed_feature_flags -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib config_aware_status_uses_real_official_playwright_readiness_gate -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib provider_observation_uses_playwright_cli_output_state -- --nocapture`
- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
