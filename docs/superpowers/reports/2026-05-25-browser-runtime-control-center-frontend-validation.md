# Browser Runtime Control Center Frontend Validation

Date: 2026-05-25

## Commands

- `npm --prefix ui run build`: PASS.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center`: PASS, 4 passed with existing Rust warnings only.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_provider_probe`: PASS, 3 passed with existing Rust warnings only.
- `npm --prefix ui test -- --run src/lib/browser-runtime/browser-runtime-control-center.test.ts src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/views/Kaleidoscope/modules/Integrations/IntegrationsModule.test.tsx src/lib/dev-tauri-mock.test.ts`: PASS in focused subsets.

## Browser Smoke

Target: `http://127.0.0.1:5178/` with `VITE_UCLAW_MOCK_TAURI=1`.

Settings > Browser Runtime:

- Browser Runtime Control Center renders first: PASS.
- Desired route shows `Playwright CLI > Playwright MCP > Local Chromium`: PASS.
- Active route is read from the Rust-shaped Control Center report: PASS, mock report selected Local Chromium.
- Diagnostics section renders route evidence, probe artifacts, probe history, and collapsed raw report: PASS.
- Raw report is collapsed by default and reveals `"desiredProviderPriority"` only after opening: PASS.
- Static/mock-looking prepare copy is absent: PASS.
- Misleading `No active local Chromium context exists for this session.` global warning is absent: PASS.
- Raw `setup 未完成` / `需要 setup` provider copy is absent from the Browser Runtime page: PASS.

Kaleidoscope > Integrations > Playwright MCP:

- Built-in Playwright MCP integration is visible: PASS.
- Advanced label is visible: PASS.
- Raw MCP tools remain locked off: PASS.
- Wrapped browser actions only: PASS.
- Diagnostics rows are visible: `Last sidecar probe`, `Last action envelope`, `Last artifact/error route`: PASS.

## Notes

- Added mock Tauri fixtures for Browser Runtime, user profile, cost, provider, and automation reads so browser-only validation can open Settings and Kaleidoscope without unrelated null-render crashes.
- No unrelated Settings or Integrations surfaces were redesigned in this PR.
