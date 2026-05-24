# Browser Runtime Dry-Run Drift Audit

## Summary

This pre-Phase-5B audit checks whether Phase 1 through Phase 5A drifted away
from the Browser Runtime Supervisor ADR because earlier repo guidance made
`agentic_loop.rs` and `tauri_commands.rs` feel too risky to edit. The goal is
not to implement Phase 5B yet. The goal is to classify dry-run and contract-only
lanes as either ADR-correct sequencing or corrective work that must be scheduled
before the rest of the Browser Runtime roadmap.

## ADR Section 18 Questions

1. **User intent:** unblock the Browser Runtime goal chain by proving we are not
   trapped in dry-run work caused by obsolete file-name-only DMZ gates.
2. **Autonomy level:** L0 audit and planning only. This phase changes docs and
   tracker state, not runtime behavior.
3. **Canonical truth source:** the Browser Runtime ADR, this plan, and
   `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`.
4. **TaskEvent entries:** none. Any future runtime-pack execution slice must use
   the existing browser runtime event names from execution reports.
5. **Context read/citation:** read the ADR, tracker, goal-mode docs hygiene
   plan, Phase 2/4/5 plans, runtime-pack IPC/executor code, dispatcher prompt
   patching, and Playwright CLI contract code.
6. **Capability cards:** none. The audit consumes existing
   `browser.playwright_cli` and runtime-pack contracts but does not promote or
   execute a provider.
7. **Policy hooks:** no runtime policy gates are exercised. The audit must
   preserve the new guidance that `agentic_loop.rs` and `tauri_commands.rs` are
   normal hot-path code files, while still requiring GitNexus impact and focused
   tests before editing existing symbols.
8. **World projection:** none. The audit only records whether existing Settings,
   Startup Doctor, task-time prompt, checkpoint, and provider-contract
   projections are sufficient for entering Phase 5B.
9. **Harness cases:** markdown whitespace checks, stale-rule grep, GitNexus
   detect, and focused browser-runtime Rust regressions as the no-code safety
   baseline.
10. **Rollback/disable path:** revert this docs PR. It changes no code,
    database rows, browser sessions, runtime-pack files, or provider selection.
11. **Does not own:** no runtime-pack download/extract/delete, no Playwright
    child process, no IPC mutation, no task-loop wiring, no provider promotion,
    no UI behavior change, and no database migration.

## Allowed Files

- `docs/superpowers/plans/2026-05-24-browser-runtime-dry-run-drift-audit.md`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`

## Non-Goals

- Do not edit Rust or TypeScript code in this audit PR.
- Do not touch `agentic_loop.rs`, `tauri_commands.rs`, `main.rs`, DB
  migrations, workspace `Cargo.toml`, `BEHAVIOR.md`, `AGENTS.md`, or
  `CONTEXT.md`.
- Do not implement Phase 5B child-worker execution in this PR.
- Do not convert Settings dry-run buttons to real runtime mutations here.
- Do not promote `browser.playwright_cli` over chromiumoxide.

## Audit Scope

- Phase 2C-2F runtime-pack dry-run and managed executor boundary.
- Phase 4O-4Q task-time prompt/dispatch bridge and avoidance of
  `agentic_loop.rs`.
- Phase 4R-4X Settings/Startup Doctor IPC and avoidance of `tauri_commands.rs`.
- Phase 5A Playwright CLI provider contract-only lane.
- Any tracker language that still implies obsolete DMZ stops for
  `agentic_loop.rs` or `tauri_commands.rs`.

## Audit Findings

1. Phase 4R-4X did not create bad architecture by avoiding
   `tauri_commands.rs`. The dedicated `browser::runtime_pack_ipc` module plus
   narrow `main.rs` registration is the desired thin-module direction.
2. Phase 4O-4Q did not need to edit `agentic_loop.rs` because the accepted
   dispatch boundary was the tool-call dispatcher plus browser task request
   parsing. This preserved approval semantics and kept agent loop orchestration
   thin.
3. Phase 5A is intentionally contract-only per ADR Phase 5 sequencing. Phase 5B
   should leave the contract lane and add supervised short-lived child-worker
   execution behind the feature flag.
4. The remaining dry-run risk is real runtime-pack execution. Phase 2 created
   a no-side-effect planner, dry-run executor, and abstract managed runner, but
   no filesystem/network adapters yet perform app-managed download, verify,
   extract, promote, cleanup, or rollback. This is not caused by
   `tauri_commands.rs`; it is an unfinished ADR Phase 2 capability that now
   blocks the promise that users do not need global npm/manual Playwright setup.
5. Runtime readiness can currently be over-reported because the default
   filesystem probe treats `worker_startup_ok` and `real_page_probe_ok` as
   true. Phase 5A consumes `ready && can_run_browser_tasks` as provider-ready,
   so Phase 5B-preflight must add real worker/page probes and tests that broken
   workers do not report ready.
6. Settings live status currently drops backend path data. Rust returns
   `runtime_root` and `current_pack_dir`, but the frontend runtime-pack status
   type/view-model does not use those fields for the Settings path row. This is
   a small UI/IPC correction that should be fixed before goal completion,
   ideally adjacent to the real readiness preflight.
7. Historical tracker rows and old phase plans still mention DMZ gates for
   `agentic_loop.rs` and `tauri_commands.rs`. Treat those as historical
   evidence explaining earlier caution, not as active constraints after PR
   #452. Current rules are: normal hot-path discipline, GitNexus impact before
   symbol edits, narrow modules, focused tests, and fresh review when broad,
   risky, or HIGH-impact.

## Corrective Phase Order

Continue in this order:

1. **Phase 5B-preflight A:** add real runtime-pack step-runner/probe adapters
   behind policy gates and tests: worker startup probe, real page probe,
   download, checksum verify, staging extract/install, promote current pack,
   rollback retention, cleanup, and rollback. This closes the Phase 2 dry-run
   lane before relying on the app-managed pack in a real provider.
2. **Phase 5B-preflight B:** fix Settings live status mapping so IPC-returned
   `runtimeRoot` / `currentPackDir` reaches the runtime-pack path row.
3. **Phase 5B:** add the Playwright CLI short-lived child-worker fixture runner
   behind `playwright_cli`, consuming the app-managed runtime pack and the
   Phase 5A JSON envelope.
4. **Phase 5C+:** add timeout/kill/retry/artifact/harness expansion and only
   later consider provider routing or promotion.

## Impact Targets

- Docs-only edits require no symbol-level GitNexus impact.
- Before any corrective code phase edits existing symbols, run GitNexus impact
  for the specific symbol. Likely future targets include runtime-pack executor
  functions, the child-worker runner entry point, and any IPC command symbols.

## Verification

- `rg -n 'special DMZ constraints for agentic_loop.rs|special DMZ constraints for tauri_commands.rs|agentic_loop\\.rs.*active.*DMZ|tauri_commands\\.rs.*active.*DMZ' docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-dry-run-drift-audit.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-dry-run-drift-audit`

## Rollback

Revert the audit commit. It has no runtime side effects.
