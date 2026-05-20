# Harness UI Connectors Scorecard

Date: 2026-05-20
Branch: `codex/harness-ui-connectors`
Merged: GitHub PR #285, merge commit `c1a57cf`

## Scope

- Expose the Browser parity harness in Settings > System Diagnostics.
- Expose the self-improvement gate harness beside the existing Memory/GBrain and Agent control-plane harnesses.
- Add an `All` runner so the four autonomy suites can be exercised from one frontend surface.

## Implemented

- `run_browser_parity_harness` Tauri command registered in the desktop invoke handler.
- Deterministic Browser parity fixture executor for stable local regression runs.
- System diagnostics UI controls for `All`, `Browser`, `Memory`, `Agent`, and `Self`.
- Self-improvement gate reports normalized into the same scorecard UI as the other harnesses.
- Frontend regression coverage for invoking all four harness commands from the unified controls.

## Product Boundary

The Browser button runs the deterministic parity harness. It validates the browser-agent contract, action trace shape, checkpoint and intervention states, auth profile pre-navigation ordering, multi-tab expectations, file upload expectations, and recovery scoring without relying on live websites or an LLM.

Real live browser autonomy is still verified through chat `browser_task`, the browser panel, and Browser Task Monitor.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::browser --lib`
- `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`
- `npm test -- --run src/components/settings/SystemTab.test.tsx`
- `npm run build`
- `git diff --check`

## Follow-Up

- Add persisted historical harness runs to make System Diagnostics show trend lines instead of only the latest in-memory report.
- Add a separate live Browser smoke harness that launches a local fixture server and uses the real browser task loop with strict timeout and no external network dependency.
