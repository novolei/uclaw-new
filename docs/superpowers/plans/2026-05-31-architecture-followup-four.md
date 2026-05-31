# Architecture Follow-up Four Plan

Date: 2026-05-31
Branch: `codex/architecture-followup-four`
Base: `origin/main` at `0daed321`
Status: implemented

## Goal

Complete the four follow-up tasks proposed after PR #613, in priority order:

1. Converge more production agent-loop callers onto `agent::run_assembly`.
2. Expand the `safety::decision` test matrix.
3. Move local Chromium and Playwright CLI provider-specific logic into Browser
   provider adapters.
4. Extend plugin lifecycle ownership beyond discover/register into health and
   unregister status.

## Scope

- Keep behavior stable while moving ownership boundaries.
- Prefer focused modules over adding more orchestration to hot-path files.
- Do not touch schema migrations.
- Keep all changes in this follow-up branch/worktree.

## GitNexus Notes

- `npx gitnexus analyze` refreshed the worktree index.
- Impact before implementation:
  - `run_agent`: LOW, 1 direct caller.
  - `safety::decision::decide_tool_call`: LOW, tests only in index.
  - `BrowserProviderActionExecutor` impl: LOW.
  - `PluginLifecycleOwner` impl: LOW.
  - `AgentApi.unregister_plugin`: LOW.

## Completion Evidence

- Done: `rg "run_agentic_loop\\(" src-tauri/src` now shows only
  `agentic_loop.rs` internals/tests, the run-assembly wrapper, and a stale
  comment; production callers route through `agent::run_assembly`.
- Done: `safety::decision` tests cover permission coverage, DB rules + audit,
  legacy policy, plan mode, blocked tools, and Browser Evaluate-shaped
  requests.
- Done: Browser provider execution delegates local Chromium to
  `LocalChromiumProviderAdapter` and Playwright CLI translation/execution/
  evidence/preview mirroring to `PlaywrightCliProviderAdapter`.
- Done: Plugin lifecycle report exposes status, health, and unregister results,
  and the echo plugin registration/unregistration path is covered.
- Done: focused Rust loops passed:
  - `safety::decision`
  - `agent::run_assembly`
  - `plugins::lifecycle`
  - `browser::playwright_cli_adapter`
  - `browser::provider_execution`
  - `channels::dispatcher`
  - `automation::runtime::service`
  - `symphony_graph::runtime::node_run`
  - `agent::regular_task`
  - `agent::rollout_integration`
  - `agent::teams::worker`
- Done: `git diff --check`.
- Done: GitNexus `detect-changes --scope staged` reported 17 files, 43
  symbols, 0 affected processes, low risk.
