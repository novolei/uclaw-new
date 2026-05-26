# Playwright CLI Socket Session Fix

## Goal

Prevent official `playwright-cli` browser actions from failing on macOS with `listen EINVAL` when uClaw passes a long conversation UUID as the CLI session name.

## Evidence

- Live Bilibili run failed in `browser_navigate` through Playwright CLI with socket path length 129.
- Minimal reproduction with full session name failed with `listen EINVAL`.
- The same command with short session name `u-e479dffb` succeeded and returned Page/Snapshot output.
- `playwright_cli_session_name()` currently emits `uclaw-<full uuid>`, which is too long once official Playwright CLI prepends its own socket token.

## Scope

- Keep official Playwright CLI session names short and stable.
- Preserve enough prefix for debugging plus a stable hash suffix for collision resistance.
- Add regression tests for UUID-shaped session ids and command argument generation.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli::tests::official_cli -- --nocapture`
- `git diff --check`
- `npx gitnexus detect-changes --repo /Users/ryanliu/Documents/uclaw-worktrees/playwright-cli-short-session`
