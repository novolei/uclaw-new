# Autonomy Harness Overnight Tracker

**Start:** 2026-05-20
**Branch:** `codex/overnight-harness-248-252`
**Base:** `origin/main` @ `ae12884`
**Mode:** isolated Codex worktree, reviewer-gated task sequence

---

## Status

| # | Task | Status | Review | Verification | Notes |
|---|---|---|---|---|---|
| 1 | Runtime Diagnostics UI v2 | completed | reviewer-gated | `cargo test --manifest-path src-tauri/Cargo.toml diagnostics_status_tests --lib`; `cargo test --manifest-path src-tauri/Cargo.toml mcp::tests::diagnostic_error_summary_classifies_without_user_content --lib`; `npm run build`; `git diff --check` | Map real memU/gbrain runtime truth into actionable diagnostics; fixed review findings for gbrain stale paths, CLI timeouts, and diagnostic privacy. |
| 2 | GBrain MCP Tool UX hardening | completed | self-reviewed | `cargo test --manifest-path src-tauri/Cargo.toml mcp::tests::gbrain_cli_error_payload_is_structured_for_recovery_ui --lib`; `npm test -- --run src/components/agent/tool-renderers/index.test.tsx`; `git diff --check` | GBrain CLI failures now return structured recovery payloads and render through the existing tool-result UI. |
| 3 | Memory Inventory Smoke Harness | pending | pending | pending | Regressible smoke for memU + gbrain inventory truth. |
| 4 | Release Path Simulation Test | pending | pending | pending | Guard packaged resource resolution instead of dev-only paths. |
| #248 | Browser parity harness | pending | pending | pending | Browser-use parity fixtures and scorecard. |
| #249 | Memory/gbrain eval harness | pending | pending | pending | Memory/gbrain adapter eval cases. |
| #250 | Agent loop control-plane harness | pending | pending | pending | Agent loop/tool/permission trace harness. |
| #251 | Harness UI/reporting | pending | pending | pending | Requires `ui-ux-pro-max` before implementation. |
| #252 | Self-improvement gates | pending | pending | pending | Promotion gates for memory/skill/prompt changes. |

---

## Review Protocol

For every task:

1. Implementation lands in a narrow diff.
2. Run targeted tests or checks.
3. Run code-quality review.
4. Run task/spec-fit review.
5. Fix blocking findings before moving to the next task.
6. Record result and verification command here.

---

## Update Log

- 2026-05-20: Created isolated worktree from `origin/main` to avoid the dirty Claude branch in the main checkout.
- 2026-05-20: Started Task 1 discovery across diagnostics, memU, gbrain, MCP, and existing harness modules.
- 2026-05-20: Task 1 implemented diagnostics snapshots and reason-based UI rows. Verification: `cargo test --manifest-path src-tauri/Cargo.toml diagnostics_status_tests --lib` passed after provisioning ignored resource placeholders for this clean worktree; `npm run build` passed using the shared local `ui/node_modules` symlink.
- 2026-05-20: Task 1 review fixes landed: memU diagnostics no longer auto-restart during health probe; gbrain checks connected-with-zero-tools, missing PGLite, stale/missing command or entry, missing `GBRAIN_HOME`, recent CLI SIGKILL/timeout summaries, and redacted diagnostic exports. Final verification passed: diagnostics Rust tests, MCP diagnostic privacy test, UI build, and diff check.
- 2026-05-20: Task 2 implemented structured gbrain CLI error payloads for timeout, page-not-found suggestions, PGLite lock/not-ready, path mismatch, launcher failure, permission denied, and SIGKILL. The chat tool-result renderer now shows actionable recovery hints and candidate slugs without introducing a separate UI surface.
