# Browser Screenshot Provider Routing

## Context

`browser_navigate` now routes through the Browser Runtime provider adapter, but
`browser_screenshot` still calls the legacy local Chromium context directly and
returns only base64. When the user asks to save a screenshot, the agent falls
back to `bash + npx playwright-cli`, which bypasses route evidence and can use a
different Playwright CLI session namespace.

## Scope

- Add an explicit Browser Runtime screenshot action.
- Route the public `browser_screenshot` tool through `BrowserProviderActionExecutor`.
- Support optional workspace-scoped file saving.
- Map Playwright CLI screenshots to the official `playwright-cli screenshot`
  command with `--filename`.
- Preserve local Chromium fallback and base64 output.

## Verification

- `CARGO_TARGET_DIR=/tmp/uclaw-codex-target cargo test -p uclaw official_cli_action_maps_to_session_scoped_screenshot_command`
- `CARGO_TARGET_DIR=/tmp/uclaw-codex-target cargo test -p uclaw selected_cli_route_executes_screenshot_with_filename`
