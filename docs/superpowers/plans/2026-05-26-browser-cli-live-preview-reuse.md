# Browser CLI Live Preview Reuse Plan

## Goal

Restore the live Browser preview panel and floating mini-preview when
`browser_navigate` is routed through the official Playwright CLI provider.

## Root Cause

The legacy live UI is driven by local Chromium tab IDs and CDP screencast
events:

- `browser_navigate` returned a local `BrowserContext` tab ID.
- `BrowserPanel` and `BrowserPreviewOverlay` used that tab ID to call
  `browser_start_screencast`.
- Rust emitted `browser:screencast-frame` events for that local tab.

The Playwright CLI provider currently returns `playwright-cli:*`, which is a
provider session ID, not a local `BrowserContext` tab. The frontend correctly
does not start local CDP screencast for that ID, so both preview surfaces stay
on the placeholder.

## Implementation

1. Keep Playwright CLI as the canonical action provider and preserve provider
   route evidence.
2. After a successful CLI browser action, best-effort mirror preview-compatible
   actions into the existing local `BrowserActionRegistry`.
3. For successful CLI navigation, return the local preview tab ID while storing
   the provider tab ID and preview evidence in the observation payload.
4. For CLI click/type, mirror against the returned preview tab ID when present
   so the old screencast remains visually current.
5. Skip preview mirroring in test-only managers without a Tauri app handle, and
   never fail the provider action because preview mirroring failed.

## ADR 18 Checklist

1. User problem: CLI-routed browser actions work but the app shows no live
   browser surface.
2. Non-goal: Replace Playwright CLI execution or expose raw MCP tools.
3. Source of truth: Provider action result remains canonical; local mirror is UI
   projection only.
4. Boundary: Backend provider execution owns the preview bridge, frontend keeps
   existing screencast UI.
5. Failure mode: Provider success remains success even if preview mirror fails.
6. Compatibility: Legacy local Chromium preview path is reused unchanged.
7. Security: No new shell command surface; local browser context stays inside
   existing uClaw profile policy.
8. Observability: Observation JSON records provider tab ID, preview tab ID, and
   preview mirror status.
9. Rollout: Narrow PR behind existing provider route; no config migration.
10. Verification: Rust provider execution tests plus targeted UI tests for real
    tab gating.
11. Ownership: Browser Runtime owns provider execution and preview projection;
    Browser UI owns rendering.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution_tests -- --nocapture`
- `cd ui && npm test -- --run browser-tab-atoms preview-panel-atoms`

