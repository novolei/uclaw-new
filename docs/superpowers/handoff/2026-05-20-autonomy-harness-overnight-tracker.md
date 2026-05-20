# Autonomy Harness Overnight Tracker

**Start:** 2026-05-20
**Branch:** `codex/overnight-harness-248-252`
**Base:** `origin/main` @ `ae12884`
**Mode:** isolated Codex worktree, reviewer-gated task sequence

---

## Numbering Note

The rows named `#248` through `#252` below are the original planned harness slice labels from the autonomy rollout plan, not the final GitHub PR numbers. The implementation batch for those slices was merged as GitHub PR #282 (`feat(harness): add autonomy eval and self-improvement gates`). The frontend connector that exposes Browser and Self alongside Memory and Agent in System Diagnostics was merged later as GitHub PR #285 (`feat(harness): connect autonomy suites in diagnostics`).

## Status

| # | Task | Status | Review | Verification | Notes |
|---|---|---|---|---|---|
| 1 | Runtime Diagnostics UI v2 | completed | reviewer-gated | `cargo test --manifest-path src-tauri/Cargo.toml diagnostics_status_tests --lib`; `cargo test --manifest-path src-tauri/Cargo.toml mcp::tests::diagnostic_error_summary_classifies_without_user_content --lib`; `npm run build`; `git diff --check` | Map real memU/gbrain runtime truth into actionable diagnostics; fixed review findings for gbrain stale paths, CLI timeouts, and diagnostic privacy. |
| 2 | GBrain MCP Tool UX hardening | completed | self-reviewed | `cargo test --manifest-path src-tauri/Cargo.toml mcp::tests::gbrain_cli_error_payload_is_structured_for_recovery_ui --lib`; `npm test -- --run src/components/agent/tool-renderers/index.test.tsx`; `git diff --check` | GBrain CLI failures now return structured recovery payloads and render through the existing tool-result UI. |
| 3 | Memory Inventory Smoke Harness | completed | reviewer-gated | `cargo test --manifest-path src-tauri/Cargo.toml harness::memory_inventory --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Added `run_memory_inventory_smoke` to verify memU and gbrain inventory reachability, count truth, empty inventories, and error states. |
| 4 | Release Path Simulation Test | completed | reviewer-gated | `cargo test --manifest-path src-tauri/Cargo.toml memu_runtime_resolution_tests --lib`; `git diff --check` | Simulates packaged resources and now refuses dev/system fallback when a packaged resource dir is incomplete, preventing release launcher/manifest dev-path leakage. |
| #248 | Browser parity harness | completed | reviewer-gated | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::browser --lib`; `cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Added executable browser parity adapter, deterministic fixture materialization, real `BrowserAgentLoop` executor bridge, deterministic auth-profile seeding, and scorecard artifacts. |
| #249 | Memory/gbrain eval harness | implemented | self-reviewed; reviewer slot unavailable | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory --lib`; `cargo test --manifest-path src-tauri/Cargo.toml memory_gbrain_eval_harness_command_tests --lib`; `cargo test --manifest-path src-tauri/Cargo.toml harness::memory_inventory --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Added memory/gbrain harness adapter, app-native Tauri command entrypoint, scorecard artifacts, live write/recall probe, and recall-grounding/hallucination scoring. |
| #250 | Agent loop control-plane harness | implemented | pending review | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::agent_loop --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Added normalized agent-loop control-plane trace harness for tool/result pairing, permission boundaries, checkpoints, and non-running final status. |
| #251 | Harness UI/reporting | implemented; expanded in GitHub #285 | self-reviewed | `npm test -- --run src/components/settings/SystemTab.test.tsx`; `npm run build` | Initial slice added Memory and Agent suite buttons. GitHub #285 expanded the same System tab surface to `All`, `Browser`, `Memory`, `Agent`, and `Self`, wiring Browser parity and self-improvement gates into the frontend scorecard UI. Used `ui-ux-pro-max` guidance for dense operational UI, accessible buttons, and non-decorative status cues. |
| #252 | Self-improvement gates | implemented | self-reviewed; reviewer slot unavailable | `cargo test --manifest-path src-tauri/Cargo.toml harness::self_improvement --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Added self-improvement candidate gates for memory/gbrain/skill/prompt/hook promotion. Candidates must carry evidence, pass required suites, meet score threshold, avoid blockers, and include rollback references. |

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
- 2026-05-20: Task 3 added the app-native memory inventory smoke harness. Reviewer found a blocking empty-gbrain parser bug (`No pages found.` misread as a slug); fixed with a regression test. The smoke now distinguishes pass, empty, unavailable, and error for memU and gbrain.
- 2026-05-20: Task 4 added packaged-resource simulation for memory runtimes. Reviewer found the first version only covered complete bundles; fixed by changing resolvers to refuse dev/data/system fallback when `resource_dir` is packaged but incomplete. Regression tests now cover both complete release resources and incomplete release resources.
- 2026-05-20: PR #248 browser parity harness started. Added deterministic scoring for navigation, multi-tab planning, file upload, auth profile restore, boundary detection, checkpoint resume, and long-task recovery over real `BrowserTaskRun` traces. Reviewer found blocking issues around non-executable scoring, `:0` fixture URLs, `/tmp` upload paths, executor-error partial episodes, URL false positives, and non-deterministic auth restore. Fixes added `BrowserAgentLoop` executor wiring, fixture server/materialization, workspace-safe upload fixture paths, failed scorecard artifacts, observed-state-only URL checks, `Decide` false-positive coverage, fake fixture storageState, and broker seeding via `BrowserAgentLoopParityExecutor`.
- 2026-05-20: PR #249 memory/gbrain eval harness started. Added scorecard cases over `MemoryInventorySmokeReport` so memU/gbrain availability, empty inventory truth, and gbrain MCP tool exposure can be graded as harness episodes.
- 2026-05-20: PR #249 review gap fix implemented. Added `run_memory_gbrain_eval_harness` as an app-visible command that materializes harness episodes/artifacts from live memory inventory smoke and executes a namespaced live write/recall probe against memU and gbrain when those services are connected. `MemoryGbrainEvalEvidence` now scores write receipts, recall evidence, grounded expected facts, and forbidden hallucinated facts.
- 2026-05-20: Tried to launch a fresh reviewer for PR #249 but the thread had reached the subagent limit; self-review found no broad formatting churn or unrelated scope. Residual risk: the live probe intentionally writes harness-namespaced test facts, so it should remain an explicit eval command rather than an automatic diagnostics action.
- 2026-05-20: PR #250 implemented control-plane trace harness. It normalizes model turns, tool calls/results, permission requests, checkpoints, and final loop status into harness events and scorecards. The app command `run_agent_control_plane_harness` runs deterministic fixture traces and writes artifacts.
- 2026-05-20: PR #251 implemented Harness UI/reporting in the System tab. Added explicit Memory and Agent harness run buttons, pass/fail/score summaries, failed check surfacing, and a focused component test around the Agent scorecard path.
- 2026-05-20: PR #252 implemented self-improvement gates. Added deterministic promotion/hold/reject policy checks for mutable memory, gbrain, skill, prompt, and hook candidates, plus the app command `run_self_improvement_gate_harness` and scorecard documentation.
- 2026-05-20: GitHub PR #285 connected the remaining harnesses to the System tab. Added the app command `run_browser_parity_harness`, deterministic `BrowserFixtureParityExecutor`, `All`/`Browser`/`Memory`/`Agent`/`Self` controls, self-improvement scorecard normalization, and `harness-ui-connectors-scorecard.md`. Boundary: the Browser UI button is a deterministic parity fixture run, while live browser autonomy remains verified through chat `browser_task`, Browser panel, and Browser Task Monitor.
