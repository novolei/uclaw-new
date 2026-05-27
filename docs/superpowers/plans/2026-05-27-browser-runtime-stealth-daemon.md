# Playwright Stealth and Long-Lived Resident Daemon Worker Plan

This document describes the implementation details for adding Playwright Stealth (anti-bot bypassing) and a Long-Lived Resident Daemon Worker process to the `browser.playwright_cli` provider.

## Proposed Changes

1. **`uclaw-playwright-worker.mjs`**:
   - Check if `--daemon` argument is passed.
   - If `--daemon` is set, enter a readline line-by-line reading mode instead of reading to EOF.
   - Keep long-lived browser, context, and page instances in global references.
   - Overwrite standard navigator attributes (webdriver, platforms, languages) using `context.addInitScript(...)`.
   - Set standard user agent and viewport in `browser.newContext(...)` options.
   - Periodically save storage state (cookies/local storage) back to state path.
2. **`playwright_cli.rs`**:
   - Maintain a thread-safe static registry of `PlaywrightDaemon` indexed by session ID.
   - Communicate over stdin/stdout line-by-line using NDJSON.
   - Include a supervisor retry-with-fallback logic.
3. **`provider_execution.rs`**:
   - Pass `session_id` to CLI execution.

## Verification
- Run existing and new tests.
