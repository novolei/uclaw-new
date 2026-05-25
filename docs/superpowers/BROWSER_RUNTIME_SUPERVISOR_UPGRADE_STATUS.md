# Browser Runtime Supervisor Upgrade Status - Single Source of Truth

> Live state for the Browser Runtime Supervisor and Playwright provider
> implementation program.
>
> This file follows the closed-loop pattern from
> `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: every phase PR updates
> this status file so later sessions can resume from the current row instead of
> reconstructing thread history.
>
> Last updated: 2026-05-25 by Codex
> Current phase: Post-completion real-state correction PR5 in progress
> Source ADR:
> `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

---

## Quick View

| Phase | Theme | Status | Owner Session | Worktree / Branch | Next Action |
|---|---|---|---|---|---|
| Phase 0 | Contracts, flags, and projection skeleton | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts` / `codex/browser-runtime-phase0-contracts` | Closed; contract regressions stay in every later browser-runtime phase. |
| Phase 1 | Supervisor around current chromiumoxide runtime | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor` / `codex/browser-runtime-phase1-supervisor` | Closed for shell slice; later wiring slices must use this supervisor surface. |
| Phase 2 | App-managed Playwright runtime pack | Runtime-pack shell through Phase 2F plus Phase 5B-preflight real runner/probe adapters merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-runtime-pack-runner` / `codex/browser-runtime-phase5b-preflight-runtime-pack-runner` | Closed for app-managed manifest/status, strict readiness, real local runner/probe adapters, and policy-gated executor boundary. Provider execution continues through Phase 5+. |
| Phase 3 | Startup Splash, Startup Doctor, and shell UX | Phase 3A-3C and 3E-3H merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3h-app-startup-route` / `codex/browser-runtime-phase3h-app-startup-route` | Closed for branded root startup route; later recovery/deep-link work must build on the merged Startup Splash route. |
| Phase 4 | Browser Runtime settings and task-time preparation UX | Phase 4A-4X merged to `main` / `origin/main`; exit audit complete | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4x-settings-action-dry-run-ipc` / `codex/browser-runtime-phase4x-settings-action-dry-run-ipc` | Closed for user-visible Settings, Doctor, prompt, checkpoint, deep-link, read-only IPC, and dry-run action evidence. Real runtime execution/provider work moves to Phase 5+. |
| Phase 5 | Playwright CLI thin lane behind a feature flag | Phase 5A-5F merged to `main` / `origin/main`; exit gate complete | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff` / `codex/browser-runtime-phase5f-action-state-diff` | Closed for feature-flagged Playwright CLI thin lane. Provider promotion and parity routing remain Phase 8. |
| Phase 6 | Browser identity authorization and profile UX | Phase 6A-6I merged to `main` / `origin/main`; ADR Phase 6 gate complete | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6i-payment-confirmation-harness` / `codex/browser-runtime-phase6i-payment-confirmation-harness` | Closed for authorize/reuse/status/active-task/revoke/drain/checkpoint/resume, generic identity authorization completion, and payment-confirmation harness evidence. |
| Phase 7 | Playwright MCP sidecar behind a feature flag | Phase 7A-7G merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7g-mcp-selection-policy` / `codex/browser-runtime-phase7g-mcp-selection-policy` | Closed for MCP sidecar, stdio action boundary, artifact/error routing, and MCP-vs-CLI selection guardrail. |
| Phase 8 | Provider abstraction, parity harness, and default selection | Phase 8A-8J merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8j-provider-default-policy` / `codex/browser-runtime-phase8j-provider-default-policy` | Closed for provider route evidence and reversible default policy; Phase 9 recipe work starts from merge commit `cab8f161`. |
| Phase 9 | Recipes, locator cache, and domain-skill candidates | Phase 9A-9E merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9e-harness-matrix` / `codex/browser-runtime-phase9e-harness-matrix` | Closed for pure recipe/domain-skill harness coverage; no production replay, locator persistence, or domain-skill writes were introduced. |
| Phase 10 | Optional hosted providers and hard-site escape hatches | Phase 10A-10B merged to `main` / `origin/main`; ADR Phase 10 gate complete | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase10b-hosted-provider-harness` / `codex/browser-runtime-phase10b-hosted-provider-harness` | Closed for hosted-provider capability contract plus disabled fallback, data-boundary prompt, artifact capture, cost visibility, local fallback, and opt-in mock-hosted harness evidence. |
| Real State PR1 | Rust aggregated runtime status service | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr1` / `codex/browser-runtime-real-state-pr1` | Closed as PR #503; later real-state PRs consume the aggregate status source. |
| Real State PR2 | Splash/App Rust-state handoff | Open as PR #504; review gate pending | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr2-splash-app-state` / `codex/browser-runtime-real-state-pr2-splash-app-state` | Fresh review must accept the GitNexus HIGH root-App handoff impact before merge. |
| Real State PR3 | Task-time runtime status routing | Merged to `main` / `origin/main` as PR #506 | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr3-task-runtime-status` / `codex/browser-runtime-real-state-pr3-task-runtime-status` | Closed; autonomous Browser task actions consume aggregate runtime status before provider routing. |
| Real State PR4 | Browser Panel runtime projection | Open as PR #507; CRITICAL review gate pending | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr4-direct-tool-guard` / `codex/browser-runtime-real-state-pr4-direct-tool-guard` | Fresh review must accept the BrowserPanel/BrowserStatusBar CRITICAL impact before merge. |
| Real State PR5 | UI command runtime touch | In progress | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr5-ui-command-runtime-touch` / `codex/browser-runtime-real-state-pr5-ui-command-runtime-touch` | Route supported Browser UI actions through runtime status/provider executor and status-touch the remaining direct UI IPC commands. |

---

## Post-Completion Real-State Correction

### PR1 - Rust Aggregated Runtime Status

- Entry criteria: a main-branch audit found that ADR implementation was
  over-claimed in tracker text. The code has contracts, status DTOs, dry-run
  surfaces, and harness evidence, but the live app still does not use one Rust
  `BrowserRuntimeSupervisor` truth source from Splash through browser action
  surfaces.
- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr1.md`.
- Scope: add a shared Rust status service and wire
  `get_browser_runtime_status` to the aggregated service while preserving the
  existing runtime-pack response fields.
- Progress: `AppState` now owns a shared `BrowserRuntimeStatusService`.
  `get_browser_runtime_status` returns a flattened superset of the existing
  runtime-pack report with Rust supervisor status, provider readiness summary,
  world projection, and supervisor event names.
- Verification:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
    passed: `3 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
    passed: `42 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_supervisor`
    passed: `7 passed`.
  - `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/runtime_status.rs`
    passed.
  - `git diff --check -- src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/runtime_status.rs src-tauri/src/browser/mod.rs src-tauri/src/app.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr1.md`
    passed.
  - GitNexus `detect_changes(scope=all)` passed after indexing this worktree:
    LOW risk, `affected_count: 0`, no affected execution flows.
- Explicit non-scope: no `App.tsx` handoff change, no Settings real execution,
  no runtime-pack download/delete, no provider default promotion, no
  BrowserPanel/screencast routing changes, and no TaskEvent persistence.
- Closeout: merged as PR #503 into `origin/main` at merge commit `52808ed1`.

### PR2 - Splash/App Rust-State Handoff

- Status: open as PR #504 with required fresh-review gate because GitNexus
  reports HIGH root-`App` startup handoff impact.
- Scope: move startup Browser Runtime status ownership to `App`, pass the Rust
  status/failure/loading state into `StartupSplash`, and keep the app shell
  handoff blocked until initialization, minimum splash visibility, and runtime
  status completion or bounded timeout fallback.
- Explicit non-scope: no runtime-pack execution, no Settings action execution,
  no provider default promotion, no BrowserPanel/screencast routing changes, no
  browser action supervisor guard, and no TaskEvent persistence.
- PR: https://github.com/novolei/uclaw-new/pull/504.

### PR3 - Task-Time Runtime Status Routing

- Entry criteria: PR1 created the aggregate Rust status source, but
  `BrowserAgentLoop` still builds `BrowserProviderActionExecutor` with default
  route options. Task-time browser action routing therefore has no live
  runtime-pack readiness snapshot from `BrowserRuntimeStatusService`.
- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr3-task-runtime-status.md`.
- PR: https://github.com/novolei/uclaw-new/pull/506.
- Scope: pass `BrowserRuntimeStatusService` into autonomous Browser task tools
  and let `BrowserAgentLoop` route each task-time action with a fresh
  runtime-pack report from the Rust aggregate status source.
- Progress: `BrowserAgentLoop` now accepts an optional
  `BrowserRuntimeStatusService`, inspects the aggregate Rust runtime status
  before each task-time provider route decision, and converts the live
  runtime-pack report into `BrowserProviderActionRouteOptions`. Browser task,
  resume, and retry tools receive the shared service from both chat and Agent
  session tool registries.
- Impact notes: GitNexus pre-edit impact for `BrowserAgentLoop`,
  `BrowserProviderActionExecutor`, `BrowserTaskTool`,
  `BrowserTaskResumeTool`, and `RetryWithBrowserAgentTool` was LOW with no
  affected execution flows. `send_message` and `send_agent_message` could not
  be symbol-impact checked because GitNexus skips `tauri_commands.rs` as the
  one large file over its 512KB analyzer threshold; the PR keeps that file to
  eight registration lines and validates it with compile tests and
  `git diff --check`.
- Verification:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
    passed: `15 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
    passed: `8 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
    passed: `3 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
    passed: `14 passed`.
  - `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs`
    passed.
  - `git diff --check -- src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs src-tauri/src/tauri_commands.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr3-task-runtime-status.md`
    passed.
  - `npx gitnexus analyze` in the PR3 worktree passed:
    `39,072 nodes`, `65,106 edges`, `300 flows`; analyzer skipped one large
    file (`tauri_commands.rs`).
  - GitNexus `detect_changes(scope=all)` after indexing the PR3 worktree
    reported LOW risk, `changed_count: 29`, `affected_count: 0`, and no
    affected execution flows.
- Explicit non-scope: no Settings real execution, no direct browser tool
  supervisor guard, no runtime-pack install/repair/delete, no provider default
  promotion, no Startup Splash handoff, and no TaskEvent persistence.
- Closeout: merged as PR #506 into `origin/main` at merge commit `685d15ad`.

### PR4 - Browser Panel Runtime Projection

- Status: open as PR #507 with required fresh-review gate because GitNexus
  reported CRITICAL impact for the BrowserPanel/BrowserStatusBar path.
- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr4-browser-panel-status.md`.
- Scope: let Browser Panel fetch `getBrowserRuntimeStatus` and render a compact
  Rust supervisor/provider status chip in the status bar while keeping browser
  view, navigation, DOM overlay, and direct command behavior unchanged.
- Verification:
  - `cd ui && npm test -- --run src/components/browser/BrowserPanel.test.tsx src/components/browser/BrowserStatusBar.test.tsx`
    passed: `2 files / 4 tests`.
  - `cd ui && npm run build` passed with existing dynamic-import and chunk-size
    warnings.
  - `npx gitnexus analyze` in the PR4 worktree passed:
    `39,101 nodes`, `65,167 edges`, `300 flows`; analyzer skipped
    `tauri_commands.rs`.
  - GitNexus staged `detect_changes` reported LOW risk with
    `changed_count: 19`, `affected_count: 0`, and no affected flows.
- PR: https://github.com/novolei/uclaw-new/pull/507.
- Explicit non-scope: no backend execution routing, no direct IPC command guard,
  no Splash/App handoff, no runtime-pack side effects, and no provider default
  promotion.

### PR5 - UI Command Runtime Touch

- Entry criteria: PR3 routes autonomous Browser task actions through aggregate
  runtime status, but direct Browser UI IPC commands still call
  `BrowserContextManager` without consulting the shared Rust runtime status.
- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr5-ui-command-runtime-touch.md`.
- Scope: Browser UI IPC commands read `BrowserRuntimeStatusService` before
  execution. `browser_ui_navigate` and `browser_ui_switch_tab` route through
  `BrowserProviderActionExecutor` because they have existing `BrowserAction`
  equivalents with compatible return behavior. Commands whose semantics are not
  yet represented by `BrowserAction` status-touch the runtime and keep the
  current direct Chromium behavior.
- Impact notes: GitNexus cannot resolve individual `tauri_commands.rs` IPC
  functions because the analyzer skips that large file. Indexed support symbols
  reported LOW impact: `BrowserProviderActionExecutor`,
  `BrowserRuntimeStatusService`, `BrowserProviderActionRouteOptions`, and
  `BrowserActionResult`.
- Verification:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib tauri_commands::browser_ui_runtime_command_tests`
    passed: `2 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
    passed: `3 passed`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
    passed: `8 passed`.
  - `git diff --check -- src-tauri/src/tauri_commands.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr5-ui-command-runtime-touch.md`
    passed.
  - `rustfmt --edition 2021 --check src-tauri/src/tauri_commands.rs` was not a
    usable PR5 verification because the large file is not currently rustfmt
    clean and rustfmt proposed thousands of unrelated pre-existing line
    rewrites.
  - Local worktree test setup required gitignored symlinks to main checkout
    runtime resources: `src-tauri/pyembed`, `src-tauri/gbrain-source`, and
    `src-tauri/bunembed`.
- Explicit non-scope: no coordinate action provider mapping, no back/forward/
  reload provider action contract, no screencast provider contract, no legacy
  `BrowserService` rewrite, no TaskEvent persistence, no provider promotion, and
  no PR2/PR4 frontend stacking.

---

## Post-Completion Manual UX Follow-up: Startup Splash

### Minimum Visibility

- PR #501 merged as `7c14c2d1` from `codex/startup-splash-min-duration`.
- Entry criteria: manual frontend verification found the production Startup
  Splash flashed by too quickly to communicate brand, readiness, progress, or
  Browser Runtime diagnostic affordances.
- Progress: `App` keeps Startup Splash mounted for a minimum visible interval
  and uses a short opacity handoff before rendering `AppShell`.
- Impact notes: GitNexus pre-edit impact for `App` was LOW. Post-edit
  `detect-changes` was HIGH because root `App` participates in top-level
  startup/listener/settings/model flows; the diff stayed limited to startup
  timing and preserved the existing initialization calls.
- Verification notes:
  - `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx`
    passed: `2 files / 13 tests`.
  - `cd ui && npm run build` passed with existing chunk-size / dynamic-import
    warnings.
  - Browser smoke under `npm run dev:mock-tauri` showed Splash present during
    the minimum-visible window, then exiting before handoff.

### If2Ai Visual Port

- Current PR branch: `codex/startup-splash-if2ai-port`.
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/startup-splash-if2ai-port`.
- Entry criteria: after PR #501 made the Splash perceptible, manual validation
  requested that uClaw reuse the more polished If2Ai splash visual system from
  `/Users/ryanliu/Documents/IfAI/if2Ai/src/components/loading/If2AiLoadingScreen.tsx`.
- Progress: `StartupSplash` now ports the If2Ai warm sand background, drifting
  grid, centered icon tile, orange glow wordmark, wave-dot loader, and build
  telemetry while preserving uClaw's existing Startup Doctor status, progress,
  recovery panel, details expansion, and Browser Runtime Settings affordance.
- Impact notes: GitNexus pre-edit impact for `StartupSplash` was LOW: 2 direct
  callers (`startup-splash-preview.tsx`, `StartupSplash.test.tsx`), 0 affected
  execution flows. No `App.tsx`, IPC, backend, runtime-pack, provider, or
  policy boundary changed in this visual port.
- Verification notes:
  - `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx src/components/startup/startup-splash-scenarios.test.ts`
    passed: `3 files / 17 tests`.
  - `cd ui && npm run build` passed with existing dynamic-import and chunk-size
    warnings.
  - Browser preview smoke under `npm run dev:mock-tauri` captured desktop
    first-frame, desktop failed, and mobile failed screenshots:
    `/tmp/uclaw-if2ai-splash-first-frame.png`,
    `/tmp/uclaw-if2ai-splash-failed-desktop.png`, and
    `/tmp/uclaw-if2ai-splash-failed-mobile.png`; mobile check reported no
    horizontal overflow and accessible heading `uClaw`.
  - `git diff --check -- <changed files>` passed.
  - GitNexus staged detect after indexing this worktree reported LOW:
    `changed_count: 21`, `affected_count: 0`, no affected execution flows.
- Next action: commit, push, and open the narrow visual-port PR.

---

## Live Decision Log

| Date | Decision | Evidence | Effect |
|---|---|---|---|
| 2026-05-25 | Start Real State PR3 from `origin/main` while PR #504 awaits review. | PR #504 is CLEAN/MERGEABLE but has no fresh review; PR3 backend task-time routing depends only on PR #503's aggregate Rust status source. | Continue real-state convergence without stacking frontend PR2 changes; PR3 focuses on `BrowserAgentLoop` provider routing status injection. |
| 2026-05-25 | Reopen Browser Runtime work as a post-completion real-state correction instead of treating the verified-complete tracker as sufficient. | Main-branch audit after PR #502 found live-path gaps: Splash reads status but App handoff does not depend on Rust runtime state; browser actions and UI commands still bypass a shared supervisor service. | Start with PR1 as a narrow Rust status-source slice, then use later PRs for App handoff, real prepare execution, browser call-site guards, and provider/World Projection routing. |
| 2026-05-23 | Implement Browser Runtime Supervisor as phased PR slices, not one broad rewrite. | Browser Runtime Supervisor ADR section 12 and user request for phase-pack execution. | Each phase gets a plan, status row, verification notes, and reversible commit boundary. |
| 2026-05-25 | Add a post-completion Startup Splash minimum-visibility fix from manual validation. | Manual frontend verification found the production Startup Splash flashes by too quickly to communicate brand, readiness, or Browser Runtime diagnostics. | Keep ADR phases complete, but track this as a narrow UX quality follow-up: `codex/startup-splash-min-duration` / `/Users/ryanliu/Documents/uclaw-worktrees/startup-splash-min-duration`. |
| 2026-05-25 | Port the If2Ai splash visual design into uClaw after minimum visibility landed. | User requested copying `/Users/ryanliu/Documents/IfAI/if2Ai` splash design into uClaw while adapting uClaw's current Startup Doctor UX logic. | Keep Browser Runtime phases closed; track this as a second narrow post-completion UX follow-up on `codex/startup-splash-if2ai-port` with no App routing, IPC, provider, or runtime side effects. |
| 2026-05-23 | Phase 0 is metadata/contracts only. | ADR Phase 0 expected outcome says later phases add behavior behind stable contracts. | No Playwright process, MCP sidecar, browser launch, Tauri command, DB migration, or UI wiring in Phase 0. |
| 2026-05-23 | Use this file as the Browser Runtime close-loop tracker. | User asked to follow the `AGENT_OS_JCODE_UPGRADE_STATUS.md` tracker pattern. | Every Browser Runtime phase must update Quick View, branch hygiene, progress, and verification notes. |
| 2026-05-23 | Rebase Phase 0 worktree onto latest `main` before commit. | Worktree initially had ADR commit on older merge-base `3d710297`; latest `main` was `d7a9527`. | Phase branch now has latest `main` plus rebased Browser ADR commit `4cb7538`, then Phase 0 WIP reapplied. |
| 2026-05-23 | Start Phase 1 as a supervisor shell before hot-path rewiring. | Phase 1 ADR is broad; narrow first PR should make runtime states, deadlines, doctor classification, artifacts, and projection available without changing action execution. | Later Phase 1 follow-ups can route action dispatch through the supervisor once the shell is tested. |
| 2026-05-23 | Phase 0 and Phase 1 are now merged into `main` and `origin/main`. | `main` and `origin/main` both point at `81d9b9dc Merge remote-tracking branch 'origin/codex/browser-runtime-phase1-supervisor'`. | Phase 2 starts from `origin/main` instead of the older Phase 1 worktree base. |
| 2026-05-23 | Phase 2 begins with a local runtime-pack manifest/status/doctor shell. | ADR Phase 2 includes install/repair/cleanup/rollback, but the first reversible slice should avoid network download, worker execution, and UI. | The first Phase 2 PR proves pack state classification before adding download or Playwright process behavior. |
| 2026-05-23 | Continue Phase 2 with an operation planner before side effects. | PR #414 merged the manifest/status/doctor shell; ADR Phase 2 still needs install, repair, cleanup, rollback, network confirmation, active-task protection, and rollback retention. | Phase 2B adds a pure plan boundary for Splash, Settings, and future executors without downloading, extracting, deleting, or launching Playwright. |
| 2026-05-23 | Add a dry-run executor before real side effects. | PR #415 merged the operation planner; the next safe step is an execution report boundary that proves policy gating and artifact/event metadata before real downloads or deletes. | Phase 2C keeps execution auditable and side-effect free while preparing the seam for later real executor adapters. |
| 2026-05-23 | Add a read-only filesystem probe before real installation. | PR #416 merged the dry-run executor; Startup Doctor and Settings still need local pack evidence without launching Playwright or mutating files. | Phase 2D loads the runtime manifest, probes expected pack paths, detects version mismatch/corrupt manifests, and feeds the existing doctor. |
| 2026-05-23 | Add a status-report aggregator before UI wiring. | PR #417 merged the filesystem probe; Startup Doctor and Settings need one queryable runtime status contract, not direct knowledge of every probe/doctor/planner step. | Phase 2E composes filesystem, doctor, primary action, operation plan, and event names without emitting events or mutating runtime files. |
| 2026-05-24 | Add a managed executor boundary before real side effects. | PR #418 merged the status report; ADR Phase 2 still needs install/repair/cleanup/rollback, but runtime mutations need an explicit policy-gated runner seam first. | Phase 2F adds managed execution DTOs, policy gates, and a step-runner boundary without downloading, deleting, extracting, or launching Playwright. |
| 2026-05-24 | Split Phase 3 into a small 3A startup-shell substrate. | PR #419 merged Phase 2F; ADR Phase 3 includes branded splash, doctor, background preparation, recovery, and screenshots, which is too broad for one PR. | Phase 3A adds a typed frontend Startup Doctor view model and loading shell only; real backend doctor IPC, settings, root error recovery, and final asset polish stay out of scope. |
| 2026-05-24 | Defer root `App` loading-state integration from Phase 3A to Phase 3B. | An attempted `App` loading-state swap had LOW pre-change impact but staged GitNexus detect returned HIGH because `App` affects 9 top-level listener/settings/runtime processes. | Phase 3A remains additive and mergeable; Phase 3B must explicitly review the `App` blast radius before wiring the shell into app startup. |
| 2026-05-24 | Do runtime-pack status mapping before root `App` integration. | PR #420 merged the additive Startup Splash substrate; root `App` wiring still has known HIGH staged-detect risk, while ADR Phase 3 also requires Startup Doctor to consume runtime-pack state. | Phase 3B adds a pure frontend adapter from Phase 2 runtime-pack status reports into Startup Doctor checks, with no IPC, `App`, Settings, or runtime side effects. |
| 2026-05-24 | Add a standalone preview harness before root `App` integration. | PR #421 merged Phase 3B runtime-pack status mapping; root `App` wiring still has known HIGH staged-detect risk, while ADR Phase 3 also requires screenshot gates. | Phase 3C adds deterministic preview scenarios and a standalone Vite page for first-frame/details/ready/deferred/failed checks without changing production startup routing. |
| 2026-05-24 | Stop root `App` route after HIGH staged detect and continue Phase 3 below `App`. | PR #422 merged Phase 3C; a Phase 3D proposal replacing the root loading spinner with `StartupSplash` passed focused tests but final staged GitNexus detect reported HIGH because `App` affects 9 top-level app processes. | Do not retry root `App` startup routing without explicit HIGH-risk review; Phase 3E advances recovery surfaces inside `StartupSplash`, whose impact is LOW. |
| 2026-05-24 | Close Phase 3E and stop the PR chain before Phase 4. | PR #423 merged Phase 3E recovery surfaces; Phase 4 is explicitly gated on the Phase 3 shell route, and the only remaining shell-route integration attempt is blocked by GitNexus HIGH. | Phase 3F records the reviewer-plan requirement; no further implementation should proceed until the root `App` blast radius is explicitly accepted. |
| 2026-05-24 | Prepare a root `App` review acceptance pack instead of editing `App`. | PR #424 merged the Phase 3F gate note; `BEHAVIOR.md` section 8 requires writer/reviewer flow for anything flagged HIGH/CRITICAL by GitNexus. | Phase 3G defines the future writer scope, reviewer prompt, and go/no-go gates; implementation remains blocked until explicit acceptance. |
| 2026-05-24 | Start Phase 3H as the writer half of the root `App` startup route. | PR #425 merged the Phase 3G acceptance pack; pre-edit GitNexus impact for `App` in the Phase 3H worktree reported LOW risk. | Phase 3H may edit only the root loading branch and must leave merge/acceptance to a fresh reviewer if final staged detect reports the known HIGH blast radius. |
| 2026-05-24 | Accept and merge the Phase 3H root startup route, then start Phase 4A as a readonly settings substrate. | A fresh reviewer sub-agent returned `REVIEW ACCEPTED` for PR #426, confirming listener registration, settings/model initialization, AppShell handoff, and root error behavior were preserved; PR #426 merged as `13133bb1`. | Phase 4 can begin, but starts with a reversible Settings tab/view-model slice before IPC, deep links, task-time prompts, or runtime side effects. |
| 2026-05-24 | Accept and merge the Phase 4A readonly Browser Runtime settings surface. | A fresh reviewer sub-agent returned `REVIEW ACCEPTED` for PR #427, confirming Settings navigation, tab rendering, badges, SettingsPanel handoff, and no runtime side effects were preserved; PR #427 merged as `5e0f18fb`. | Phase 4B can build on the settings destination, but still must avoid IPC, deep links, task-time prompts, provider promotion, and real runtime mutations. |
| 2026-05-24 | Start Phase 4B as local action-intent previews, not execution. | Phase 4A exposed inert action affordances; ADR Phase 4 still needs user-understandable prepare/repair/reinstall/cleanup/rollback controls before backend execution is safe. | Phase 4B may make enabled buttons select preview metadata only; execution, policy prompts, SearchPalette/Startup Doctor deep links, task checkpoints, and TaskEvents remain later phases. |
| 2026-05-24 | Merge Phase 4B action-intent previews and start Phase 4C auto-prepare semantics. | PR #428 merged as `d3f9f995`; ADR Phase 4 still requires disable-auto-prepare controls and explicit semantics that browser automation remains available for task-time prompts. | Phase 4C adds only local auto-prepare control previews; settings persistence, IPC, deep links, and task checkpointing remain later Phase 4 slices. |
| 2026-05-24 | Merge Phase 4C auto-prepare semantics and start Phase 4D as a pure task-time prompt model. | PR #429 merged as `50b5ab8f`; ADR Phase 4 still requires task-time prepare/defer/no-browser decisions and `paused_waiting_for_browser_runtime` checkpoint semantics. | Phase 4D adds a pure frontend model only; UI rendering, IPC, TaskEvents, and actual checkpoint writes remain later slices. |
| 2026-05-24 | Merge Phase 4D task-time prompt model and start Phase 4E as additive prompt UI. | PR #430 merged as `7d4f70e0`; ADR Phase 4 still needs users to see and select the task-time choices before backend wiring is safe. | Phase 4E renders the prompt model only; App/task runtime wiring, IPC, TaskEvents, deep links, and checkpoint writes remain later slices. |
| 2026-05-24 | Merge Phase 4E task-time prompt UI and start Phase 4F as the first settings deep-link slice. | PR #431 merged as `ab59f9aa`; AppShell already had a SearchPalette settings TODO, and ADR Phase 4 requires Settings deep links from multiple surfaces. | Phase 4F wires SearchPalette only; Startup Doctor, task-time prompt, error/recovery links, IPC, TaskEvents, and checkpoint writes remain later slices. |
| 2026-05-24 | Merge Phase 4F SearchPalette deep link and start Phase 4G as a Startup Doctor component callback. | PR #432 merged as `00ce02ed`; ADR Phase 4 still requires Settings deep links from Startup Doctor, task-time prompts, and error/recovery surfaces. | Phase 4G adds only the StartupSplash callback/button contract for browser-runtime doctor attention; root `App`, IPC, TaskEvents, task checkpoints, and runtime side effects remain later slices. |
| 2026-05-24 | Merge Phase 4G Startup Doctor deep link and start Phase 4H as a task-time prompt component callback. | PR #433 merged as `5dd0745c`; ADR Phase 4 still requires Settings deep links from task-time runtime prompts and error/recovery surfaces. | Phase 4H adds only the task-time prompt callback/button contract; App/task runtime wiring, IPC, TaskEvents, checkpoint writes, and runtime side effects remain later slices. |
| 2026-05-24 | Open Phase 4H task-time prompt settings deep-link PR. | PR #434 contains the optional `BrowserRuntimeTaskTimePrompt` settings callback/button, focused prompt tests, and the Phase 4H plan/tracker update. | Merge if GitHub reports CLEAN and checks pass; then continue to the remaining Phase 4 error/recovery deep-link surface before backend task checkpointing. |
| 2026-05-24 | Merge Phase 4H and start Phase 4I as a structured error/recovery action contract. | PR #434 merged as `bf6a4693`; ADR Phase 4 still requires Settings deep links from error/recovery surfaces. GitNexus pre-edit impact for `ErrorMessage` is HIGH, and fresh reviewer Ptolemy returned `REVIEW ACCEPTED`. | Phase 4I may add only `open_browser_runtime_settings` handling in the existing frontend error/recovery action switch; backend IPC, TaskEvents, task checkpoints, and runtime side effects remain out of scope. |
| 2026-05-24 | Open Phase 4I error recovery settings deep-link PR. | PR #435 contains `open_browser_runtime_settings`, focused direct/grouped renderer tests, final HIGH GitNexus detect notes, and reviewer acceptance from Ptolemy and Jason. | Merge if GitHub reports CLEAN; next Phase 4 work should plan task checkpoint/defer semantics separately because it may touch task runtime boundaries. |
| 2026-05-24 | Merge Phase 4I and start Phase 4J as a backend paused-waiting contract. | PR #435 merged as `ab65fab8`; ADR Phase 4 still requires deferral to checkpoint tasks as `paused_waiting_for_browser_runtime` unless a no-browser fallback can satisfy the request. | Phase 4J adds only the browser task status string and rollout conversion contract; agent-loop wiring, prompt dispatch, IPC, and real checkpoint writes remain later slices. |
| 2026-05-24 | Open Phase 4J paused-waiting runtime contract PR. | PR #436 contains `PausedWaitingForBrowserRuntime`, task-store roundtrip coverage, rollout bridge checkpoint/boundary conversion, and Phase 4J plan/tracker updates. GitNexus staged detect is LOW with 0 affected processes. | Merge if GitHub reports CLEAN; next slice should plan task-runtime wiring separately before any real prompt dispatch or checkpoint side effects. |
| 2026-05-24 | Merge Phase 4J and start Phase 4K as frontend paused-waiting projection. | PR #436 merged as `1f8739ec`; backend can now emit `paused_waiting_for_browser_runtime`, but frontend Browser task projection types and monitor copy do not yet recognize that status. | Phase 4K is deliberately projection-only because true prompt dispatch/runtime gating touches browser task execution boundaries; Phase 4L should own that wiring after a separate plan. |
| 2026-05-24 | Open Phase 4K paused-waiting task projection PR. | PR #437 contains frontend status union updates, BrowserTaskMonitor readable waiting-runtime rendering, focused monitor/hook tests, and reviewer acceptance for the CRITICAL pre-edit UI path. | Merge if GitHub reports CLEAN; next slice must not be folded into this PR and should plan task-runtime prompt dispatch separately. |
| 2026-05-24 | Merge Phase 4K and start Phase 4L as an explicit task runtime pause gate. | PR #437 merged as `5bd56ba1`; frontend projection is ready for `paused_waiting_for_browser_runtime`, but no backend path can yet create that state at task time. | Phase 4L adds only an explicit `browser_task` defer decision and pause-before-browser checkpoint. Prompt dispatch, Settings IPC, runtime-pack execution, no-browser fallback, provider promotion, and DMZ edits remain later slices. |
| 2026-05-24 | Open Phase 4L task runtime pause gate PR. | PR #438 contains the explicit `browser_task` defer gate, paused-waiting checkpoint behavior, focused parser/pause tests, runtime regressions, and staged GitNexus LOW detect. | Merge if GitHub reports CLEAN; Phase 4M should not fold into this PR and should plan real prompt dispatch / IPC separately. |
| 2026-05-24 | Merge Phase 4L and start Phase 4M as a typed prompt/action decision bridge. | PR #438 merged as `a566decf`; backend can pause on explicit `runtime_preparation_decision: "defer"`, but frontend prompt actions do not yet carry that backend-ready payload. | Phase 4M adds model metadata only. Tool-call mutation, prompt dispatch, Settings IPC, runtime-pack execution, no-browser fallback execution, provider promotion, and DMZ edits remain later slices. |
| 2026-05-24 | Open Phase 4M task-time decision bridge PR. | PR #439 contains typed prompt-action metadata that maps checkpointed defer to `runtime_preparation_decision: "defer"`, focused prompt tests, default Rust regressions, and staged GitNexus `risk_level: none`. | Merge if GitHub reports CLEAN; next Phase 4 slice must separately plan prompt dispatch / tool-call mutation because that may touch task-runtime or approval boundaries. |
| 2026-05-24 | Merge Phase 4M and start Phase 4N as a pure tool-call patch boundary. | PR #439 merged as `a0dd62e5`; frontend prompt actions now carry the defer payload, but dispatch/approval hot paths still need a clean helper boundary before wiring. | Phase 4N adds only a pure helper for applying defer metadata to serialized `browser_task` arguments. Prompt dispatch, tool approval mutation, Settings IPC, backend execution, and DMZ edits remain later slices. |
| 2026-05-24 | Open Phase 4N task-time tool-call patch boundary PR. | PR #440 contains pure `browser_task` argument patch helpers, focused prompt-model coverage, default Rust regressions, and staged GitNexus LOW detect. | Merge if GitHub reports CLEAN; actual prompt dispatch / approval wiring remains a separate Phase 4 slice. |
| 2026-05-24 | Merge Phase 4N and start Phase 4O as a prompt-dispatch review pack. | PR #440 merged as `08a2b65f`; GitNexus impact for `run_agentic_loop` is HIGH and the file is DMZ, while `dispatcher.rs::execute_tool_calls` and `BrowserTaskTool.execute` are LOW. | Phase 4O is docs-only and records writer/reviewer gates before any prompt-dispatch implementation. Do not edit `agentic_loop.rs` without fresh reviewer acceptance. |
| 2026-05-24 | Open Phase 4O prompt-dispatch review pack PR. | PR #441 contains only tracker/plan changes, default Rust regressions, whitespace checks, and staged GitNexus LOW detect. Fresh reviewer Popper is reviewing the HIGH/DMZ gate plan. | Merge if PR #441 is CLEAN and reviewer accepts; next implementation PR must keep to an accepted low-risk dispatch boundary or stop again for reviewer approval. |
| 2026-05-24 | Merge Phase 4O and start Phase 4P as dispatcher-only prompt patch wiring. | PR #441 merged as `4d67f487`; reviewer Popper returned `REVIEW ACCEPTED`; Phase 4O allowed a `dispatcher.rs`-only implementation and GitNexus impact for `ChatDelegate.execute_tool_calls` is LOW. | Phase 4P may normalize serialized Browser task prompt patches before approval/execution, but must not edit `agentic_loop.rs`, IPC, DB migrations, runtime-pack execution, or provider selection. |
| 2026-05-24 | Open Phase 4P task-time dispatch patch boundary PR. | PR #442 contains dispatcher-only Browser task runtime prompt patch normalization, focused dispatcher tests, default browser-runtime regressions, and staged GitNexus LOW detect. | Merge if GitHub reports CLEAN; future prompt/UI/IPC wiring remains separate and must not be folded into PR #442. |
| 2026-05-24 | Merge Phase 4P and start Phase 4Q as pure task-time dispatch-effect modeling. | PR #442 merged as `1fd68675`; backend dispatcher can now consume serialized Browser task request patches. | Phase 4Q gives `prepare_now`, `defer`, and `continue_without_browser` typed frontend dispatch effects without live agent-loop wiring, IPC, runtime-pack execution, or no-browser execution. |
| 2026-05-24 | Open Phase 4Q task-time dispatch effects PR. | PR #443 contains typed frontend dispatch effects for prepare/defer/no-browser decisions, focused prompt-model tests, prompt UI regression coverage, default Rust browser-runtime regressions, and GitNexus staged detect LOW with no affected processes. | Merge if GitHub reports CLEAN; next Phase 4 work should not fold IPC, runtime-pack execution, no-browser execution, or DMZ task-loop wiring into PR #443. |
| 2026-05-24 | Merge Phase 4Q and start Phase 4R as a Settings IPC review pack. | PR #443 merged as `1db302be`; Phase 4 still needs real Settings/Doctor status and action IPC, but backend command wiring touches DMZ `tauri_commands.rs`, while shared frontend `getSettings` impact is HIGH. | Phase 4R records writer/reviewer gates only; no Tauri command, frontend invoke, runtime-pack execution, provider promotion, or user-data mutation in this PR. |
| 2026-05-24 | Open Phase 4R Settings IPC review pack PR. | PR #444 contains only the tracker and Phase 4R plan, records DMZ/HIGH impact gates, default Rust regressions, whitespace checks, and GitNexus staged detect LOW. | Merge only if GitHub reports CLEAN and the fresh reviewer accepts the writer/reviewer pack. |
| 2026-05-24 | Merge Phase 4R and start Phase 4S as a read-only status IPC slice. | PR #444 merged as `c4a31567`; reviewer Noether returned `REVIEW ACCEPTED`; GitNexus impact for `main` and `inspect_runtime_pack_status` is LOW, while `getSettings` remains HIGH and must stay untouched. | Phase 4S may add a dedicated read-only Browser Runtime status command and frontend bridge, but no Settings live wiring, runtime-pack execution, provider promotion, or shared settings initialization changes. |
| 2026-05-24 | Open Phase 4S readonly status IPC PR. | PR #445 contains the dedicated read-only `get_browser_runtime_status` command, standalone frontend bridge, focused Rust/UI tests, default browser-runtime regressions, and GitNexus staged detect MEDIUM with no HIGH/CRITICAL. | Merge if GitHub reports CLEAN; next Phase 4 slice should consume this command from Settings or Doctor without folding in action execution, provider promotion, or shared `getSettings` changes. |
| 2026-05-24 | Merge Phase 4S and start Phase 4T as a Settings clarity follow-up. | PR #445 merged as `88f552f3`; a fresh PR #427 reviewer accepted the HIGH blast radius but flagged ambiguous update-state/developer-fallback rows as Important. | Phase 4T fixes only the display ambiguity in Browser Runtime Settings; live IPC consumption, runtime actions, provider promotion, and shared `getSettings` changes remain later slices. |
| 2026-05-24 | Open Phase 4T Settings status clarity PR. | PR #446 contains first-class Settings rows for update state and developer fallback state, focused Settings component tests, default browser-runtime regressions, and GitNexus staged detect LOW with 0 affected processes. | Merge if GitHub reports CLEAN; next Phase 4 slice should consume the read-only status command from Settings or Doctor without folding in runtime action execution. |
| 2026-05-24 | Merge Phase 4T and start Phase 4U as a Settings live status read. | PR #446 merged as `aa6838d6`; Phase 4S already added a dedicated read-only status bridge, and the Settings rows are now unambiguous. | Phase 4U may call `getBrowserRuntimeStatus` from Browser Runtime Settings, but must not wire action execution, shared `getSettings`, Startup Doctor, provider promotion, or backend mutations. |
| 2026-05-24 | Open Phase 4U Settings live status read PR. | PR #447 contains the read-only Settings bridge call, explicit-status preview bypass, focused Settings tests, default browser-runtime regressions, and GitNexus staged detect LOW with 0 affected processes. | Merge if GitHub reports CLEAN; next Phase 4 slice should continue read-only Doctor/Settings status consumption or plan action execution separately. |
| 2026-05-24 | Merge Phase 4U and start Phase 4V as a Startup Doctor live status read. | PR #447 merged as `ffc7b811`; Settings now consumes the dedicated read-only status bridge, while Startup Doctor still rendered static/default status on launch. | Phase 4V may call `getBrowserRuntimeStatus` from Startup Splash when no explicit preview model is supplied, but must not execute runtime actions, edit backend IPC, or touch provider selection. |
| 2026-05-24 | Merge Phase 4V and start Phase 4W as a Settings run-doctor refresh. | PR #448 merged as `5bd70bd4`; Settings and Startup Doctor now both consume the dedicated read-only status bridge. GitNexus impact for the shared bridge is HIGH because it fans into Settings, Startup, and root `App`, and fresh reviewer Carver accepted a Settings-local refresh plan. | Phase 4W may make the existing Settings `run_doctor` button refresh status through the read-only bridge, but must not change backend IPC, execute runtime actions, mutate packs, or touch provider selection. |
| 2026-05-24 | Merge Phase 4W and start Phase 4X as Settings action dry-run IPC. | PR #449 merged as `f24a88b4`; Settings can now refresh read-only status on demand. Runtime-pack action buttons still need backend planner evidence before any real execution is safe. | Phase 4X may add a dedicated dry-run Tauri command and Settings rendering for execution reports, but must not perform real prepare/repair/reinstall/cleanup/rollback, emit TaskEvents, promote providers, or mutate runtime files. `main.rs` command registration is a narrow DMZ touch and must be reviewer-visible. |
| 2026-05-24 | Close Phase 4 after PR #450 and start Phase 5A as a pure CLI provider contract. | PR #450 merged as `3dbd9500`; fresh reviewer Galileo returned `REVIEW ACCEPTED`; ADR Phase 4 user-visible Settings/Doctor/task-time prompt/defer/dry-run surfaces are now in place, while real Playwright provider execution belongs to Phase 5. | Phase 5A may define readiness and JSON action-envelope contracts for `browser.playwright_cli`, but must not spawn Node/Playwright, execute browser actions, promote the provider, add IPC, or mutate runtime packs. |
| 2026-05-24 | Merge Phase 5A and start goal-mode docs hygiene before Phase 5B. | PR #451 merged as `947b3aee`; fresh reviewer Mencius returned `REVIEW ACCEPTED`. The user then asked to audit `AGENTS.md`, `BEHAVIOR.md`, and `CONTEXT.md` for goal-mode friction, especially over-strict DMZ language around `agentic_loop.rs` and `tauri_commands.rs`. | This docs-only sidecar may align behavior docs with phase-pack goal mode and remove file-name-only DMZ gates for runtime hot paths. It must not add runtime behavior or fold in Phase 5B implementation. |
| 2026-05-24 | Merge goal-mode docs hygiene and start a dry-run drift audit before Phase 5B. | PR #452 merged as `8608b694`; the user explicitly asked to audit Phase 1-5A for dry-run/code-shape drift caused by old `agentic_loop.rs` / `tauri_commands.rs` constraints. | Audit first, then continue. If the audit confirms runtime-pack real adapters are the only remaining dry-run drift, schedule a Phase 5B-preflight runtime-pack step-runner slice before the Playwright child-worker slice. |
| 2026-05-24 | Merge dry-run drift audit and start Phase 5B-preflight A. | PR #453 merged as `cd6ccc61`; the audit found no `agentic_loop.rs` / `tauri_commands.rs` design drift, but did find optimistic runtime-pack readiness and missing real runner/probe adapters. | Phase 5B-preflight A fixes strict readiness and adds a concrete local runtime-pack runner boundary before any Playwright CLI child worker. |
| 2026-05-24 | Merge Phase 5B-preflight A and start Settings live path mapping. | PR #454 merged as `6694d888`; it contained strict runtime-pack readiness defaults, a local-first managed step runner, focused runtime-pack tests, default browser-runtime regressions, and GitNexus staged detect LOW. | Phase 5B-preflight B exposes live runtime root/current pack path in Settings before Playwright child-worker execution. |
| 2026-05-24 | Merge Phase 5B-preflight B and start Phase 5B child-worker execution. | PR #455 merged as `681070db`; it contained frontend runtime root/current pack type mapping, Settings display rows, focused UI tests, default browser-runtime regressions, and GitNexus staged detect MEDIUM with no HIGH/CRITICAL. | Phase 5B proper may add a supervised short-lived Playwright CLI child-worker boundary behind the feature flag, but must not promote the provider, route tasks, add IPC, or introduce global npm/user Playwright production paths. |
| 2026-05-24 | Merge Phase 5B child-worker boundary and start Phase 5C worker script contract. | PR #456 merged as `a5141cac`; it added app-managed Node/worker path validation, stdin/stdout protocol, timeout kill, nonzero-exit handling, and LOW staged GitNexus detect. | Phase 5C may add the managed Playwright worker script and contract tests, but must not promote the provider, route tasks, add Settings/IPC, or use global npm/user-installed Playwright as a production path. |
| 2026-05-24 | Open Phase 5C worker script PR. | PR #457 contains the managed Playwright worker script, worker-side declarative action handlers, focused Rust contract tests through pack-local Node with a fake Playwright module, default browser-runtime regressions, and GitNexus staged detect LOW. | Merge if GitHub reports CLEAN; next Phase 5 slice should wire execution behind the provider/supervisor gate without bypassing policy, artifact, or runtime-pack boundaries. |
| 2026-05-24 | Merge Phase 5C worker script and start Phase 5D provider execution adapter. | PR #457 merged as `96a8b5bd`; it shipped the managed worker script, declarative worker actions, artifact-visible screenshot output, and LOW staged GitNexus detect. | Phase 5D may add a callable provider adapter around flags, runtime readiness, envelope building, child-worker execution, and structured error mapping, but must not route agent tasks or promote the provider. |
| 2026-05-24 | Open Phase 5D provider execution adapter PR. | PR #458 contains typed provider execution DTOs, the feature/runtime-gated adapter, structured worker/runner error mapping, focused adapter tests, default browser-runtime regressions, and GitNexus staged detect LOW. | Merge if GitHub reports CLEAN; next Phase 5 slice should finish CLI fixture/harness gates such as locator/coordinate fallback and risk screenshot policy evidence before Phase 6. |
| 2026-05-24 | Merge Phase 5D provider adapter and start Phase 5E fixture gates. | PR #458 merged as `78561429`; it added the callable feature/runtime-gated adapter and structured provider execution results without task routing or provider promotion. | Phase 5E is a verification slice to close ADR Phase 5 gate evidence for locator fallback, coordinate fallback, risk screenshot policy, and remaining declarative action outputs. |
| 2026-05-24 | Merge Phase 5E fixture gates and start Phase 5F action state diff. | PR #459 merged as `e3e57f72`; it covered success/failure envelopes, timeout/kill, locator and coordinate fallback, risk screenshot behavior, artifact refs, no raw script, and declarative action outputs. ADR Phase 5 still explicitly asks for action result plus DOM/state diff on stable locator clicks, type, and wait. | Phase 5F adds compact state-diff evidence to click/type/wait outputs before Phase 6. It must not route tasks, promote the provider, add IPC/UI, or leak raw page text. |
| 2026-05-24 | Merge Phase 5F and start Phase 6A identity revocation contract. | PR #460 merged as `76fea14c`; Phase 5 now covers the feature flag, app-managed pack, supervised child worker, JSON envelope, declarative actions, raw-script exclusion, timeout/kill, locator/coordinate fallback, risk screenshots, artifact refs, and compact state-diff evidence. | Phase 6 starts with revoked-visible identity metadata and resolve/load blocking before Settings connect/status UI or task drain behavior. |
| 2026-05-24 | Merge Phase 6A and start Phase 6B identity IPC. | PR #461 merged as `a5fff49e`; identity metadata can now preserve revoked status while blocking resolve/load and deleting secrets. | Phase 6B exposes safe list/revoke IPC and frontend bridge contracts without Settings UI, auth WebView, task drain, or raw secret exposure. |
| 2026-05-24 | Merge Phase 6B and start Phase 6C Settings identity status. | PR #462 merged as `e824ef07`; safe identity list/revoke IPC and frontend bridge types now exist. | Phase 6C may render identity status and user-triggered revoke in Settings, but still excludes connect/import, auth WebView, task drain, TaskEvents, and provider promotion. |
| 2026-05-24 | Merge Phase 6C and start Phase 6D identity active-task drain tracker. | PR #463 merged as `367a9361`; Settings now shows identity status and user-triggered revoke, but active-task count is still `null`. ADR Phase 6 still requires active task display, bounded revoke drain, and paused checkpoint. | Phase 6D adds the live active-task/drain boundary so revoke is no longer a display-only action for running identity-backed tasks. It must keep new logic in focused modules and avoid any dry-run lane caused by large-file fear. |
| 2026-05-24 | Open Phase 6D identity active-task drain tracker PR. | PR #464 contains the process-local identity task registry, active-task IPC summaries, bounded revoke drain deadline, safe-boundary checkpointing, thin `tauri_commands.rs` registry wiring, focused regressions, and GitNexus staged detect MEDIUM with no HIGH/CRITICAL. | Fresh reviewer should audit the behavioral runtime wiring before merge; if accepted and GitHub reports CLEAN, merge and continue the next Phase 6 slice. |
| 2026-05-24 | Merge Phase 6D and start Phase 6E Settings active-task details. | PR #464 merged as `f4f8788f`; fresh reviewer Mendel accepted the reviewer-finding fixes for revoked checkpoint resume and startup auth injection guard. Phase 6D IPC now returns real active task summaries. | Phase 6E consumes those real summaries in Settings so users can see which identity-backed tasks are active/draining before revoke decisions. Authorization WebView, reauthorize, isolated fallback, end-task UI, and payment confirmation remain separate PRs. |
| 2026-05-24 | Merge Phase 6E and start Phase 6F identity boundary actions. | PR #465 merged as `313b7e83`; Settings now renders real active identity task details, drain deadlines, and run/session identifiers. ADR Phase 6 still requires post-checkpoint recovery choices. | Phase 6F adds the typed backend resume decision contract for isolated-profile fallback, explicit reauthorize, and end-task, while keeping auth WebView, Settings recovery UI, TaskEvents, and payment confirmation separate. |
| 2026-05-24 | Merge Phase 6F and start Phase 7A MCP provider contract. | PR #466 merged as `ad088ed1`; fresh reviewer Copernicus returned `REVIEW ACCEPTED` after the P1 follow-up fixes, with only a residual real-browser resume integration-test gap. | Phase 7 starts with a pure `browser.playwright_mcp` contract slice: provider status, controlled sidecar spec, uClaw-level envelope, disabled fallback, and raw MCP exposure blocking. No MCP spawn, raw tool surface, IPC, TaskEvents, or provider promotion in this PR. |
| 2026-05-24 | Merge Phase 7A and start Phase 7B MCP runtime-pack probe. | PR #467 merged as `2b1e7f77`; the MCP provider contract exists, but the app-managed runtime pack did not yet track the pinned `@playwright/mcp` package path. | Phase 7B adds manifest/path/probe evidence for `node_modules/@playwright/mcp` while preserving existing CLI readiness when that MCP package is absent. No sidecar spawn, package installation, IPC, TaskEvents, provider promotion, or global npm fallback in this PR. |
| 2026-05-24 | Merge Phase 7B and start Phase 7C as a package pin correction before sidecar execution. | PR #468 merged as `90fe28d7`; `npm view @playwright/mcp@1.53.0` returned 404, while `npm view @playwright/mcp version` reported current stable `0.0.75` and `npm view @playwright/mcp@0.0.75 bin` reported `playwright-mcp: cli.js`. | Correct the app-managed MCP package pin before adding a supervised runner, so Phase 7D does not inherit an impossible sidecar package spec. |
| 2026-05-24 | Merge Phase 7C and start Phase 7D as the supervised MCP sidecar runner. | PR #470 merged as `5adc67a0`; fresh reviewer Turing returned `REVIEW ACCEPTED`. The existing MCP sidecar spec still had npx-style package args even though production must use app-managed pack paths. | Phase 7D starts MCP from `current_pack_dir/node/bin/node` plus `current_pack_dir/node_modules/@playwright/mcp/cli.js`, preserving the package pin as metadata and keeping raw MCP tools hidden. |
| 2026-05-24 | Merge Phase 7D and start Phase 7E as the MCP stdio action boundary. | PR #471 merged as `0d1ef4b1`; fresh reviewer Confucius returned `REVIEW ACCEPTED` after the flaky timing assertion was replaced with deterministic startup-exit stderr evidence. | Phase 7E moves beyond sidecar spawn by translating fixed uClaw actions into supervised MCP `initialize` and `tools/call` stdio JSON-RPC against an app-managed child process. |
| 2026-05-24 | Merge Phase 7E and start Phase 7F as MCP artifact/error routing. | PR #472 merged as `d21b9fa2`; fresh reviewer Boole returned `REVIEW ACCEPTED` after timeout poisoning and stable snapshot arguments were added. | Phase 7F converts sidecar success/error outputs into provider-level result DTOs carrying artifact refs, event metadata, error codes, and retryability, while still avoiding Phase 8 provider promotion. |
| 2026-05-24 | Merge Phase 7F and start Phase 7G as MCP selection policy. | PR #473 merged as `359b94e9`; fresh reviewer Locke returned `REVIEW ACCEPTED` with only non-blocking DTO/fixture follow-ups. | Phase 7G encodes the ADR rule that MCP must stay behind the CLI thin lane unless the task explicitly requires MCP-specific capability, without wiring live task routing or provider promotion. |
| 2026-05-24 | Merge Phase 7G and start Phase 8A as provider route decision. | PR #474 merged as `6d1704e0`; fresh reviewer Dalton returned `REVIEW ACCEPTED` after checking scope boundaries, ranking behavior, tests, and tracker consistency. | Phase 8A starts provider abstraction with a pure route decision contract over provider status snapshots and event intentions, while live routing remains the next Phase 8 slice. |
| 2026-05-24 | Merge Phase 8A and start Phase 8B as provider router surface. | PR #475 merged as `f8a3a2cc`; fresh reviewer Anscombe returned `REVIEW ACCEPTED`, with a non-blocking note to clarify `previous_provider_id` semantics before live routing. | Phase 8B introduces a small in-memory router surface for provider status snapshots, disabled providers, and previous-provider tracking before agent-loop/IPC wiring. |
| 2026-05-24 | Merge Phase 8B and start Phase 8C as provider scorecard contract. | PR #476 merged as `814bfb40`; fresh reviewer Socrates returned `REVIEW ACCEPTED` after the rollback-semantics blocker was fixed. | Phase 8C adds explicit harness score metadata to provider capability cards before live routing/default promotion, so provider choice remains evidence-backed. |
| 2026-05-24 | Merge Phase 8C and start Phase 8D as provider route events. | PR #477 merged as `42a764fb`; fresh reviewer Hooke returned `REVIEW ACCEPTED` after the `BrowserRuntimeTransition` deserialize regression was fixed. | Phase 8D must turn route event intents into rollout-visible TaskEvents, while keeping fixture counts as evidence metadata rather than runtime quality scores. |
| 2026-05-24 | Merge Phase 8D and start Phase 8E as live provider route signals. | PR #478 merged as `23f57438`; fresh reviewer Chandrasekhar returned `REVIEW ACCEPTED`, with a non-blocking note that future live routing should use one timestamp per route decision batch. | Phase 8E wires provider route decisions into `BrowserAgentLoop` before local action execution. It may emit rollout-visible route signals when rollout is enabled, but must not switch execution to CLI/MCP or promote providers. |
| 2026-05-24 | Merge Phase 8E and start Phase 8F as provider execution boundary. | PR #479 merged as `19b99593`; fresh reviewer Russell returned `REVIEW ACCEPTED` after checking live route placement, local guard, signal batching, and low GitNexus compare risk. | Phase 8F should keep `agent_loop.rs` thin by extracting the route/guard/local action execution into a focused provider execution module, without enabling CLI/MCP execution or provider promotion. |
| 2026-05-24 | Open Phase 8F provider execution boundary PR. | PR #480 contains the focused `BrowserProviderActionExecutor`, live agent-loop delegation, non-local fail-closed guard preservation, focused provider execution tests, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit the real action-path refactor before merge; if accepted and GitHub reports CLEAN, merge and continue to the next Phase 8 slice. |
| 2026-05-24 | Merge Phase 8F and start Phase 8G as CLI/MCP provider candidate route inputs. | PR #480 merged as `49b71dd0`; fresh reviewer Ampere returned `REVIEW ACCEPTED` and GitHub reported CLEAN. | Phase 8G may add feature-flagged CLI/MCP candidate status inputs to the live provider executor, but must keep safe defaults local-only and block non-local execution until explicit execution wiring lands. |
| 2026-05-24 | Open Phase 8G CLI/MCP provider candidate route-input PR. | PR #481 contains route options for feature flags, optional runtime-pack readiness evidence, disabled provider IDs, focused provider execution tests, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit the candidate input boundary and safe-default behavior before merge; if accepted and GitHub reports CLEAN, merge and continue to the next Phase 8 slice. |
| 2026-05-24 | Merge Phase 8G and start Phase 8H as CLI selected-route execution. | PR #481 merged as `e527ec45`; fresh reviewer Herschel returned `REVIEW ACCEPTED` and GitHub reported CLEAN. | Phase 8H may execute explicitly selected Playwright CLI routes through the existing app-managed worker adapter, but must keep safe defaults local and avoid provider promotion, MCP execution, UI, IPC, DB, or raw scripts. |
| 2026-05-24 | Open Phase 8H CLI selected-route execution PR. | PR #482 contains selected CLI route execution through the existing app-managed worker adapter, BrowserActionResult normalization, unsupported-action blocking, focused provider execution tests, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit selected-provider execution semantics and safe-default behavior before merge; if accepted and GitHub reports CLEAN, merge and continue to the next Phase 8 slice. |
| 2026-05-24 | Merge Phase 8H and start Phase 8I as provider parity matrix harness. | PR #482 merged as `49c274de`; fresh reviewer Lovelace returned `REVIEW ACCEPTED` and reran `browser::provider_execution`. | Phase 8I should add model-free parity matrix evidence that the same harness case can route across local Chromium, Playwright CLI, Playwright MCP, and mock hosted providers, plus fallback artifact visibility, without promotion or real hosted/MCP execution. |
| 2026-05-24 | Open Phase 8I provider parity matrix harness PR. | PR #483 contains the model-free provider parity matrix module, shared navigate/click forced-route cases, fallback artifact-visibility evidence, attachable harness artifact output, focused harness/provider/runtime regressions, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit the harness evidence and no-promotion boundary before merge; if accepted and GitHub reports CLEAN, merge and continue to the next Phase 8 slice. |
| 2026-05-24 | Merge Phase 8I and start Phase 8J as provider default policy gate. | PR #483 merged as `5a664789`; fresh reviewer Schrodinger returned `REVIEW ACCEPTED` and GitHub reported CLEAN before merge. | Phase 8J owns only a pure, reversible default-provider decision contract. It must not mutate settings, promote CLI/MCP/hosted providers, change route ranking, add UI/IPC/DB, or execute providers. |
| 2026-05-24 | Open Phase 8J provider default policy gate PR. | PR #484 contains the pure default-provider policy contract, reversible promotion/fallback decisions, rollback provider metadata, focused provider/runtime regressions, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit the no-promotion boundary and fallback policy gates before merge; if accepted and GitHub reports CLEAN, merge and continue to the next ADR phase. |
| 2026-05-24 | Merge Phase 8J and start Phase 9A as recipe candidate contract. | PR #484 merged as `cab8f161`; fresh reviewer Faraday returned `REVIEW ACCEPTED` after two fallback/promotion blocker fixes. | Phase 9A starts with a pure recipe candidate/redaction/fingerprint/provider-version contract. It must not replay recipes, write domain skills, persist locator caches, add UI/IPC/DB, or mutate production behavior. |
| 2026-05-24 | Open Phase 9A recipe candidate contract PR. | PR #486 contains the pure recipe candidate contract, redaction rejection, promotion-readiness and rollback metadata, fingerprint/provider-version replay gates, focused browser regressions, and GitNexus staged detect LOW with 0 affected processes. | Fresh reviewer should audit the pure-contract boundary before merge; if accepted and GitHub reports CLEAN, merge and continue to the next Phase 9 slice. |
| 2026-05-24 | Rebase Phase 9A after unrelated NexusMemory main advance. | PR #485 merged `prep/nexus-memory` as `e5a98220` while Phase 9A was in flight. | Phase 9A branch rebased cleanly onto `e5a98220` so PR #486 stays based on current `origin/main`; Browser Runtime scope remains unchanged. |
| 2026-05-24 | Merge Phase 9A and start Phase 9B as recipe normalization intake. | PR #486 merged as `5228d0ab` after fresh reviewer Mill returned `REVIEW ACCEPTED`; Phase 9A final commit was `fb2276a9 feat(browser): add recipe candidate contract`. | Phase 9B adds only a pure intake builder from action observations to recipe candidates. It must not replay recipes, persist locator caches, write domain skills, add UI/IPC/DB, or change provider behavior. |
| 2026-05-25 | Merge Phase 9B and start Phase 9C as a locator cache contract. | PR #487 merged as `930530cb` after fresh reviewer Hume returned `REVIEW ACCEPTED`; Phase 9B final commit was `884dfac2 feat(browser): normalize recipe candidates`. | Phase 9C adds only pure locator-cache eligibility and reuse decisions. It must not persist caches, replay actions, write domain skills, add UI/IPC/DB, or change provider behavior. |
| 2026-05-25 | Merge Phase 9C and start Phase 9D as a domain-skill candidate gate. | PR #488 merged as `d96f432d` after fresh reviewer Zeno returned `REVIEW ACCEPTED`; Phase 9C final commit was `52ada9a7 feat(browser): add recipe locator cache contract`. | Phase 9D adds only a pure eligibility gate for domain-skill candidates. It must not write domain-skill files, replay actions, persist locators, add UI/IPC/DB, or change provider behavior. |
| 2026-05-25 | Merge Phase 9D and start Phase 9E as a recipe/domain-skill harness matrix. | PR #489 merged as `769e0d1e` after reviewer Bohr blocked a whitespace-only evidence bug, the branch was fixed, and fresh reviewer Euclid returned `REVIEW ACCEPTED`; Phase 9D final commit was `fe3418b2 feat(browser): gate domain skill candidates`. | Phase 9E turns the ADR Phase 9 gate into a pure matrix report. It must not execute replay, persist locators, write domain skills, add UI/IPC/DB, or change provider behavior. |
| 2026-05-25 | Merge Phase 9E and start Phase 10A as a hosted-provider capability contract. | PR #490 merged as `c16a6720` after reviewer Arendt blocked an artifact-preservation bug, the branch was fixed, and fresh reviewer Cicero returned `REVIEW ACCEPTED`; Phase 9E final commit was `d00fd124 feat(browser): add recipe harness matrix`. | Phase 10A starts with a pure hosted-provider contract behind `BrowserProvider`/capability-card policy. It must not add a real hosted SDK, network path, credentials, UI, IPC, DB migration, provider promotion, or live execution. |
| 2026-05-24 | Merge Phase 10A and start Phase 10B as a hosted-provider harness matrix. | PR #491 merged as `568a0af0` after reviewer Ramanujan blocked a fail-closed status bug, the branch was fixed, and fresh reviewer Avicenna returned `REVIEW ACCEPTED`; Phase 10A final commit was `55b361d6 feat(browser): add hosted provider policy contract`. | Phase 10B owns only pure harness evidence for the ADR Phase 10 gate. It must not add hosted SDKs, credentials, real network execution, provider promotion, UI, IPC, DB migration, `agentic_loop.rs`, or `tauri_commands.rs` changes. |
| 2026-05-24 | Merge Phase 10B and start Phase 6H as identity authorization backfill. | PR #492 merged as `58e2d58b`; Phase 10B final commit was `01d96e7d feat(browser): add hosted provider harness matrix`. Tracker review found Phase 6 still had an auth WebView / payment-confirmation residual note, while code already has automation-specific browser/WebView login capture in `tauri_commands.rs`. | Phase 6H adds a generic browser identity authorization completion contract and keeps existing automation commands as thin compatibility shims. This is an explicit dry-run/special-DMZ audit fix: do not leave real identity capture trapped in a spec-only lane because `tauri_commands.rs` is large. |
| 2026-05-25 | Merge Phase 6H and start Phase 6I payment confirmation harness. | PR #493 merged as `d248e4f5`; fresh reviewer Hume returned `REVIEW ACCEPTED` with no blocking notes. The branch added generic identity authorization IPC/bridge and thin automation login shims. | Phase 6I owns only harness evidence for the ADR Phase 6 payment-confirmation gate: payment boundary plus `ask_user_response`. No payment UI, checkout execution, provider promotion, DB migration, hosted provider, or task-loop rewrite. |
| 2026-05-25 | Merge Phase 6I and start final completion audit. | PR #494 merged as `f6447a71`; final commit was `aadd581b test(browser): cover payment confirmation harness`. Reviewer Lovelace blocked the first revision on a missing `/checkout` fixture route; the branch added the route and server test, then fresh reviewer Mencius accepted. | All ADR phases now have merged implementation or harness evidence through `origin/main`; the remaining work is docs-only tracker closeout and requirement-by-requirement completion verification. |
| 2026-05-25 | Merge completion audit and start final tracker sync. | PR #495 merged as `7e94b5ed`; final commit was `020a8ffd docs(browser): close runtime supervisor completion audit`. Fresh reviewer Banach returned `REVIEW ACCEPTED` after two stale-state reviewer findings were fixed. Unrelated PR #496 then advanced `origin/main` to `17ffe1c6`. | Final tracker sync updates this file from the in-flight completion-audit state to merged-main truth, then the goal can be audited from `origin/main`. |
| 2026-05-25 | Close Browser Runtime Supervisor / Playwright Provider Strategy as implemented and verified. | PR #497 merged as `1db7d988`; unrelated PR #498 then advanced `origin/main` to `52ba4833`; final focused checks on that current main base passed: `browser::runtime_pack` 42 tests, `browser::runtime` 59 tests, `browser::provider::tests` 16 tests, `harness::adapters::browser` 19 tests, and docs diff checks clean. | No further Browser Runtime phase is planned. Future browser runtime work requires a new ADR/spec or a new tracker row outside this completed goal. |

---

## Current Branch Hygiene

| Check | Current Value |
|---|---|
| Primary worktree | `/Users/ryanliu/Documents/uclaw` |
| Current phase worktree | `/Users/ryanliu/Documents/uclaw-worktrees/startup-splash-if2ai-port` |
| Current phase branch | `codex/startup-splash-if2ai-port` |
| Current local base | `7c14c2d1 Merge pull request #501 from novolei/codex/startup-splash-min-duration` |
| Browser ADR commit on phase branch | Included in merged `origin/main` history. |
| Phase 0 implementation commit | Merged through `origin/main` history as `a24cbc08 feat(browser): add runtime supervisor phase0 contracts`. |
| Phase 1 implementation commit | Merged through `origin/main` history as `bcf823f8 feat(browser): add runtime supervisor phase1 shell`. |
| Phase 2 implementation commit | Merged through `origin/main` history as `96752fe6 feat(browser): add runtime pack manager shell`. |
| Phase 2B implementation commit | Merged through `origin/main` history as `6915f184 feat(browser): plan runtime pack operations`. |
| Phase 2C implementation commit | Merged through PR #416 as `feat(browser): add runtime pack dry-run executor`. |
| Phase 2D implementation commit | Merged through PR #417 as `feat(browser): probe runtime pack filesystem`. |
| Phase 2E implementation commit | Merged through PR #418 as `feat(browser): add runtime pack status report`. |
| Phase 2F implementation commit | Merged through PR #419 as `9d02cb33 feat(browser): add runtime pack executor boundary`; merge commit `45463455`. |
| Phase 3A implementation commit | Merged through PR #420 as `267f2c6f feat(browser): add startup shell substrate`; merge commit `2c380373`. |
| Phase 3B implementation commit | Merged through PR #421 as `8112b362 feat(browser): map runtime pack status into startup doctor`; merge commit `7efe4fee`. |
| Phase 3C implementation commit | Merged through PR #422 as `741deb58 feat(browser): add startup splash preview harness`; merge commit `0eb46780`. |
| Phase 3D implementation commit | Not committed. Stopped after staged GitNexus detect reported HIGH risk for `App` touching 9 top-level app processes. |
| Phase 3E implementation commit | Merged through PR #423 as `52035cf4 feat(browser): add startup recovery surfaces`; merge commit `f2dabbe3`. |
| Phase 3F implementation commit | Merged through PR #424 as `3d4121be docs(browser): record startup route review gate`; merge commit `3e9e4817`. |
| Phase 3G implementation commit | Merged through PR #425 as `8a1bf76b docs(browser): add root app route review pack`; merge commit `c5ce25c1`. |
| Phase 3H implementation commit | Merged through PR #426 as `35d7e39c feat(browser): route app startup through splash`; merge commit `13133bb1`. |
| Phase 4A implementation commit | Merged through PR #427 as `374fb39d feat(browser): add runtime settings surface`; merge commit `5e0f18fb`. |
| Phase 4B implementation commit | Merged through PR #428 as `9aca960d feat(browser): add runtime settings action intents`; merge commit `d3f9f995`. |
| Phase 4C implementation commit | Merged through PR #429 as `985af8e3 feat(browser): add auto-prepare settings intent`; merge commit `50b5ab8f`. |
| Phase 4D implementation commit | Merged through PR #430 as `ab359858 feat(browser): add task-time runtime prompt model`; merge commit `7d4f70e0`. |
| Phase 4E implementation commit | Merged through PR #431 as `b27410dc feat(browser): add task-time runtime prompt UI`; merge commit `ab59f9aa`. |
| Phase 4F implementation commit | Merged through PR #432 as `e58516fc feat(browser): add settings palette runtime deep link`; merge commit `00ce02ed`. |
| Phase 4G implementation commit | Merged through PR #433 as `a3bcb459 feat(browser): link startup doctor to runtime settings`; merge commit `5dd0745c`. |
| Phase 4H implementation commit | Merged through PR #434 as `d02ae409 feat(browser): link task-time prompt to runtime settings`; merge commit `bf6a4693`. |
| Phase 4I implementation commit | Merged through PR #435 as `007c89ea feat(browser): link recovery errors to runtime settings`; merge commit `ab65fab8`. |
| Phase 4J implementation commit | Merged through PR #436 as `24ae4b22 feat(browser): add paused-waiting runtime task status`; merge commit `1f8739ec`. |
| Phase 4K implementation commit | Merged through PR #437 as `7b667f4a feat(browser): show paused-waiting runtime task status`; merge commit `5bd56ba1`. |
| Phase 4L implementation commit | Merged through PR #438 as `f0fa0b39 feat(browser): add task runtime pause gate`; merge commit `a566decf`. |
| Phase 4M implementation commit | Merged through PR #439 as `1c844a19 feat(browser): bridge task-time runtime decision`; merge commit `a0dd62e5`. |
| Phase 4N implementation commit | Merged through PR #440 as `d5068b30 feat(browser): add task-time tool call patch boundary`; merge commit `08a2b65f`. |
| Phase 4O implementation commit | Merged through PR #441 as `d4026f18 docs(browser): add prompt dispatch review pack`; merge commit `4d67f487`. |
| Phase 4P implementation commit | Merged through PR #442 as `2db901d8 feat(browser): add task-time dispatch patch boundary`; merge commit `1fd68675`. |
| Phase 4Q implementation commit | Merged through PR #443 as `11e6032f feat(browser): model task-time dispatch effects`; merge commit `1db302be`. |
| Phase 4R implementation commit | Merged through PR #444 as `60925a1e docs(browser): add settings ipc review pack`; merge commit `c4a31567`. |
| Phase 4S implementation commit | Merged through PR #445 as `a66ec2ce feat(browser): add readonly runtime status ipc`; merge commit `88f552f3`. |
| Phase 4T implementation commit | Merged through PR #446 as `ac9537b9 fix(browser): clarify runtime settings status rows`; merge commit `aa6838d6`. |
| Phase 4U implementation commit | Merged through PR #447 as `a09bd1b6 feat(browser): read runtime status in settings`; merge commit `ffc7b811`. |
| Phase 4V implementation commit | Merged through PR #448 as `befe2656 feat(browser): read runtime status in startup doctor`; merge commit `5bd70bd4`. |
| Phase 4W implementation commit | Merged through PR #449 as `166504ad feat(browser): refresh runtime doctor from settings`; merge commit `f24a88b4`. |
| Phase 4X implementation commit | Merged through PR #450 as `069dafd4 feat(browser): dry-run runtime actions from settings`; merge commit `3dbd9500`. |
| Phase 5A implementation commit | Merged through PR #451 as `b420bff5 feat(browser): add playwright cli provider contract`; merge commit `947b3aee`. |
| Goal-mode docs hygiene commit | Merged through PR #452 as `c1ea0f34 docs(goal): align browser runtime goal-mode rules`; merge commit `8608b694`. |
| Dry-run drift audit commit | Merged through PR #453 as `4542eff9 docs(browser): audit dry-run drift before phase5b`; merge commit `cd6ccc61`. |
| Phase 5B-preflight A implementation commit | Merged through PR #454 as `b1ba9f54 feat(browser): add runtime pack local runner`; merge commit `6694d888`. |
| Phase 5B-preflight B implementation commit | Merged through PR #455 as `67454c4a feat(browser): show runtime pack paths in settings`; merge commit `681070db`. |
| Phase 5B child-worker implementation commit | Merged through PR #456 as `cba085ec feat(browser): run playwright cli child worker`; merge commit `a5141cac`. |
| Phase 5C worker-script implementation commit | Merged through PR #457 as `6cdc2ea1 feat(browser): add playwright cli worker script`; merge commit `96a8b5bd`. |
| Phase 5D provider-adapter implementation commit | Merged through PR #458 as `48e4b7a3 feat(browser): add playwright cli provider adapter`; merge commit `78561429`. |
| Phase 5E fixture-gates implementation commit | Merged through PR #459 as `a8345e97 test(browser): cover playwright cli fixture gates`; merge commit `e3e57f72`. |
| Phase 5F action-state-diff implementation commit | Merged through PR #460 as `7c045fdd feat(browser): add playwright cli action state diffs`; merge commit `76fea14c`. |
| Phase 6A identity-revocation implementation commit | Merged through PR #461 as `6214f9fb feat(browser): add identity revocation contract`; merge commit `a5fff49e`. |
| Phase 6B identity-IPC implementation commit | Merged through PR #462 as `d8089416 feat(browser): expose identity ipc contract`; merge commit `e824ef07`. |
| Phase 6C Settings identity-status implementation commit | Merged through PR #463 as `374a2200 feat(browser): show identity status in settings`; merge commit `367a9361`. |
| Phase 6D identity-drain-tracker implementation commit | Merged through PR #464 as `d62505bb feat(browser): track identity task drain`; merge commit `f4f8788f`. |
| Phase 6E Settings active-task details implementation commit | Merged through PR #465 as `01363646 feat(browser): show identity active tasks in settings`; merge commit `313b7e83`. |
| Phase 6F identity-boundary-actions implementation commit | Merged through PR #466 as `3b729893 feat(browser): add identity boundary resume decisions`; merge commit `ad088ed1`. |
| Phase 7A MCP provider contract implementation commit | Merged through PR #467 as `bec34855 feat(browser): add playwright mcp provider contract`; merge commit `2b1e7f77`. |
| Phase 7B MCP runtime-pack probe implementation commit | Merged through PR #468 as `7ea2453f feat(browser): track playwright mcp runtime pack`; merge commit `90fe28d7`. |
| Phase 7C MCP package pin correction implementation commit | Merged through PR #470 as `eb5e33b0 fix(browser): correct playwright mcp package pin`; merge commit `5adc67a0`. |
| Phase 7D MCP sidecar runner implementation commit | Merged through PR #471 as `18b97544 feat(browser): start playwright mcp sidecar`; merge commit `0d1ef4b1`. |
| Phase 7E MCP stdio action boundary implementation commit | Merged through PR #472 as `111eada1 feat(browser): add mcp stdio action boundary`; merge commit `d21b9fa2`. |
| Phase 7F MCP artifact/error routing implementation commit | Merged through PR #473 as `1d2512bf feat(browser): route mcp artifact errors`; merge commit `359b94e9`. |
| Phase 7G MCP selection policy implementation commit | Merged through PR #474 as `9388c666 feat(browser): add mcp selection policy`; merge commit `6d1704e0`. |
| Phase 8A provider route decision implementation commit | Merged through PR #475 as `5ca12d86 feat(browser): add provider route decision`; merge commit `f8a3a2cc`. |
| Phase 8B provider router surface implementation commit | Merged through PR #476 as `a391b496 feat(browser): add provider router surface`; merge commit `814bfb40`. |
| Phase 8C provider scorecard contract implementation commit | Merged through PR #477 as `1522a18e feat(browser): add provider scorecard metadata`; merge commit `42a764fb`. |
| Phase 8D provider route events implementation commit | Merged through PR #478 as `f2983b77 feat(browser): emit provider route signals`; merge commit `23f57438`. |
| Phase 8E live route signals implementation commit | Merged through PR #479 as `6550fa6c feat(browser): emit live provider route signals`; merge commit `19b99593`. |
| Phase 8F provider execution boundary implementation commit | Merged through PR #480 as `74ddeb03 feat(browser): extract provider action execution boundary`; merge commit `49b71dd0`. |
| Phase 8G CLI/MCP provider candidate route-input implementation commit | Merged through PR #481 as `a52482a4 feat(browser): add provider route candidate inputs`; merge commit `e527ec45`. |
| Phase 8H CLI selected-route execution implementation commit | Merged through PR #482 as `c2f5f388 feat(browser): execute selected cli provider routes`; merge commit `49c274de`. |
| Phase 8I provider parity matrix harness implementation commit | Merged through PR #483 as `fd7dc776 feat(browser): add provider parity matrix harness`; merge commit `5a664789`. |
| Phase 8J provider default policy gate implementation commit | Merged through PR #484 as `d7673862 feat(browser): add provider default policy gate`; merge commit `cab8f161`. |
| Phase 9A recipe candidate contract implementation commit | Merged through PR #486 as `fb2276a9 feat(browser): add recipe candidate contract`; merge commit `5228d0ab`. |
| Phase 9B recipe normalization intake implementation commit | Merged through PR #487 as `884dfac2 feat(browser): normalize recipe candidates`; merge commit `930530cb`. |
| Phase 9C locator cache contract implementation commit | Merged through PR #488 as `52ada9a7 feat(browser): add recipe locator cache contract`; merge commit `d96f432d`. |
| Phase 9D domain-skill candidate gate implementation commit | Merged through PR #489 as `fe3418b2 feat(browser): gate domain skill candidates`; merge commit `769e0d1e`. |
| Phase 9E recipe/domain-skill harness matrix implementation commit | Merged through PR #490 as `d00fd124 feat(browser): add recipe harness matrix`; merge commit `c16a6720`. |
| Phase 10A hosted-provider capability contract implementation commit | Merged through PR #491 as `55b361d6 feat(browser): add hosted provider policy contract`; merge commit `568a0af0`. |
| Phase 10B hosted-provider harness matrix implementation commit | Merged through PR #492 as `01d96e7d feat(browser): add hosted provider harness matrix`; merge commit `58e2d58b`. |
| Phase 6H identity-authorization implementation commit | Merged through PR #493 as `7a0f9254 feat(browser): add identity authorization contract`; merge commit `d248e4f5`. |
| Phase 6I payment-confirmation harness implementation commit | Merged through PR #494 as `aadd581b test(browser): cover payment confirmation harness`; merge commit `f6447a71`. |
| Completion audit commit | Merged through PR #495 as `020a8ffd docs(browser): close runtime supervisor completion audit`; merge commit `7e94b5ed`. |
| Unrelated post-audit main advance | PR #496 merged as `17ffe1c6`; it is outside Browser Runtime scope but is included in the current `origin/main` base. |
| Final tracker sync commit | Merged through PR #497 as `8728adbc docs(browser): sync final runtime tracker state`; merge commit `1db7d988`. |
| Unrelated post-final-sync main advance | PR #498 merged as `52ba4833`; it is outside Browser Runtime scope but is included in the current `origin/main` base for final verification. |
| Verified complete closeout commit | Merged through PR #499 as `docs(browser): mark runtime supervisor verified complete`; later post-completion UX work is outside ADR phase implementation. |
| Startup minimum-visibility commit | Merged through PR #501 as `fix(ui): keep startup splash perceptible`; merge commit `7c14c2d1`. |
| Startup If2Ai visual-port commit | In progress on `codex/startup-splash-if2ai-port`; plan `docs/superpowers/plans/2026-05-25-startup-splash-if2ai-port.md`. |
| Known pre-existing tracked changes | None in the startup visual-port worktree at start. Primary worktree remains separate with unrelated tracked and untracked user changes. |
| Linked ignored runtime resources | `ui/node_modules` linked from the primary worktree for focused frontend verification only. |
| Nested repo caveat | `/Users/ryanliu/Documents/uclaw/ulooi` is a separate git root; do not mix status or commits. |

## Phase 1 Entry Criteria

Phase 1 can start because:

- Phase 0 committed the browser runtime contracts and provider cards;
- the Phase 1 worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor`;
- the branch starts from `84743093 feat(browser): add runtime supervisor phase0 contracts`;
- this slice avoids hot-path action rewiring and focuses on a tested supervisor
  shell around local chromiumoxide state.

## Phase 1 Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase1-supervisor.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor`
- Branch:
  `codex/browser-runtime-phase1-supervisor`
- Scope:
  local Chromium supervisor shell, deadline profile, doctor classification,
  artifact-pack metadata, projection builder, and context-manager session
  snapshot.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert `runtime_supervisor.rs`, `runtime_supervisor_tests.rs`,
  `browser/mod.rs` exports, this status file, and the Phase 1 plan file.

### Phase 1 Impact Notes

- Existing browser execution symbols are intentionally avoided in this first
  shell slice: `BrowserActionRegistry`, `BrowserAgentLoop`, `BrowserContext`,
  and `tauri_commands.rs` are not edited.
- `BrowserContextManager` is observed through existing public methods only; its
  implementation is not changed.
- `browser/mod.rs` receives additive module exports only.

### Phase 1 Verification Notes

- GitNexus impact for existing `BrowserService` in `src-tauri/src/browser/mod.rs`
  reported LOW risk, 0 direct callers, and 0 affected processes before adding
  module exports.
- Focused supervisor verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_supervisor`
  returned `7 passed; 0 failed; 2573 filtered out`.
- Phase 0 contract regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  returned `5 passed; 0 failed; 2575 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider`
  returned `6 passed; 0 failed; 2574 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_supervisor.rs src-tauri/src/browser/runtime_supervisor_tests.rs`
  and `git diff --check -- <changed-files>`.
- GitNexus staged detect after refreshing the Phase 1 worktree index reported
  `risk_level: low`, `changed_files: 5`, and `affected_processes: []`.
- After the branch was merged to the local primary `main`, `origin/main` was
  also confirmed at `81d9b9dc`; effective post-merge verification used the
  correct filters:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `12 passed; 0 failed; 2568 filtered out`, and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2574 filtered out`.

## Phase 2 Entry Criteria

Phase 2 can start because:

- Phase 0 and Phase 1 are merged into `main` and `origin/main`;
- the Phase 2 worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack`;
- the branch starts from `81d9b9dc`, the current `origin/main`;
- this first slice avoids network download, Playwright worker startup, Tauri
  commands, DB migrations, and Settings UI.

Recommended Phase 2 first tests:

- runtime manifest defaults and semver-like version fields;
- path layout and `PLAYWRIGHT_BROWSERS_PATH` environment derivation;
- doctor classifications for missing manifest, missing Node, missing
  Playwright package, missing browser binary, corrupt cache, version mismatch,
  and ready state;
- update policy classification for security, ordinary, idle, active task,
  rollback, and offline states;
- cleanup/rollback action planning without deleting files in tests.

## Phase 2 Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase2-runtime-pack.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack`
- Branch:
  `codex/browser-runtime-phase2-runtime-pack`
- Scope:
  app-managed Playwright runtime-pack manifest/status/doctor shell, local path
  policy, install/update/remediation action planning, and tests.
- Implementation:
  `runtime_pack.rs` defines runtime-pack manifest metadata, uClaw-managed path
  policy including `PLAYWRIGHT_BROWSERS_PATH`, doctor issue/status
  classification, remediation actions, and update decisions.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2 runtime-pack module, tests, browser module exports, this
  status file, and the Phase 2 plan file.

### Phase 2 Impact Notes

- Existing browser execution symbols are intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- `browser/mod.rs` receives additive module exports only.
- GitNexus impact for existing `BrowserService` in `src-tauri/src/browser/mod.rs`
  reported LOW risk, 0 direct callers, and 0 affected processes before adding
  module exports.
- The first Phase 2 slice does not download, install, repair, cleanup, roll
  back, spawn Node, run Playwright, start MCP, write settings, or write DB
  migrations.

### Phase 2 Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Initial Rust focused verification before linking `gbrain-source` failed in
  the Tauri build script with `resource path 'gbrain-source' doesn't exist`;
  this was a worktree dependency issue, not a source failure.
- A post-format focused test briefly failed with `No space left on device` while
  writing Cargo incremental cache. Generated `target/` directories for the
  Phase 0, Phase 1, and Phase 2 browser-runtime worktrees were cleaned, freeing
  local disk space without touching source files.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `10 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `22 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2584 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check -- <changed-files>` returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 5`, and
  `affected_processes: []`.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 5`, and
  `affected_processes: []`.
- GitNexus staged detect after refreshing the Phase 2 worktree index reported
  `risk_level: low`, `changed_files: 5`, and `affected_processes: []`.
- Phase 2 runtime-pack shell committed on
  `codex/browser-runtime-phase2-runtime-pack` as current `HEAD`:
  `feat(browser): add runtime pack manager shell`.
- Phase 2 runtime-pack shell was merged through PR #414 as
  `49c43241 Merge pull request #414 from novolei/codex/browser-runtime-phase2-runtime-pack`.

## Phase 2B Entry Criteria

Phase 2B can start because:

- PR #414 merged the Phase 2 manifest/status/doctor shell into `main` and
  `origin/main`;
- the Phase 2B worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2b-install-plan`;
- the branch starts from `49c43241`, the current `origin/main`;
- this slice still avoids network download, archive extraction, filesystem
  deletion, Playwright worker startup, Tauri commands, DB migrations, and
  Settings/Splash UI.

Recommended Phase 2B tests:

- prepare plan includes download, sha256 verification, staging install, doctor,
  promotion, rollback retention, and `PLAYWRIGHT_BROWSERS_PATH`;
- metered/cellular/restricted or large downloads require lightweight
  confirmation until explicitly confirmed;
- offline/captive network and startup auto-prepare disabled defer without
  doing network work;
- cleanup and rollback defer while active browser tasks exist;
- rollback blocks when no previous working pack is available;
- keep-current produces a ready no-op plan.

## Phase 2B Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase2b-install-plan.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2b-install-plan`
- Branch:
  `codex/browser-runtime-phase2b-install-plan`
- Scope:
  no-side-effect runtime-pack operation planner for prepare, repair, reinstall,
  cleanup, rollback, keep-current, network confirmation, active-task deferral,
  rollback availability, environment variables, and intended TaskEvent names.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2B planner DTOs/functions/tests, browser module exports, this
  status file update, and the Phase 2B plan file.

### Phase 2B Impact Notes

- GitNexus impact before editing reported:
  `diagnose_runtime_pack` MEDIUM risk with 6 direct test callers and 0 affected
  execution flows; `decide_runtime_pack_update` LOW risk with 2 direct test
  callers and 0 affected execution flows; `BrowserRuntimePackAction` LOW risk
  with 0 direct callers; `BrowserService` export surface LOW risk with 0
  affected flows.
- Existing browser execution symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- The Phase 2B slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, write settings, or write DB migrations.

### Phase 2B Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `17 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `29 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2591 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check -- <changed-files>` returned no output.
- Phase 2B operation planner was merged through PR #415 as
  `625193fd Merge pull request #415 from novolei/codex/browser-runtime-phase2b-install-plan`.

## Phase 2C Entry Criteria

Phase 2C can start because:

- PR #415 merged the Phase 2B operation planner into `main` and `origin/main`;
- the Phase 2C worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2c-executor`;
- the branch starts from `625193fd`, the current `origin/main`;
- this slice still avoids real network download, archive extraction, filesystem
  deletion, Playwright worker startup, Tauri commands, DB migrations, and
  Settings/Splash UI.

Recommended Phase 2C tests:

- planned prepare dry-run reports success, step reports, event names, artifact
  id, network/destructive flags, and current-pack retention intent;
- confirmation-required, deferred, and blocked plans do not execute steps;
- keep-current ready plan reports no-op success;
- cleanup and rollback plans surface destructive flags after confirmation and
  no active tasks.

## Phase 2C Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase2c-dry-run-executor.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2c-executor`
- Branch:
  `codex/browser-runtime-phase2c-executor`
- Scope:
  deterministic dry-run executor and execution report DTOs for runtime-pack
  operation plans. The executor consumes plans, honors policy statuses, returns
  per-step dry-run reports, artifact ids, event names, current-pack retention,
  and network/destructive/confirmation flags.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2C executor DTOs/functions/tests, browser module exports,
  this status file update, and the Phase 2C plan file.

### Phase 2C Impact Notes

- GitNexus impact before editing reported:
  `plan_runtime_pack_operation` MEDIUM risk with 7 direct test callers and 0
  affected execution flows; `BrowserRuntimePackOperationPlan` LOW risk with the
  planner as its only direct caller and 0 affected execution flows;
  `BrowserService` export surface LOW risk with 0 affected flows.
- Existing browser execution symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- The Phase 2C slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, write settings, or write DB migrations.

### Phase 2C Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `22 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `34 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2596 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check -- <changed-files>` returned no output.

## Phase 2D Entry Criteria

Phase 2D can start because:

- PR #416 merged the Phase 2C dry-run executor into `main` and `origin/main`;
- the Phase 2D worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2d-filesystem-probe`;
- the branch starts from `e78a3dae`, the current `origin/main`;
- this slice still avoids real network download, archive extraction, filesystem
  deletion, Playwright worker startup, Tauri commands, DB migrations, and
  Settings/Splash UI.

Recommended Phase 2D tests:

- manifest loader reports missing, loaded, and invalid JSON outcomes;
- filesystem probe maps a complete local pack to an existing ready probe;
- version mismatch, invalid manifest, missing worker script, offline state, and
  active task count flow into the existing doctor-compatible probe;
- module exports include the new read-only probe DTOs/functions.

## Phase 2D Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase2d-filesystem-probe.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2d-filesystem-probe`
- Branch:
  `codex/browser-runtime-phase2d-filesystem-probe`
- Scope:
  read-only runtime-pack manifest loading and filesystem snapshot/probe DTOs.
  The probe checks current pack paths, previous pack availability, manifest
  validity, version alignment, worker script presence, and dynamic offline /
  active-task signals, then returns an existing `BrowserRuntimePackProbe`.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2D probe DTOs/functions/tests, browser module exports, this
  status file update, and the Phase 2D plan file.

### Phase 2D Impact Notes

- GitNexus impact before editing reported:
  `BrowserRuntimePackProbe` MEDIUM risk with 5 direct test callers and 0
  affected execution flows; `BrowserRuntimePackManifest` LOW risk with test-only
  callers and 0 affected execution flows; `BrowserRuntimePackPaths` LOW risk
  with 0 affected flows; `BrowserService` export surface LOW risk with 0
  affected flows.
- Existing browser execution symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- The Phase 2D slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, write settings, or write DB migrations.

### Phase 2D Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `25 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `37 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2599 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check -- <changed-files>` returned no output.

## Phase 2E Entry Criteria

Phase 2E can start because:

- PR #417 merged the Phase 2D filesystem probe into `main` and `origin/main`;
- the Phase 2E worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2e-status-report`;
- the branch starts from `d6d9a559`, the current `origin/main`;
- this slice still avoids real network download, archive extraction, filesystem
  deletion, Playwright worker startup, Tauri commands, DB migrations, and
  Settings/Splash UI.

Recommended Phase 2E tests:

- ready local pack composes filesystem probe, doctor, keep-current plan, and
  event names;
- missing runtime while offline returns deferred preparation without download
  steps;
- task-time metered preparation returns confirmation-required plan state;
- module exports include the new read-only status report DTOs/function.

## Phase 2E Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase2e-status-report.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2e-status-report`
- Branch:
  `codex/browser-runtime-phase2e-status-report`
- Scope:
  read-only runtime-pack status report aggregator that composes filesystem
  evidence, doctor outcome, primary remediation action, operation plan, and
  event names for future Startup Doctor / Settings consumers.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2E status DTOs/function/tests, browser module exports, this
  status file update, and the Phase 2E plan file.

### Phase 2E Impact Notes

- GitNexus impact before editing reported:
  `diagnose_runtime_pack` MEDIUM risk with 9 direct test callers and 0 affected
  execution flows; `plan_runtime_pack_operation` MEDIUM risk with 12 direct
  test callers and 0 affected execution flows; `BrowserRuntimePackOperationRequest`
  and `BrowserRuntimePackDoctorOutcome` LOW risk with test/module callers only;
  `BrowserService` export surface LOW risk with 0 affected flows.
- Existing browser execution symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- The Phase 2E slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, emit TaskEvents, write settings, or
  write DB migrations.

### Phase 2E Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Initial Rust focused verification before linking `gbrain-source` failed in
  the Tauri build script with `resource path 'gbrain-source' doesn't exist`;
  this was a worktree dependency issue, not a source failure.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `28 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `40 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2602 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check -- <changed-files>` returned no output.

## Phase 2F Entry Criteria

Phase 2F can start because:

- PR #418 merged the Phase 2E status-report aggregator into `main` and
  `origin/main`;
- the Phase 2F worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2f-executor-boundary`;
- the branch starts from `2efcfc56`, the current `origin/main`;
- this slice still avoids real network download, archive extraction, filesystem
  deletion, Playwright worker startup, Tauri commands, DB migrations, and
  Settings/Splash UI.

Recommended Phase 2F tests:

- managed executor blocks network plans unless policy explicitly allows network;
- managed executor blocks destructive cleanup/rollback plans unless policy
  explicitly allows destructive actions;
- successful managed execution calls a runner for each planned step and records
  completed step reports;
- failed runner step stops execution, records the error, and returns artifact /
  event metadata;
- confirmation-required, deferred, and blocked plans do not call the runner.

## Phase 2F Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase2f-executor-boundary.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2f-executor-boundary`
- Branch:
  `codex/browser-runtime-phase2f-executor-boundary`
- Scope:
  policy-gated managed executor boundary for runtime-pack operation plans,
  including execution policy, step-runner trait, step-run outcomes, managed
  execution reports, and focused tests.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 2F executor DTOs/trait/function/tests, browser module
  exports, this status file update, and the Phase 2F plan file.

### Phase 2F Impact Notes

- GitNexus impact before editing reported:
  `execute_runtime_pack_plan_dry_run` MEDIUM risk with 5 direct test callers
  and 0 affected execution flows; `BrowserRuntimePackExecutionMode`,
  `BrowserRuntimePackExecutionStatus`, and `BrowserRuntimePackStepExecutionStatus`
  LOW risk with 0 affected flows; `BrowserRuntimePackStepExecutionReport` and
  `BrowserRuntimePackExecutionReport` LOW risk through the dry-run executor and
  tests only; `BrowserService` export surface LOW risk with 0 affected flows.
- Existing browser execution symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, and `tauri_commands.rs` are not edited.
- The Phase 2F slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, emit TaskEvents, write settings, or
  write DB migrations.

### Phase 2F Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Initial Rust focused verification before linking `pyembed` failed in the
  Tauri build script with `resource path 'pyembed/python' doesn't exist`; this
  was a worktree dependency issue, not a source failure.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`.
- Runtime contract/supervisor regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- Formatting and whitespace checks passed for runtime-pack Rust files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs`
  and `git diff --check` returned no output.
- `src-tauri/src/browser/mod.rs` is export-only in this slice. A full
  `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/browser/mod.rs`
  would reformat the legacy `BrowserService` block and create unrelated diff,
  so that formatting churn was intentionally not accepted into this PR.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 5`, and
  `affected_processes: []`.
- Phase 2F executor boundary was merged through PR #419 as
  `45463455 Merge pull request #419 from novolei/codex/browser-runtime-phase2f-executor-boundary`.

## Phase 3A Entry Criteria

Phase 3A can start because:

- PR #419 merged the Phase 2F managed executor boundary into `main` and
  `origin/main`;
- the Phase 3A worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3a-startup-shell`;
- the branch starts from `45463455`, the current `origin/main`;
- Phase 3 is intentionally split because the ADR scope includes startup brand
  visuals, Startup Doctor, background preparation, recovery UX, settings deep
  links, and screenshot gates;
- this first slice avoids backend doctor IPC, real runtime preparation, final
  visual asset production, root render error recovery, Settings UI, DB
  migrations, root `App` integration, and DMZ files.

Recommended Phase 3A tests:

- Startup Doctor view model defaults to concise checking state and clamps
  progress;
- ready, degraded, and failed check sets derive the correct startup phase;
- Startup Splash renders a concise first frame by default without showing a
  checklist;
- diagnostic details expand on demand and open automatically for attention
  states;
- root `App` loading-state integration is explicitly deferred to Phase 3B
  because it triggered HIGH staged GitNexus detect.

## Phase 3A Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3a-startup-shell.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3a-startup-shell`
- Branch:
  `codex/browser-runtime-phase3a-startup-shell`
- Scope:
  typed frontend Startup Doctor view model, branded local-first Startup Splash
  component, focused Vitest coverage, and tracker updates.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 3A startup model/component/tests, this status file update,
  and the Phase 3A plan file.

### Phase 3A Impact Notes

- GitNexus was refreshed in the Phase 3A worktree before editing; the analyzer
  auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise
  changes were restored before implementation.
- GitNexus impact before the attempted `App` integration in
  `ui/src/App.tsx` reported LOW risk, 0 direct callers, 0 affected processes,
  and 0 affected modules; however, staged GitNexus detect then reported HIGH
  because `App` participates in 9 top-level app processes, so the `App` change
  was removed from Phase 3A and deferred to Phase 3B.
- New Startup Doctor model and Startup Splash component symbols are additive.
- Existing browser runtime/provider symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, `runtime_pack.rs`, `tauri_commands.rs`, and DB
  migrations are not edited.
- The Phase 3A slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, emit TaskEvents, write settings, or
  write DB migrations.

### Phase 3A Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused Startup Doctor and Startup Splash verification passed:
  `cd ui && npm test -- --run src/lib/startup/startup-doctor.test.ts src/components/startup/StartupSplash.test.tsx`
  returned `2 passed`, `8 passed`.
- Default Rust browser-runtime regressions still passed even though Phase 3A
  changes no Rust files:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- A temporary browser-rendered startup shell preview was used during the
  attempted `App` integration, but the root integration was removed after the
  HIGH staged GitNexus detect; final Phase 3A verification relies on focused
  component/model tests and leaves app-route screenshots to Phase 3B.
- UI production build passed:
  `cd ui && npx vite build --outDir /tmp/uclaw-phase3a-vite-build --emptyOutDir`
  returned `built in 9.67s`; Vite emitted the existing
  `tauri-bridge.ts` mixed dynamic/static import warning and large chunk size
  warning.
- `cd ui && npx tsc --noEmit` still fails on pre-existing unrelated type
  errors in automation, browser screencast, settings, hook, login-window, and
  dev-tauri mock tests; none of those files are part of Phase 3A.
- No Rust files changed, so `rustfmt --edition 2021 --check <changed-rust-files>`
  is not applicable for Phase 3A.
- `git diff --check -- <changed-files>` returned no output after the final
  UI/tracker edits.
- Final staged GitNexus detect after removing root `App` integration reported
  `risk_level: low`, `changed_files: 6`, `affected_processes: []`.
- Phase 3A startup shell substrate was merged through PR #420 as
  `2c380373 Merge pull request #420 from novolei/codex/browser-runtime-phase3a-startup-shell`.

## Phase 3B Entry Criteria

Phase 3B can start because:

- PR #420 merged the additive Startup Splash / Startup Doctor substrate into
  `main` and `origin/main`;
- the Phase 3B worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3b-doctor-status-adapter`;
- the branch starts from `2c380373`, the current `origin/main`;
- ADR Phase 3 requires Startup Doctor checks to include runtime manifest,
  runtime-pack path, network state, and last-known runtime status;
- this slice avoids root `App` integration, backend IPC, Settings UI, DB
  migrations, runtime-pack mutation, Playwright launch, and DMZ files.

Recommended Phase 3B tests:

- ready runtime-pack status marks runtime doctor checks passed;
- offline/deferred runtime-pack status remains warning/degraded, not launch
  failure;
- repair/reinstall status recommends details and preserves remediation text;
- blocked runtime-pack operation plans become failed recovery state;
- existing Startup Doctor progress/phase tests still pass.

## Phase 3B Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3b-doctor-status-adapter.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3b-doctor-status-adapter`
- Branch:
  `codex/browser-runtime-phase3b-doctor-status-adapter`
- Scope:
  typed frontend runtime-pack status DTOs, pure mapping into Startup Doctor
  checks, focused Vitest coverage, and tracker updates.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 3B startup-doctor adapter/test additions, this status file
  update, and the Phase 3B plan file.

### Phase 3B Impact Notes

- GitNexus was refreshed in the Phase 3B worktree before implementation; the
  analyzer auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus could not resolve the newly added Phase 3A TypeScript symbols
  (`deriveStartupDoctorViewModel` / `startup-doctor.ts`) as indexed impact
  targets, so Phase 3B avoids modifying the existing
  `deriveStartupDoctorViewModel` function and relies on final staged detect for
  graph impact.
- New runtime-pack status adapter DTOs and helper functions are additive.
- Existing browser runtime/provider Rust symbols remain intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`,
  `BrowserActionRegistry`, `runtime_pack.rs`, `tauri_commands.rs`, and DB
  migrations are not edited.
- The Phase 3B slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright, start MCP, emit TaskEvents, write settings, or
  write DB migrations.

### Phase 3B Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused Startup Doctor adapter verification passed:
  `cd ui && npm test -- --run src/lib/startup/startup-doctor.test.ts`
  returned `1 passed`, `8 passed`.
- Focused Startup Doctor plus Startup Splash regression passed:
  `cd ui && npm test -- --run src/lib/startup/startup-doctor.test.ts src/components/startup/StartupSplash.test.tsx`
  returned `2 passed`, `12 passed`.
- Default Rust browser-runtime regressions still passed even though Phase 3B
  changes no Rust files:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- No Rust files changed, so `rustfmt --edition 2021 --check <changed-rust-files>`
  is not applicable for Phase 3B.
- `git diff --check -- <changed-files>` returned no output after the
  UI/tracker edits.
- Final staged GitNexus detect reported `risk_level: low`,
  `changed_files: 4`, and `affected_processes: []`.
- Phase 3B runtime-pack status adapter was merged through PR #421 as
  `7efe4fee Merge pull request #421 from novolei/codex/browser-runtime-phase3b-doctor-status-adapter`.

## Phase 3C Entry Criteria

Phase 3C can start because:

- PR #421 merged the runtime-pack status adapter into `main` and
  `origin/main`;
- the Phase 3C worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3c-splash-preview-harness`;
- the branch starts from `7efe4fee`, the current `origin/main`;
- ADR Phase 3 requires screenshot checks for Startup Splash / Startup Doctor
  states before production startup wiring;
- root `App` integration still has known HIGH staged-detect risk, so this
  slice keeps the screenshot harness standalone and deterministic.

Recommended Phase 3C tests:

- first-frame preview renders concise loading state without expanding details;
- details preview renders all Startup Doctor checks in a reduced-motion theme;
- ready, deferred, and failed scenarios resolve to the expected startup phase;
- theme query parameters apply supported app shell classes only;
- existing Startup Splash and Startup Doctor regressions still pass.

## Phase 3C Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3c-splash-preview-harness.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3c-splash-preview-harness`
- Branch:
  `codex/browser-runtime-phase3c-splash-preview-harness`
- Scope:
  deterministic Startup Splash scenario fixtures, standalone Vite preview
  page, reduced-motion/theme query handling, focused Vitest coverage, browser
  screenshots, and tracker updates.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Phase 3C preview HTML/entry, scenario helper/tests, this status
  file update, and the Phase 3C plan file.

### Phase 3C Impact Notes

- GitNexus was refreshed in the Phase 3C worktree before implementation; the
  analyzer auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- This slice adds new preview/scenario files only; existing Startup Splash,
  Startup Doctor, root `App`, Tauri IPC, runtime-pack Rust, provider, and DB
  migration symbols are not modified.
- Because no existing function, class, method, or symbol is edited, there are
  no pre-change GitNexus impact targets for this slice; final staged detect
  remains the graph closeout gate.
- The Phase 3C slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright as a provider, start MCP, emit TaskEvents, write
  settings, or write DB migrations.

### Phase 3C Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused preview and startup regression verification passed:
  `cd ui && npm test -- --run src/components/startup/startup-splash-scenarios.test.ts src/components/startup/StartupSplash.test.tsx src/lib/startup/startup-doctor.test.ts`
  returned `3 passed`, `16 passed`.
- Browser preview checks passed against the standalone Vite preview page:
  first-frame rendered `Preparing uClaw` with collapsed details; the
  details-expanded `qingye` reduced-motion scenario rendered all eight Startup
  Doctor checks. Console checks reported `errors 0` and `warnings 0` for both
  navigations. Screenshots were captured as
  `uclaw-phase3c-first-frame.png` and
  `uclaw-phase3c-details-qingye-reduced.png`.
- Default Rust browser-runtime regressions still passed even though Phase 3C
  changes no Rust files:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- No Rust files changed, so `rustfmt --edition 2021 --check <changed-rust-files>`
  is not applicable for Phase 3C.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output after the final UI/tracker edits.
- Final staged GitNexus detect reported `risk_level: low`,
  `changed_files: 6`, and `affected_processes: []`.
- Phase 3C startup splash preview harness was merged through PR #422 as
  `0eb46780 Merge pull request #422 from novolei/codex/browser-runtime-phase3c-splash-preview-harness`.

## Phase 3D Entry Criteria

Phase 3D could start because:

- PR #422 merged the standalone Startup Splash preview harness into `main` and
  `origin/main`;
- the Phase 3D worktree was isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3d-app-startup-route`;
- the branch started from `0eb46780`, the current `origin/main`;
- ADR Phase 3 requires replacing the generic initialization surface with the
  branded startup route;
- GitNexus impact for `App` in `ui/src/App.tsx` initially reported LOW risk
  with 0 direct callers, 0 affected processes, and 0 affected modules before
  editing.

## Phase 3D Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3d-app-startup-route.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3d-app-startup-route`
- Branch:
  `codex/browser-runtime-phase3d-app-startup-route`
- Scope attempted:
  replace the root `App` loading branch with the verified Startup Splash,
  preserve existing initialization/handoff behavior, add focused App tests, and
  update this tracker.
- Result:
  stopped before commit and PR because final staged GitNexus detect reported
  HIGH risk.

### Phase 3D Impact Notes

- GitNexus was refreshed in the Phase 3D worktree before implementation; the
  analyzer auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus impact for `App` reported LOW risk before editing, but final staged
  detect reported HIGH risk because touching `App` affected 9 top-level app
  processes: `App -> MakeListener`, `App -> UpdateState`, `App -> Reg`,
  `App -> CreateInitialStreamState`, `App -> BuildResolvedTarget`,
  `App -> UpsertBrowserTaskStep`, `App -> SafeU`, `App -> GetSettings`, and
  `App -> GetCachedStickyUserMessage`.
- Stop decision:
  the Phase 3D proposal is not committed or pushed. Do not retry root `App`
  startup routing without an explicit HIGH-risk reviewer plan or a lower-risk
  ownership boundary.

### Phase 3D Verification Notes

- Focused App startup-route verification passed before the stop:
  `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx src/lib/startup/startup-doctor.test.ts`
  returned `3 passed`, `14 passed`.
- Browser root smoke with `VITE_UCLAW_MOCK_TAURI=1` rendered the main app into
  the existing root error boundary after initialization because
  `WelcomeView.tsx` reads `.filter` from a null dev-mock value. The same error,
  with the same `11 errors / 11 warnings` console shape, reproduced on synced
  `main` at `0eb46780` without Phase 3D changes, so this is recorded as a
  pre-existing dev-mock shell issue rather than a Phase 3D regression.
- Default Rust browser-runtime regressions passed before the stop:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output, but this phase did not pass GitNexus and has no commit.

## Phase 3E Entry Criteria

Phase 3E can start because:

- Phase 3D root `App` startup routing is blocked by HIGH staged GitNexus risk;
- ADR Phase 3 still has a lower-risk requirement for branded recovery surfaces
  covering runtime setup failure, offline mode, deferred preparation, and
  unavailable provider states;
- the Phase 3E worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3e-startup-recovery-surfaces`;
- the branch starts from `0eb46780`, the current `origin/main`;
- GitNexus impact for `StartupSplash` reported LOW risk with 1 direct caller
  (`startup-splash-preview.tsx`) and 0 affected processes before editing;
- GitNexus impact for `getStartupSplashScenario` reported LOW risk with
  preview-only Startup module impact and 0 affected processes before editing.

Recommended Phase 3E tests:

- degraded Startup Doctor state renders branded recovery guidance;
- failed runtime setup state renders recovery guidance with the blocking
  remediation detail;
- controlled-closed recovery guidance can reveal diagnostics through the
  existing details callback;
- preview scenarios include a collapsed offline recovery state;
- existing Startup Splash and Startup Doctor regressions still pass.

## Phase 3E Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3e-startup-recovery-surfaces.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3e-startup-recovery-surfaces`
- Branch:
  `codex/browser-runtime-phase3e-startup-recovery-surfaces`
- Scope:
  add side-effect-free Startup Splash recovery panel for degraded/failed
  states, add an `offline-recovery` preview scenario, focused tests, and this
  tracker update.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the Startup Splash recovery panel, scenario/test updates, this status
  file update, and the Phase 3E plan file.

### Phase 3E Impact Notes

- GitNexus was refreshed in the Phase 3E worktree before implementation; the
  analyzer auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus impact for `StartupSplash` and `getStartupSplashScenario` both
  reported LOW risk and 0 affected execution flows before editing.
- This slice modifies only Startup Splash / preview harness surfaces and does
  not change root `App`, `main.tsx`, backend IPC, runtime-pack Rust, provider
  code, Settings, or DB migrations.
- The Phase 3E slice does not download, install, repair, cleanup, roll back,
  spawn Node, run Playwright as a provider, start MCP, emit TaskEvents, write
  settings, or write DB migrations.

### Phase 3E Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused Startup UI verification passed:
  `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx src/components/startup/startup-splash-scenarios.test.ts src/lib/startup/startup-doctor.test.ts`
  returned `3 passed`, `18 passed`.
- Browser preview verification passed for the collapsed offline recovery state:
  `http://127.0.0.1:5177/startup-splash-preview.html?scenario=offline-recovery&theme=light`
  rendered the recovery panel with diagnostics collapsed; console check returned
  `Errors: 0, Warnings: 0` after adding an inline empty favicon to the preview
  page. Screenshot artifact:
  `uclaw-phase3e-offline-recovery-clean.png`.
- Browser preview verification passed for the failed recovery state:
  `http://127.0.0.1:5177/startup-splash-preview.html?scenario=failed&theme=qingye&motion=reduced`
  rendered the `Recovery needed` panel and failed runtime diagnostics; console
  check returned `Errors: 0, Warnings: 0`. Screenshot artifact:
  `uclaw-phase3e-failed-recovery-qingye.png`.
- Default Browser Runtime Rust regressions passed even though Phase 3E changed
  no Rust code:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- Final staged GitNexus detect for the Phase 3E worktree reported
  `risk_level: low`, `changed_files: 7`, `changed_count: 26`, and
  `affected_processes: []`.
- Phase 3E startup recovery surfaces were merged through PR #423 as
  `f2dabbe3 Merge pull request #423 from novolei/codex/browser-runtime-phase3e-startup-recovery-surfaces`.

## Phase 3F Entry Criteria

Phase 3F can start because:

- PR #423 merged Phase 3E Startup Splash recovery surfaces into `main` and
  `origin/main`;
- the Phase 3F worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3f-root-route-reviewer-plan`;
- the branch starts from `f2dabbe3`, the current `origin/main`;
- ADR Phase 3 still requires root route integration, but the only attempted
  root `App` startup routing slice stopped after HIGH staged GitNexus risk;
- ADR Phase 4 is gated on the Phase 3 shell route and must not start while
  root startup routing is still unresolved.

Recommended Phase 3F checks:

- tracker marks Phase 3E as merged through PR #423;
- tracker records that Phase 4 is blocked by the Phase 3 shell-route gate;
- plan answers ADR section 18 and defines the root `App` reviewer requirement;
- docs-only diff has no whitespace errors and no affected execution flows.

## Phase 3F Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3f-root-route-reviewer-plan.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3f-root-route-reviewer-plan`
- Branch:
  `codex/browser-runtime-phase3f-root-route-reviewer-plan`
- Scope:
  close the Phase 3E tracker state, record PR #423 / commit / merge commit,
  and codify the HIGH-risk root `App` reviewer gate before any future shell
  route implementation.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this docs-only tracker and Phase 3F plan update.

### Phase 3F Impact Notes

- No source symbols are edited in Phase 3F, so pre-edit GitNexus impact is not
  required.
- Final staged GitNexus detect is still required before commit.
- This slice does not touch root `App`, `main.tsx`, AppShell, backend IPC,
  runtime-pack Rust, provider code, Settings, DB migrations, TaskEvents, or
  runtime side effects.

### Phase 3F Verification Notes

- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase3f-root-route-reviewer-plan.md`
  returned no output.
- `git diff --cached --check` returned no output.
- GitNexus was refreshed for the Phase 3F worktree; analyzer-updated
  `AGENTS.md` / `CLAUDE.md` statistics were restored because they are outside
  this docs-only scope.
- Final staged GitNexus detect for the Phase 3F worktree reported
  `risk_level: low`, `changed_files: 2`, `changed_count: 21`, and
  `affected_processes: []`.
- Phase 3F root route reviewer gate was merged through PR #424 as
  `3e9e4817 Merge pull request #424 from novolei/codex/browser-runtime-phase3f-root-route-reviewer-plan`.

## Phase 3G Entry Criteria

Phase 3G can start because:

- PR #424 merged the Phase 3F root route reviewer gate into `main` and
  `origin/main`;
- the Phase 3G worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3g-app-route-review-pack`;
- the branch starts from `3e9e4817`, the current `origin/main`;
- `BEHAVIOR.md` section 8 requires a writer/reviewer flow for anything flagged
  HIGH/CRITICAL by GitNexus;
- ADR Phase 3 still requires the main Tauri WebView first route to use the
  branded startup experience;
- ADR Phase 4 remains blocked until that Phase 3 shell route is reviewed and
  landed.

Recommended Phase 3G checks:

- reviewer pack names the Phase 3D HIGH-risk affected processes;
- reviewer pack defines writer allowed files and non-goals;
- reviewer pack defines the fresh-session reviewer prompt;
- reviewer pack defines go/no-go gates and verification for any future root
  `App` writer PR;
- docs-only diff has no whitespace errors and no affected execution flows.

## Phase 3G Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3g-app-route-review-pack.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3g-app-route-review-pack`
- Branch:
  `codex/browser-runtime-phase3g-app-route-review-pack`
- Scope:
  close the Phase 3F tracker state, define the future root `App` writer scope,
  define the reviewer prompt, and document go/no-go gates for accepting the
  HIGH-risk blast radius.
- Current PR:
  [#425](https://github.com/novolei/uclaw-new/pull/425)
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this docs-only tracker and Phase 3G plan update.

### Phase 3G Impact Notes

- No source symbols are edited in Phase 3G, so pre-edit GitNexus impact is not
  required.
- Final staged GitNexus detect is still required before commit.
- This slice does not touch root `App`, `main.tsx`, AppShell, Startup Splash,
  backend IPC, runtime-pack Rust, provider code, Settings, DB migrations,
  TaskEvents, Playwright, MCP, or runtime side effects.
- Phase 3G does not approve the future HIGH-risk root `App` edit; it defines
  the acceptance pack that the DRI/user must explicitly accept before a writer
  session implements Phase 3H.

### Phase 3G Verification Notes

- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase3g-app-route-review-pack.md`
  returned no output.
- `git diff --cached --check` returned no output.
- GitNexus was refreshed for the Phase 3G worktree; analyzer-updated
  `AGENTS.md` / `CLAUDE.md` statistics were restored because they are outside
  this docs-only scope.
- Final staged GitNexus detect for the Phase 3G worktree reported
  `risk_level: low`, `changed_files: 2`, `changed_count: 23`, and
  `affected_processes: []`.
- Phase 3G root `App` review acceptance pack was merged through PR #425 as
  `c5ce25c1 Merge pull request #425 from novolei/codex/browser-runtime-phase3g-app-route-review-pack`.

## Phase 3H Entry Criteria

Phase 3H can start because:

- PR #425 merged the root `App` route review acceptance pack into `main` and
  `origin/main`;
- the Phase 3H worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3h-app-startup-route`;
- the branch starts from `c5ce25c1`, the current `origin/main`;
- ADR Phase 3 requires the main Tauri WebView first route to use the branded
  Startup Splash;
- Phase 3G defines the writer scope and reviewer prompt for this HIGH-risk
  root `App` path;
- pre-edit GitNexus impact for `App` in `ui/src/App.tsx` reported LOW risk,
  0 direct callers, 0 affected processes, and 0 affected modules.

Recommended Phase 3H checks:

- root loading branch renders `StartupSplash` before initialization resolves;
- existing initialization still writes cached language, initializes UI
  preferences, queries active model, and hands off to `AppShell`;
- Startup Splash component regressions still pass;
- standalone preview screenshots for first-frame/details/offline/failed remain
  console-clean;
- root app smoke either reaches AppShell or reproduces only the known
  post-handoff `WelcomeView.tsx` null `.filter` dev-mock issue;
- final staged GitNexus detect reports no new affected processes beyond the
  Phase 3D list if HIGH appears.

## Phase 3H Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase3h-app-startup-route.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3h-app-startup-route`
- Branch:
  `codex/browser-runtime-phase3h-app-startup-route`
- Scope:
  replace only the root `App` loading spinner branch with `StartupSplash`, add
  focused App tests, and update this tracker.
- Current PR:
  [#426](https://github.com/novolei/uclaw-new/pull/426)
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the App loading-branch swap, App tests, this status file update, and
  the Phase 3H plan file.

### Phase 3H Impact Notes

- GitNexus was refreshed in the Phase 3H worktree before implementation; the
  analyzer auto-updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus impact for `App` reported LOW risk, 0 direct callers, and
  0 affected execution flows before editing.
- This slice modifies only the root loading branch and focused tests. It does
  not change `main.tsx`, AppShell, global listeners, backend IPC, runtime-pack
  Rust, provider code, Settings, DB migrations, TaskEvents, Playwright, MCP, or
  runtime side effects.

### Phase 3H Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Focused App/Startup UI verification passed:
  `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx src/lib/startup/startup-doctor.test.ts`
  returned `3 passed`, `16 passed`.
- Browser preview verification passed with console warnings/errors clean for
  the required Phase 3H scenarios:
  first-frame `light` screenshot `uclaw-phase3h-first-frame.png`;
  details-expanded `qingye` reduced-motion screenshot
  `uclaw-phase3h-details-qingye.png`;
  offline recovery `light` screenshot `uclaw-phase3h-offline-recovery.png`;
  failed recovery `qingye` reduced-motion screenshot
  `uclaw-phase3h-failed-qingye.png`. Each preview console check returned
  `Errors: 0, Warnings: 0`.
- Root app smoke under `VITE_UCLAW_MOCK_TAURI=1` reached the existing root
  error boundary after startup handoff with the known
  `WelcomeView.tsx` null `.filter` dev-mock failure and the same
  `11 errors / 11 warnings` console shape recorded in Phase 3D. This remains a
  pre-existing post-handoff dev-mock issue, not a Phase 3H loading-route
  regression.
- Default Browser Runtime Rust regressions passed even though Phase 3H changed
  no Rust code:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- Final staged GitNexus detect for the Phase 3H worktree reported
  `risk_level: high`, `changed_files: 4`, `changed_count: 16`, and the same
  9 known affected `App` processes from Phase 3D/3G: `App -> MakeListener`,
  `App -> UpdateState`, `App -> Reg`, `App -> CreateInitialStreamState`,
  `App -> BuildResolvedTarget`, `App -> UpsertBrowserTaskStep`,
  `App -> SafeU`, `App -> GetSettings`, and
  `App -> GetCachedStickyUserMessage`.
- No new affected process names appeared beyond the Phase 3D list.
- Fresh reviewer sub-agent accepted PR #426 after checking that listener
  registration, settings/model initialization, AppShell handoff, and root error
  behavior were preserved. PR #426 merged as `13133bb1`.

## Phase 4A Entry Criteria

Phase 4A can start because:

- PR #426 merged the branded root Startup Splash route into `main` and
  `origin/main`;
- a fresh reviewer sub-agent explicitly returned `REVIEW ACCEPTED` for the
  known HIGH `App` blast radius before merge;
- the Phase 4A worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4a-settings-surface`;
- the branch starts from `13133bb1`, the current `origin/main`;
- ADR Phase 4 asks for a first-class Browser Runtime / Startup Doctor /
  Browser Identity settings destination;
- this slice is scoped to readonly settings surface and typed frontend adapter
  only, leaving IPC and runtime mutations for later Phase 4 slices.

Recommended Phase 4A checks:

- Browser Runtime tab appears in Settings navigation;
- readonly surface shows status, last check, version, artifact size, runtime
  pack path, release channel, update state, rollback state, developer fallback,
  and auto-prepare state;
- action affordances are visible but disabled because Phase 4A owns no runtime
  side effects;
- focused view-model and settings rendering tests pass;
- default browser-runtime Rust regressions still pass.

## Phase 4A Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4a-settings-surface.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4a-settings-surface`
- Branch:
  `codex/browser-runtime-phase4a-settings-surface`
- Scope:
  add a readonly Browser Runtime settings destination, a typed settings
  view-model adapter over the Phase 2 runtime-pack status report, inert action
  affordances, focused tests, and this tracker update.
- Current PR:
  [#427](https://github.com/novolei/uclaw-new/pull/427), accepted by fresh
  reviewer and merged as `5e0f18fb`.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the settings tab wiring, new Browser Runtime settings component,
  browser-runtime settings view-model/tests, this status file update, and the
  Phase 4A plan file.

### Phase 4A Impact Notes

- `npx gitnexus analyze` refreshed the main repo index before Phase 4A impact
  analysis. It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus impact for `SettingsPanel` reported LOW risk, 0 direct callers, and
  0 affected processes.
- GitNexus impact for `SettingsContent` reported LOW risk, 1 direct caller
  (`SettingsPanel`), and 1 affected settings process.
- GitNexus impact for `SettingsNav` reported LOW risk, 0 direct callers, and
  0 affected processes.
- GitNexus did not resolve the `SettingsTab` type alias; the manual change is
  limited to adding the `browserRuntime` union member.
- This slice does not change backend IPC, runtime-pack Rust behavior,
  provider selection, SearchPalette, task checkpointing, DB migrations, or
  real prepare/repair/cleanup/rollback side effects.

### Phase 4A Verification Notes

- Phase 4A linked ignored `ui/node_modules` from the primary worktree because
  isolated worktrees do not copy frontend dependencies.
- Initial focused UI verification failed with `vitest: command not found`
  before linking `ui/node_modules`; this was a worktree dependency issue, not a
  source failure.
- Focused UI verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/components/settings/SettingsNav.test.tsx`
  returned `3 passed`, `10 passed`.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` returned no output.
- Final staged GitNexus detect for the Phase 4A worktree reported
  `risk_level: high`, `changed_files: 10`, `changed_count: 49`, and
  10 affected settings execution flows: `SettingsPanel -> Cn`,
  `SettingsPanel -> ProviderEmptyState`, `SettingsPanel -> OnTurnCost`,
  `SettingsPanel -> SettingsSection`,
  `SettingsPanel -> ReadWorkspaceUclawMd`,
  `SettingsPanel -> ReadDefaultPrompts`,
  `SettingsPanel -> OpenWorkspaceUclawMdExternally`,
  `SettingsContent -> SettingsSection`,
  `SettingsContent -> GetMemoryRecallConfig`, and
  `SettingsPanel -> Matches`.
- Fresh reviewer sub-agent accepted PR #427 after checking Settings
  navigation, tab rendering, badges, SettingsPanel handoff, and no runtime side
  effects. PR #427 merged as `5e0f18fb`.

## Phase 4B Entry Criteria

Phase 4B can start because:

- PR #427 merged the readonly Browser Runtime settings destination into `main`
  and `origin/main`;
- a fresh reviewer sub-agent explicitly returned `REVIEW ACCEPTED` for the
  known HIGH settings-surface blast radius before merge;
- the Phase 4B worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4b-settings-action-intents`;
- the branch starts from `5e0f18fb`, the current `origin/main`;
- ADR Phase 4 asks for Browser Runtime controls that expose prepare, repair,
  reinstall, cleanup, rollback, and doctor paths;
- this slice is scoped to local action-intent previews only, leaving IPC,
  backend execution, policy prompts, task checkpointing, deep links, and real
  runtime mutations for later Phase 4 slices.

Recommended Phase 4B checks:

- action availability is derived from a runtime-pack status report and stays
  disabled when no report exists;
- selecting an enabled action only updates local preview state;
- preview metadata exposes summary, event names, destructive status, and
  confirmation requirements;
- focused view-model and settings rendering tests pass;
- default browser-runtime Rust regressions still pass.

## Phase 4B Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4b-settings-action-intents.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4b-settings-action-intents`
- Branch:
  `codex/browser-runtime-phase4b-settings-action-intents`
- Scope:
  make Browser Runtime settings actions selectable as local intent previews,
  with no IPC, no backend command, no TaskEvent emission, and no runtime
  filesystem/network/process side effects.
- Current PR:
  [#428](https://github.com/novolei/uclaw-new/pull/428) merged.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the settings action preview view-model/component/tests, this status
  file update, and the Phase 4B plan file.

### Phase 4B Impact Notes

- `npx gitnexus analyze` refreshed the main repo index before Phase 4B impact
  analysis. It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those
  noise changes were restored before implementation.
- GitNexus impact for `BrowserRuntimeSettings` reported LOW risk with
  `SettingsContent` as the direct caller and the existing settings fanout as the
  affected process surface.
- GitNexus impact for `deriveBrowserRuntimeSettingsViewModel` reported LOW
  risk with `BrowserRuntimeSettings` as the direct caller and the same settings
  fanout surface.
- Phase 4B also folds in the PR #427 reviewer P3 follow-up: the default
  settings view-model now reports unknown auto-prepare state as
  `等待运行时状态` instead of implying startup auto-prepare is known to be
  enabled before IPC/status input exists.
- This slice does not change backend IPC, runtime-pack Rust behavior, provider
  selection, SearchPalette, Startup Doctor deep links, task checkpointing, DB
  migrations, TaskEvents, or real prepare/repair/reinstall/cleanup/rollback
  side effects.

### Phase 4B Verification Notes

- Phase 4B used ignored `ui/node_modules` from the primary worktree because
  isolated worktrees do not copy frontend dependencies.
- Focused UI verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
  returned `2 passed`, `7 passed`.
- The first focused UI rerun after the PR #427 reviewer follow-up failed
  because `getByText('等待运行时状态')` became ambiguous across the runtime-pack
  path and auto-prepare rows; the test was corrected to assert multiple
  occurrences, and the rerun passed.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` returned no output.
- Final staged GitNexus detect for the Phase 4B worktree reported
  `risk_level: low`, `changed_files: 6`, `changed_count: 23`, and
  `affected_processes: []`.
- Phase 4B action-intent previews were merged through PR #428 as
  `d3f9f995 Merge pull request #428 from novolei/codex/browser-runtime-phase4b-settings-action-intents`.

## Phase 4C Entry Criteria

Phase 4C can start because:

- PR #428 merged the local action-intent preview surface into `main` and
  `origin/main`;
- the Phase 4C worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4c-auto-prepare-control`;
- the branch starts from `d3f9f995`, the current `origin/main`;
- ADR Phase 4 requires `disable-auto-prepare` controls and explicitly states
  that disabling automatic preparation must not disable browser automation
  capability;
- this slice is scoped to local auto-prepare control previews only, leaving
  settings persistence, IPC, backend policy prompts, deep links, task-time
  prompts, and real runtime mutations for later Phase 4 slices.

Recommended Phase 4C checks:

- auto-prepare unknown state remains disabled before IPC/status input exists;
- auto-prepare enabled state exposes a `关闭自动准备` preview intent;
- auto-prepare disabled state exposes a `开启自动准备` preview intent;
- preview copy explains that task-time browser use may still request runtime
  preparation;
- focused view-model and settings rendering tests pass;
- default browser-runtime Rust regressions still pass.

## Phase 4C Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4c-auto-prepare-control.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4c-auto-prepare-control`
- Branch:
  `codex/browser-runtime-phase4c-auto-prepare-control`
- Scope:
  add local no-side-effect Browser Runtime Settings preview intents for
  disabling/enabling startup/background auto-prepare, while keeping browser
  automation capability and task-time preparation separate.
- Current PR:
  [#429](https://github.com/novolei/uclaw-new/pull/429) merged.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the auto-prepare action preview view-model/component/tests, this
  status file update, and the Phase 4C plan file.

### Phase 4C Impact Notes

- `npx gitnexus analyze` indexed the Phase 4C worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored before implementation.
- GitNexus impact for `BrowserRuntimeSettings` reported LOW risk with
  `SettingsContent` as the direct caller and SettingsPanel/SettingsContent as
  the affected settings surface.
- GitNexus impact for `deriveBrowserRuntimeSettingsViewModel` reported LOW
  risk with `BrowserRuntimeSettings` as the direct caller and the same settings
  surface.
- GitNexus impact for `deriveActions` reported LOW risk through
  `deriveBrowserRuntimeSettingsViewModel` and `BrowserRuntimeSettings`.
- GitNexus did not resolve private helper symbols `actionPreview` or
  `actionSummary`; manual edits are limited to their local preview copy/event
  derivation.
- This slice does not change backend IPC, settings persistence, runtime-pack
  Rust behavior, provider selection, SearchPalette, Startup Doctor deep links,
  task checkpointing, DB migrations, TaskEvents, or real
  prepare/repair/reinstall/cleanup/rollback/auto-prepare side effects.

### Phase 4C Verification Notes

- Phase 4C linked ignored `ui/node_modules`, `src-tauri/pyembed`,
  `src-tauri/bunembed`, and `src-tauri/gbrain-source` from the primary worktree
  because isolated worktrees do not copy local dependencies/resources.
- Focused UI verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
  returned `2 passed`, `9 passed`.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- Final staged GitNexus detect for the Phase 4C worktree reported
  `risk_level: low`, `changed_files: 6`, `changed_count: 38`, and
  `affected_processes: []`.
- Phase 4C auto-prepare control intent was merged through PR #429 as
  `50b5ab8f Merge pull request #429 from novolei/codex/browser-runtime-phase4c-auto-prepare-control`.

## Phase 4D Entry Criteria

Phase 4D can start because:

- PR #429 merged the no-side-effect auto-prepare control semantics into `main`
  and `origin/main`;
- the Phase 4D worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4d-task-time-prompt-model`;
- the branch starts from `50b5ab8f`, the current `origin/main`;
- ADR Phase 4 requires a task-time "prepare Browser runtime" confirmation with
  prepare-now, defer, and continue-without-browser lanes;
- this slice is scoped to a pure frontend prompt model only, leaving UI
  rendering, IPC, TaskEvents, real checkpoint writes, deep links, and runtime
  execution for later Phase 4 slices.

Recommended Phase 4D checks:

- ready runtime reports do not show a prompt;
- planned runtime preparation offers a primary prepare-now action;
- defer records `paused_waiting_for_browser_runtime` only when browser is
  required and no no-browser fallback can satisfy the task;
- no-browser fallback is enabled only when the caller says it can satisfy the
  task;
- blocked runtime state disables prepare-now and preserves fallback/defer
  choices;
- focused prompt-model tests pass;
- default browser-runtime Rust regressions still pass.

## Phase 4D Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4d-task-time-prompt-model.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4d-task-time-prompt-model`
- Branch:
  `codex/browser-runtime-phase4d-task-time-prompt-model`
- Scope:
  add a pure task-time Browser Runtime prompt model deriving prepare-now,
  defer/checkpoint-intent, and continue-without-browser choices from runtime
  status and explicit task fallback context.
- Current PR:
  [#430](https://github.com/novolei/uclaw-new/pull/430) merged.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the task-time prompt model/tests, this status file update, and the
  Phase 4D plan file.

### Phase 4D Impact Notes

- This slice adds new frontend model/test files only and does not modify any
  existing function, class, method, backend module, settings component, or DMZ
  file.
- This slice does not change backend IPC, settings persistence, runtime-pack
  Rust behavior, provider selection, SearchPalette, Startup Doctor deep links,
  task checkpointing, DB migrations, TaskEvents, or real runtime side effects.

### Phase 4D Verification Notes

- Phase 4D linked ignored `ui/node_modules`, `src-tauri/pyembed`,
  `src-tauri/bunembed`, and `src-tauri/gbrain-source` from the primary worktree
  because isolated worktrees do not copy local dependencies/resources.
- Focused prompt-model verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
  returned `1 passed`, `4 passed`.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- Final staged GitNexus detect for the Phase 4D worktree reported
  `risk_level: low`, `changed_files: 4`, `changed_count: 37`, and
  `affected_processes: []`.
- Phase 4D task-time prompt model was merged through PR #430 as
  `7d4f70e0 Merge pull request #430 from novolei/codex/browser-runtime-phase4d-task-time-prompt-model`.

## Phase 4E Entry Criteria

Phase 4E can start because:

- PR #430 merged the pure task-time prompt model into `main` and `origin/main`;
- the Phase 4E worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4e-task-time-prompt-ui`;
- the branch starts from `7d4f70e0`, the current `origin/main`;
- ADR Phase 4 requires a task-time Browser Runtime confirmation with prepare,
  defer, and no-browser fallback decisions;
- this slice is scoped to rendering the Phase 4D model only, leaving App/task
  runtime wiring, IPC, TaskEvents, real checkpoint writes, deep links, and
  runtime execution for later Phase 4 slices.

Recommended Phase 4E checks:

- ready runtime models render no prompt;
- prepare-required models render prepare-now, defer, disabled no-browser
  fallback, event preview, and checkpoint metadata;
- local action selection calls a callback only and does not execute runtime
  side effects;
- blocked runtime models can make no-browser fallback the primary choice when
  the task context says it is available;
- focused prompt UI tests pass;
- default browser-runtime Rust regressions still pass.

## Phase 4E Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4e-task-time-prompt-ui.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4e-task-time-prompt-ui`
- Branch:
  `codex/browser-runtime-phase4e-task-time-prompt-ui`
- Scope:
  add a standalone React prompt component that renders the Phase 4D model,
  shows checkpoint/event-preview metadata, and reports local action selection
  to a caller callback.
- Current PR:
  [#431](https://github.com/novolei/uclaw-new/pull/431) merged.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the task-time prompt UI/tests, this status file update, and the Phase
  4E plan file.

### Phase 4E Impact Notes

- This slice adds new frontend component/test files and updates tracker/plan
  docs only; it does not modify existing function, class, method, backend
  module, settings component, or DMZ file.
- This slice does not change backend IPC, settings persistence, runtime-pack
  Rust behavior, provider selection, SearchPalette, Startup Doctor deep links,
  task checkpointing, DB migrations, TaskEvents, or real runtime side effects.

### Phase 4E Verification Notes

- Phase 4E linked ignored `ui/node_modules`, `src-tauri/pyembed`,
  `src-tauri/bunembed`, and `src-tauri/gbrain-source` from the primary worktree
  because isolated worktrees do not copy local dependencies/resources.
- Focused prompt UI verification passed:
  `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
  returned `1 passed`, `4 passed`.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2580 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2568 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2606 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- Final staged GitNexus detect for the Phase 4E worktree reported
  `risk_level: low`, `changed_files: 4`, `changed_count: 33`, and
  `affected_processes: []`.
- Phase 4E task-time prompt UI was merged through PR #431 as
  `ab59f9aa Merge pull request #431 from novolei/codex/browser-runtime-phase4e-task-time-prompt-ui`.

## Phase 4F Entry Criteria

Phase 4F can start because:

- PR #431 merged the standalone task-time prompt UI into `main` and
  `origin/main`;
- the Phase 4F worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4f-settings-deep-link`;
- the branch starts from `ab59f9aa`, the current `origin/main`;
- ADR Phase 4 requires Settings deep links from SearchPalette, Startup Doctor,
  task-time prompts, and error/recovery surfaces;
- AppShell currently has a SearchPalette settings TODO, making SearchPalette the
  narrowest reversible first deep-link source.

Recommended Phase 4F checks:

- SearchPalette renders the Browser Runtime settings shortcut with a clear hint;
- selecting the Browser Runtime shortcut emits a settings payload carrying
  `settingsTab: 'browserRuntime'`;
- AppShell opens the Settings dialog on a supplied settings tab without
  creating fake tabs or backend side effects;
- focused SearchPalette tests pass;
- UI build verifies AppShell type wiring;
- default browser-runtime Rust regressions still pass.

## Phase 4F Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4f-settings-deep-link.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4f-settings-deep-link`
- Branch:
  `codex/browser-runtime-phase4f-settings-deep-link`
- Scope:
  wire SearchPalette settings items to explicit Settings tabs and open the
  existing Settings dialog on the Browser Runtime tab when that shortcut is
  selected.
- Current PR:
  [#432](https://github.com/novolei/uclaw-new/pull/432).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the SearchPalette/AppShell deep-link changes, this status file update,
  and the Phase 4F plan file.

### Phase 4F Impact Notes

- `npx gitnexus analyze` indexed the Phase 4F worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `SearchPalette` in
  `ui/src/components/search/SearchPalette.tsx` reported LOW risk, one direct
  caller (`AppShell`), affected process labels `App` and `AppShell`.
- Pre-edit GitNexus impact for `SETTINGS_ITEMS` reported LOW risk, zero direct
  callers, and zero affected processes.
- Pre-edit GitNexus impact for `SettingsItem` reported LOW risk with AppShell
  import/file relationship only.
- Pre-edit GitNexus impact for `handleSearchResultSelect` in
  `ui/src/components/app-shell/AppShell.tsx` reported LOW risk, zero direct
  callers, and zero affected processes.
- This slice does not change backend IPC, settings persistence, runtime-pack
  Rust behavior, provider selection, Startup Doctor, task checkpointing, DB
  migrations, TaskEvents, or real runtime side effects.

### Phase 4F Verification Notes

- Focused SearchPalette verification passed:
  `cd ui && npm test -- --run src/components/search/SearchPalette.test.tsx`
  completed with 1 file / 17 tests passed.
- UI build verification passed:
  `cd ui && npm run build` completed successfully, with the existing Vite
  dynamic-import/chunk-size warnings only.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with 32 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with 44 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with 6 passed. Existing unrelated Rust warnings remained.
- Phase 4F linked ignored local `ui/node_modules`, `src-tauri/pyembed`,
  `src-tauri/bunembed`, and `src-tauri/gbrain-source` resources from the
  primary worktree for local verification only.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- Whitespace checks passed:
  `git diff --check -- <changed-files>` and `git diff --cached --check`
  produced no output.
- Staged GitNexus detect for the Phase 4F worktree reported MEDIUM risk:
  5 changed files, 32 changed symbols, and 5 affected AppShell process labels.
  No HIGH or CRITICAL risk was reported.
- Phase 4F SearchPalette settings deep link was merged through PR #432 as
  `00ce02ed Merge pull request #432 from novolei/codex/browser-runtime-phase4f-settings-deep-link`.

## Phase 4G Entry Criteria

Phase 4G can start because:

- PR #432 merged the SearchPalette -> Browser Runtime Settings deep link into
  `main` and `origin/main`;
- the Phase 4G worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4g-startup-doctor-deep-link`;
- the branch starts from `00ce02ed`, the current `origin/main`;
- ADR Phase 4 still requires Settings deep links from Startup Doctor, task-time
  runtime prompts, and error/recovery surfaces;
- `StartupSplash` already owns Startup Doctor recovery rendering and has LOW
  pre-edit impact, while root `App` remains a separate higher-risk wiring
  boundary.

Recommended Phase 4G checks:

- StartupSplash renders a Browser Runtime Settings action only when a
  browser-runtime doctor check is warning or failed and a callback is supplied;
- clicking the action calls the supplied callback exactly once;
- unrelated doctor attention does not render a Browser Runtime Settings action;
- existing StartupSplash first-frame, diagnostics, and recovery behavior remain
  covered by focused tests;
- UI build still passes;
- default browser-runtime Rust regressions still pass.

## Phase 4G Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4g-startup-doctor-deep-link.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4g-startup-doctor-deep-link`
- Branch:
  `codex/browser-runtime-phase4g-startup-doctor-deep-link`
- Scope:
  add a component-scoped Startup Doctor settings deep-link affordance through
  an optional `StartupSplash` callback, without wiring root `App` or executing
  runtime actions.
- Current PR:
  [#433](https://github.com/novolei/uclaw-new/pull/433).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the StartupSplash callback/button changes, focused tests, this status
  file update, and the Phase 4G plan file.

### Phase 4G Impact Notes

- `npx gitnexus analyze` indexed the Phase 4G worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `StartupSplash` in
  `ui/src/components/startup/StartupSplash.tsx` reported LOW risk, two direct
  callers (`App` and `startup-splash-preview.tsx`), and one affected process
  label (`App`).
- Pre-edit GitNexus impact for `StartupCheckRow` reported LOW risk with
  `StartupSplash` as the direct caller and the same `App` process label.
- Pre-edit GitNexus impact for `startupRecoverySurface` reported LOW risk with
  `StartupSplash` as the direct caller and the same `App` process label.
- This slice does not change root `App`, AppShell, SettingsPanel, backend IPC,
  settings persistence, runtime-pack Rust behavior, provider selection, task
  checkpointing, DB migrations, TaskEvents, or real runtime side effects.

### Phase 4G Verification Notes

- Focused StartupSplash verification passed:
  `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx`
  completed with 1 file / 8 tests passed.
- UI build verification passed:
  `cd ui && npm run build` completed successfully, with the existing Vite
  dynamic-import/chunk-size warnings only.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with 32 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with 44 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with 6 passed. Existing unrelated Rust warnings remained.
- Phase 4G linked ignored local `ui/node_modules`, `src-tauri/pyembed`,
  `src-tauri/bunembed`, and `src-tauri/gbrain-source` resources from the
  primary worktree for local verification only.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- Whitespace checks passed:
  `git diff --check -- <changed-files>` and `git diff --cached --check`
  produced no output.
- Staged GitNexus detect for the Phase 4G worktree reported LOW risk:
  4 changed files, 21 changed symbols, and no affected execution flows.
- Phase 4G Startup Doctor settings deep link was merged through PR #433 as
  `5dd0745c Merge pull request #433 from novolei/codex/browser-runtime-phase4g-startup-doctor-deep-link`.

## Phase 4H Entry Criteria

Phase 4H can start because:

- PR #433 merged the Startup Doctor -> Browser Runtime Settings callback into
  `main` and `origin/main`;
- the Phase 4H worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4h-task-time-prompt-deep-link`;
- the branch starts from `5dd0745c`, the current `origin/main`;
- ADR Phase 4 still requires Settings deep links from task-time runtime prompts
  and error/recovery surfaces;
- `BrowserRuntimeTaskTimePrompt` is currently standalone with LOW pre-edit
  impact, making a component callback the narrowest reversible slice before
  task runtime wiring.

Recommended Phase 4H checks:

- BrowserRuntimeTaskTimePrompt renders a Browser Runtime Settings action when a
  callback is supplied;
- clicking the action calls the supplied callback exactly once;
- omitting the callback leaves the existing prompt action set unchanged;
- focused prompt tests pass;
- UI build still passes;
- default browser-runtime Rust regressions still pass.

## Phase 4H Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4h-task-time-prompt-deep-link.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4h-task-time-prompt-deep-link`
- Branch:
  `codex/browser-runtime-phase4h-task-time-prompt-deep-link`
- Scope:
  add a component-scoped task-time prompt settings deep-link affordance through
  an optional `BrowserRuntimeTaskTimePrompt` callback, without wiring task
  runtime or executing runtime actions.
- Current PR:
  #434, `feat(browser): link task-time prompt to runtime settings`.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the prompt callback/button changes, focused tests, this status file
  update, and the Phase 4H plan file.

### Phase 4H Impact Notes

- `npx gitnexus analyze` indexed the Phase 4H worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `BrowserRuntimeTaskTimePrompt` in
  `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.tsx` reported
  LOW risk, zero direct callers, and zero affected processes.
- Pre-edit GitNexus impact for `eventPreview` reported LOW risk with
  `BrowserRuntimeTaskTimePrompt` as the direct caller and zero affected
  processes.
- This slice does not change root `App`, AppShell, SettingsPanel, backend IPC,
  settings persistence, runtime-pack Rust behavior, provider selection, task
  checkpointing, DB migrations, TaskEvents, or real runtime side effects.

### Phase 4H Verification Notes

- Focused prompt verification passed:
  `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
  reported 1 file and 6 tests passed.
- UI build verification passed:
  `cd ui && npm run build` completed successfully with the existing Vite
  dynamic-import and chunk-size warnings.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  reported 32 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  reported 44 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  reported 6 passed. These commands emitted existing repository warnings only.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- Whitespace checks passed:
  `git diff --check -- <changed-files>` and `git diff --cached --check`
  produced no output.
- Staged GitNexus detect reported LOW risk: 4 changed files, 20 changed
  symbols, and 0 affected processes.

## Phase 4I Entry Criteria

Phase 4I can start because:

- PR #434 merged the task-time prompt -> Browser Runtime Settings callback into
  `main` and `origin/main`;
- the Phase 4I worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4i-error-recovery-deep-link`;
- the branch starts from `bf6a4693`, the current `origin/main`;
- ADR Phase 4 still requires Settings deep links from error/recovery surfaces;
- `ErrorMessage` already owns structured recovery actions and Settings atoms,
  so adding a frontend-only action contract is the narrowest reversible slice.

Recommended Phase 4I checks:

- a structured `open_browser_runtime_settings` recovery action opens Settings
  on the Browser Runtime tab;
- direct `SDKMessageRenderer` assistant error rendering and grouped
  `MessageGroupRenderer` assistant-turn rendering both honor the action;
- existing generic `settings` recovery actions still open Settings without
  changing the current tab;
- focused renderer tests pass;
- UI build still passes;
- default browser-runtime Rust regressions still pass.

## Phase 4I Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4i-error-recovery-deep-link.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4i-error-recovery-deep-link`
- Branch:
  `codex/browser-runtime-phase4i-error-recovery-deep-link`
- Scope:
  add a frontend-only structured error/recovery action that opens Browser
  Runtime Settings, without emitting backend events or touching task runtime.
- Current PR:
  #435, `feat(browser): link recovery errors to runtime settings`.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the action switch case, focused tests, this status file update, and
  the Phase 4I plan file.

### Phase 4I Impact Notes

- `npx gitnexus analyze` indexed the Phase 4I worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `ErrorMessage` in
  `ui/src/components/agent/SDKMessageRenderer.tsx` reported HIGH risk because
  the renderer participates in `SDKMessageRenderer`, `MessageGroupRenderer`,
  and `AssistantTurnRenderer` Agent message flows.
- Fresh reviewer sub-agent Ptolemy returned `REVIEW ACCEPTED`, limited to an
  `open_browser_runtime_settings` case that sets `settingsTabAtom` to
  `browserRuntime` and opens `settingsOpenAtom`.
- This slice does not change root `App`, AppShell, SettingsPanel, backend IPC,
  settings persistence, runtime-pack Rust behavior, provider selection, task
  checkpointing, DB migrations, TaskEvents, emitted recovery actions, or real
  runtime side effects.

### Phase 4I Verification Notes

- Focused renderer verification passed:
  `cd ui && npm test -- --run src/components/agent/SDKMessageRenderer.test.tsx`
  reported 1 file and 3 tests passed. The first run also had all assertions
  pass but surfaced an unhandled Tauri event mock rejection; the test now mocks
  `@tauri-apps/api/event.listen` and reruns cleanly.
- UI build verification passed:
  `cd ui && npm run build` completed successfully with the existing Vite
  dynamic-import and chunk-size warnings.
- Default Browser Runtime Rust regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  reported 32 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  reported 44 passed;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  reported 6 passed. These commands emitted existing repository warnings only.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  this phase because no Rust files changed.
- Whitespace checks passed:
  `git diff --check -- <changed-files>` and `git diff --cached --check`
  produced no output.
- Final staged GitNexus detect reported HIGH risk: 4 changed files, 17 changed
  symbols, and 8 affected processes, all through the expected `ErrorMessage`
  direct/grouped agent message renderer paths.
- Fresh final reviewer sub-agent Jason returned `REVIEW ACCEPTED`, confirming
  the action-only change preserves existing recovery behavior and that
  direct/grouped/generic-settings tests are sufficient for this PR to proceed
  despite the expected HIGH detect.

## Phase 4J Entry Criteria

Phase 4J can start because:

- PR #435 merged the error/recovery -> Browser Runtime Settings action into
  `main` and `origin/main`;
- the Phase 4J worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4j-paused-waiting-contract`;
- the branch starts from `ab65fab8`, the current `origin/main`;
- ADR Phase 4 still requires deferral to checkpoint tasks as
  `paused_waiting_for_browser_runtime` unless a no-browser fallback can satisfy
  the request;
- adding the backend status string and rollout conversion contract is the
  narrowest Rust-side slice before task runtime wiring.

Recommended Phase 4J checks:

- `BrowserTaskStatus::PausedWaitingForBrowserRuntime` serializes and
  deserializes as `paused_waiting_for_browser_runtime`;
- task-store status helpers roundtrip the new status;
- rollout conversion emits `TaskStarted`, `Checkpoint`, and `BoundaryYield`
  without `TaskFinished` for paused-waiting runs;
- existing paused-checkpointed, intervention, running, stopped, failed, and
  completed rollout mappings remain unchanged;
- default browser-runtime regressions still pass.

## Phase 4J Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4j-paused-waiting-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4j-paused-waiting-contract`
- Branch:
  `codex/browser-runtime-phase4j-paused-waiting-contract`
- Scope:
  add a backend browser-task status and rollout conversion contract for
  `paused_waiting_for_browser_runtime`, without wiring prompt dispatch or real
  checkpoint persistence.
- Current PR:
  PR #436.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the status variant, status string mapping, rollout conversion,
  focused tests, this status file update, and the Phase 4J plan file.

### Phase 4J Impact Notes

- `npx gitnexus analyze` indexed the Phase 4J worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `BrowserTaskStatus` reported LOW risk with zero
  affected processes.
- Pre-edit GitNexus impact for `status_to_str` and `status_from_str` reported
  LOW risk with zero affected processes.
- Pre-edit GitNexus impact for `browser_run_to_events` reported MEDIUM risk
  through expected rollout bridge/test callers and
  `emit_browser_run_into_session_dir`.
- This slice does not change agent-loop execution, task-time prompt dispatch,
  backend IPC, Settings, frontend UI, DB migrations, provider selection,
  Playwright execution, or real runtime side effects.
- GitNexus detect for the PR branch vs `origin/main` reported LOW risk: 6
  changed files, 27 changed symbols, 0 affected processes.

### Phase 4J Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  passed: 8 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::task_store`
  passed: 3 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 32 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 44 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 passed, 0 failed.
- Rust verification used ignored local runtime resource links only:
  `src-tauri/pyembed`, `src-tauri/bunembed`, and
  `src-tauri/gbrain-source`.
- `rustfmt --edition 2021 --check src-tauri/src/browser/session_state.rs
  src-tauri/src/browser/task_store.rs src-tauri/src/browser/rollout_bridge.rs
  src-tauri/src/browser/rollout_bridge_tests.rs` passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase4j-paused-waiting-contract.md
  src-tauri/src/browser/session_state.rs
  src-tauri/src/browser/task_store.rs
  src-tauri/src/browser/rollout_bridge.rs
  src-tauri/src/browser/rollout_bridge_tests.rs` passed.
- `git diff --cached --check` passed.
- `git diff -- AGENTS.md CLAUDE.md` was empty after restoring GitNexus
  statistics noise.
- GitNexus `detect_changes` with compare scope against `origin/main`
  reported LOW risk: 6 changed files, 27 changed symbols, 0 affected
  processes.

## Phase 4K Entry Criteria

Phase 4K can start because:

- PR #436 merged the backend `paused_waiting_for_browser_runtime` status and
  rollout conversion into `main` and `origin/main`;
- the Phase 4K worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4k-task-runtime-pause-wiring`;
- the branch starts from `1f8739ec`, the current `origin/main`;
- frontend task projection types and the Browser task monitor do not yet
  recognize the new paused-waiting status;
- true runtime prompt dispatch and browser-task pause creation would touch
  execution boundaries, so this PR is intentionally a frontend projection slice.

Recommended Phase 4K checks:

- `BrowserTaskStatus` frontend type unions include
  `paused_waiting_for_browser_runtime`;
- `BrowserTaskMonitor` renders the new status as a waiting/checkpoint state
  with user-readable copy;
- browser task event hooks can store the new status without dropping or
  misclassifying the run;
- no BrowserPanel layout, event subscription, IPC, runtime-pack, provider, or
  Settings behavior changes.

## Phase 4K Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4k-paused-waiting-projection.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4k-task-runtime-pause-wiring`
- Branch:
  `codex/browser-runtime-phase4k-task-runtime-pause-wiring`
- Scope:
  make `paused_waiting_for_browser_runtime` visible and typed in frontend
  browser task projection only.
- Current PR:
  PR #437.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the frontend status type additions, monitor rendering/tests, this
  status file update, and the Phase 4K plan file.

### Phase 4K Impact Notes

- `npx gitnexus analyze` indexed the Phase 4K worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `BrowserTaskStatus` type aliases in
  `ui/src/atoms/browser-atoms.ts` and `ui/src/lib/tauri-bridge.ts` could not
  resolve the type alias symbols, so those aliases are treated as unknown
  impact.
- Pre-edit GitNexus impact for `BrowserTaskMonitor` reported CRITICAL risk
  because it sits under `BrowserPanel` and reaches BrowserViewer, Preview,
  Tabs, Automation, and KaleidoscopeShell paths.
- Fresh reviewer sub-agent Aristotle returned `REVIEW ACCEPTED`, provided the
  slice stays projection-only and includes focused monitor, hook, BrowserPanel,
  and UI build verification.
- This slice does not change browser execution, prompt dispatch, backend IPC,
  runtime-pack actions, provider selection, Settings, or checkpoint writes.
- Staged GitNexus detect reported LOW risk: 7 changed files, 20 changed
  symbols, 0 affected processes.

### Phase 4K Verification Notes

- `cd ui && npm test -- --run src/components/browser/BrowserTaskMonitor.test.tsx`
  passed: 1 passed, 0 failed.
- `cd ui && npm test -- --run src/hooks/useBrowserTaskEvents.test.tsx`
  passed: 4 passed, 0 failed.
- `cd ui && npm test -- --run src/components/browser/BrowserPanel.test.tsx`
  passed: 1 passed, 0 failed, with the existing Jotai `atomFamily`
  deprecation warning.
- `cd ui && npm run build` passed, with existing Vite dynamic import and
  chunk-size warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 32 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 44 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 passed, 0 failed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase4k-paused-waiting-projection.md
  ui/src/atoms/browser-atoms.ts ui/src/lib/tauri-bridge.ts
  ui/src/components/browser/BrowserTaskMonitor.tsx
  ui/src/components/browser/BrowserTaskMonitor.test.tsx
  ui/src/hooks/useBrowserTaskEvents.test.tsx` passed.
- `git diff --cached --check` passed.
- `git diff -- AGENTS.md CLAUDE.md` was empty after restoring GitNexus
  statistics noise.
- GitNexus `detect_changes` with staged scope reported LOW risk: 7 changed
  files, 20 changed symbols, 0 affected processes.

## Phase 4L Entry Criteria

Phase 4L can start because:

- PR #437 merged the frontend paused-waiting task projection into `main` and
  `origin/main`;
- the Phase 4L worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4l-task-runtime-pause-gate`;
- the branch starts from `5bd56ba1`, the current `origin/main`;
- backend task state can represent and render
  `paused_waiting_for_browser_runtime`, but no task-time path creates it yet;
- true prompt dispatch, Settings IPC, runtime-pack execution, and no-browser
  fallback would broaden the PR, so this slice accepts only an explicit defer
  decision and pauses before browser startup.

Recommended Phase 4L checks:

- `BrowserTaskRequest` defaults runtime preparation to `ready`;
- `browser_task` accepts only `ready` or `defer`;
- `defer` creates a `PausedWaitingForBrowserRuntime` run, a
  user-intervention pause step, and a checkpoint before browser context
  creation;
- resume, harness, runtime-pack, runtime, and provider regressions keep their
  existing ready-by-default behavior;
- no DMZ, IPC, UI prompt dispatch, runtime mutation, provider promotion, or
  no-browser fallback execution changes.

## Phase 4L Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4l-task-runtime-pause-gate.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4l-task-runtime-pause-gate`
- Branch:
  `codex/browser-runtime-phase4l-task-runtime-pause-gate`
- Scope:
  explicit task-time runtime preparation defer gate for `browser_task` only.
- Current PR:
  PR #438.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the request field, parser/schema additions, pause-step helper/tests,
  harness default, this status file update, and the Phase 4L plan file.

### Phase 4L Impact Notes

- `npx gitnexus analyze` indexed the Phase 4L worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `BrowserTaskRequest` reported LOW risk:
  0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `BrowserAgentLoop.run` reported LOW risk:
  0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `BrowserTaskTool.parameters_schema` reported
  LOW risk: 0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `BrowserTaskTool.execute` reported LOW risk:
  0 direct callers, 0 affected processes, 0 affected modules.
- Fresh reviewer sub-agent Hilbert returned `REVIEW ACCEPTED` for the already
  merged PR #427 audit; this confirms the earlier Phase 4A reviewer blocker is
  closed and does not affect Phase 4L.
- `rustfmt` required normalization of the touched Rust files
  `agent_loop.rs`, `tools.rs`, and `harness/adapters/browser.rs`; GitNexus
  detect on the formatted worktree still reported LOW risk with 0 affected
  processes.
- This slice does not change prompt dispatch, Settings IPC, runtime-pack
  execution, Playwright launch, provider selection, DB migrations, root `App`,
  or `tauri_commands.rs`.

### Phase 4L Verification Notes

- Linked ignored local resources from the primary worktree for verification
  only: `src-tauri/pyembed`, `src-tauri/bunembed`, and
  `src-tauri/gbrain-source`.
- Initial focused cargo attempts before linking resources failed in the Tauri
  build script because `pyembed/python` and `gbrain-source` were absent in the
  isolated worktree. These were local worktree dependency misses, not source
  failures.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  passed after formatting: 5 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
  passed after formatting: 11 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed after formatting: 32 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed after formatting: 44 passed, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed after formatting: 6 passed, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs
  src-tauri/src/browser/tools.rs src-tauri/src/harness/adapters/browser.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase4l-task-runtime-pause-gate.md
  src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs
  src-tauri/src/harness/adapters/browser.rs` passed.
- GitNexus `detect_changes` with staged scope reported LOW risk: 5 changed
  files, 124 changed symbols, 0 affected processes.

## Phase 4M Entry Criteria

Phase 4M can start because:

- PR #438 merged the explicit backend task-runtime defer gate into `main` and
  `origin/main`;
- the Phase 4M worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4m-task-time-decision-bridge`;
- the branch starts from `a566decf`, the current `origin/main`;
- frontend prompt actions already model prepare/defer/no-browser choices but
  do not yet carry the backend-ready `runtime_preparation_decision` payload;
- true prompt dispatch, tool-call mutation, Settings IPC, runtime-pack
  execution, and no-browser fallback execution require separate design.

Recommended Phase 4M checks:

- checkpointed defer prompt actions expose
  `runtime_preparation_decision: "defer"`;
- no-browser fallback actions do not accidentally request a browser-task pause;
- existing prompt rendering remains unchanged;
- no backend, IPC, DMZ, runtime-pack execution, provider promotion, or
  no-browser fallback execution changes.

## Phase 4M Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4m-task-time-decision-bridge.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4m-task-time-decision-bridge`
- Branch:
  `codex/browser-runtime-phase4m-task-time-decision-bridge`
- Scope:
  frontend prompt-model decision metadata only.
- Current PR:
  #439 (`https://github.com/novolei/uclaw-new/pull/439`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the prompt-model metadata/test additions, this status file update, and
  the Phase 4M plan file.

### Phase 4M Impact Notes

- `npx gitnexus analyze` indexed the Phase 4M worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `deriveBrowserRuntimeTaskTimePrompt` reported
  LOW risk: 0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `BrowserRuntimeTaskTimePrompt` reported LOW
  risk: 0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `BrowserRuntimeTaskTimePromptAction` reported
  LOW risk: 1 direct importer, 0 affected processes, 0 affected modules.
- This slice does not change prompt dispatch, tool approval, Settings IPC,
  backend browser task execution, runtime-pack execution, Playwright launch,
  provider selection, DB migrations, root `App`, or `tauri_commands.rs`.

### Phase 4M Verification Notes

- Focused prompt-model verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
  returned `1 passed`, `4 passed`.
- Existing prompt UI rendering verification passed:
  `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
  returned `1 passed`, `6 passed`.
- Default Rust browser-runtime regressions passed even though Phase 4M changes
  no Rust files:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2587 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2575 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2613 filtered out`.
- No Rust files changed, so
  `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  Phase 4M.
- `git diff --check -- <changed-files>` and `git diff --cached --check`
  returned no output.
- GitNexus staged detect reported `risk_level: none`, `changed_count: 0`, and
  `affected_processes: []`; the TS/docs-only prompt-model metadata changes did
  not map to indexed execution flows.

## Phase 4N Entry Criteria

Phase 4N can start because:

- PR #439 merged the typed prompt-action defer payload into `main` and
  `origin/main`;
- the Phase 4N worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4n-task-time-tool-call-patch`;
- the branch starts from `a0dd62e5`, the current `origin/main`;
- prompt dispatch / approval hot paths still lack a single helper for applying
  task-time runtime decisions to serialized `browser_task` arguments;
- true dispatch wiring, Settings IPC, runtime-pack execution, and no-browser
  fallback execution require separate review and are not folded into this PR.

Recommended Phase 4N checks:

- checkpointed defer actions patch only `browser_task` arguments with
  `runtime_preparation_decision: "defer"`;
- no-browser fallback actions leave browser-task arguments unpatched;
- non-browser tools never receive Browser runtime decision payloads;
- helper preserves existing tool arguments and does not mutate caller-owned
  argument objects;
- no backend, IPC, DMZ, runtime-pack execution, provider promotion, or prompt
  dispatch changes.

## Phase 4N Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4n-task-time-tool-call-patch.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4n-task-time-tool-call-patch`
- Branch:
  `codex/browser-runtime-phase4n-task-time-tool-call-patch`
- Scope:
  pure frontend/model tool-call argument patch helper and focused tests.
- Current PR:
  #440 (`https://github.com/novolei/uclaw-new/pull/440`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert the helper/test additions, this status file update, and the Phase 4N
  plan file.

### Phase 4N Impact Notes

- `npx gitnexus analyze` indexed the Phase 4N worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- Pre-edit GitNexus impact for `deriveBrowserRuntimeTaskTimePrompt` reported
  LOW risk: 0 direct callers, 0 affected processes, 0 affected modules.
- Pre-edit GitNexus impact for `browserTaskRuntimeDecisionPayloadForAction`
  reported LOW risk: 0 direct callers, 0 affected processes, 0 affected
  modules.
- `BrowserRuntimeTaskTimePromptAction` was not resolvable as an indexed symbol
  in this worktree; this slice avoids changing the existing action interface.
- This slice does not change prompt dispatch, tool approval, Settings IPC,
  backend browser task execution, runtime-pack execution, Playwright launch,
  provider selection, DB migrations, root `App`, or `tauri_commands.rs`.

### Phase 4N Verification Notes

- Focused prompt-model verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
  returned `1 passed`, `5 passed`.
- Existing prompt UI rendering verification passed:
  `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
  returned `1 passed`, `6 passed`.
- Default Rust browser-runtime regressions passed even though Phase 4N changes
  no Rust files:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2587 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2575 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2613 filtered out`.
- No Rust files changed, so
  `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  Phase 4N.
- `git diff --check -- <changed-files>` returned no output.
- `git diff --cached --check` returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`, 15
  changed symbols, and `affected_processes: []`.

## Phase 4O Entry Criteria

Phase 4O can start because:

- PR #440 merged the pure task-time tool-call patch helper into `main` and
  `origin/main`;
- the Phase 4O worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4o-prompt-dispatch-review-pack`;
- the branch starts from `08a2b65f`, the current `origin/main`;
- actual prompt dispatch / approval wiring is the next Phase 4 behavior, but
  it sits near `dispatcher.rs`, browser tools, and the DMZ `agentic_loop.rs`;
- GitNexus impact for `run_agentic_loop` is HIGH, so implementation must stop
  until a reviewer plan exists.

Recommended Phase 4O checks:

- document allowed writer scopes for the next implementation PR;
- document the HIGH/DMZ stop gate for `agentic_loop.rs`;
- document low-risk candidate boundaries that do not require global loop
  changes;
- keep this phase docs-only with no backend, UI, IPC, DB, runtime-pack, or
  provider behavior changes.

## Phase 4O Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4o-prompt-dispatch-review-pack.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4o-prompt-dispatch-review-pack`
- Branch:
  `codex/browser-runtime-phase4o-prompt-dispatch-review-pack`
- Scope:
  prompt-dispatch impact notes, reviewer checklist, and next writer gates only.
- Current PR:
  PR #441 (`codex/browser-runtime-phase4o-prompt-dispatch-review-pack`).
- DMZ files:
  none edited. `agentic_loop.rs` is explicitly blocked without reviewer
  acceptance.
- Migration:
  none planned.
- Rollback:
  revert this docs-only review pack.

### Phase 4O Impact Notes

- `npx gitnexus analyze` indexed the Phase 4O worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- GitNexus impact for `ChatDelegate.execute_tool_calls` in
  `src-tauri/src/agent/dispatcher.rs` reported LOW risk: 0 direct callers, 0
  affected processes, 0 affected modules.
- GitNexus impact for `run_agentic_loop` in
  `src-tauri/src/agent/agentic_loop.rs` reported HIGH risk: 4 direct callers,
  7 impacted symbols, 0 affected processes, and direct module impact across
  Agent, Channels, and Runtime. `agentic_loop.rs` is a DMZ file.
- GitNexus context identified `BrowserTaskTool.execute` in
  `src-tauri/src/browser/tools.rs`; impact for that method reported LOW risk:
  0 direct callers, 0 affected processes, 0 affected modules.
- This slice does not change prompt dispatch, tool approval, Settings IPC,
  backend browser task execution, runtime-pack execution, Playwright launch,
  provider selection, DB migrations, root `App`, `agentic_loop.rs`, or
  `tauri_commands.rs`.

### Phase 4O Verification Notes

- Default Rust browser-runtime regressions passed even though Phase 4O changes
  docs only:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2587 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2575 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2613 filtered out`.
- No Rust files changed, so
  `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable for
  Phase 4O.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase4o-prompt-dispatch-review-pack.md`
  and `git diff --cached --check` returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 2`, 12
  changed symbols, and `affected_processes: []`.

## Phase 4P Entry Criteria

Phase 4P can start because:

- PR #441 merged the Phase 4O prompt-dispatch review pack into `main` and
  `origin/main`;
- reviewer Popper returned `REVIEW ACCEPTED` for the HIGH/DMZ gate plan;
- the Phase 4P worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4p-task-time-dispatch-patch`;
- the branch starts from `4d67f487`, the current `origin/main`;
- Phase 4O identified `dispatcher.rs` as the preferred low-risk implementation
  boundary;
- GitNexus impact for `ChatDelegate.execute_tool_calls` reported LOW risk: 0
  direct callers, 0 affected processes, and 0 affected modules.

Recommended Phase 4P checks:

- keep the implementation in `dispatcher.rs`;
- normalize only serialized `browser_task` runtime prompt patches before
  approval and execution;
- preserve explicit top-level `runtime_preparation_decision` values;
- leave non-browser tools unchanged;
- do not edit `agentic_loop.rs`, `tauri_commands.rs`, root `App`, DB
  migrations, runtime-pack execution, provider selection, or Settings IPC.

## Phase 4P Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4p-task-time-dispatch-patch.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4p-task-time-dispatch-patch`
- Branch:
  `codex/browser-runtime-phase4p-task-time-dispatch-patch`
- Scope:
  dispatcher-only Browser task prompt patch normalization.
- Current PR:
  PR #442 (`codex/browser-runtime-phase4p-task-time-dispatch-patch`).
- DMZ files:
  none edited. `agentic_loop.rs` remains blocked without fresh reviewer
  acceptance.
- Migration:
  none planned.
- Rollback:
  revert this dispatcher boundary PR; callers can still pass the flat
  `runtime_preparation_decision` argument directly to `browser_task`.

### Phase 4P Impact Notes

- `npx gitnexus analyze` indexed the Phase 4P worktree before impact analysis.
  It updated only `AGENTS.md` / `CLAUDE.md` statistics, and those noise changes
  were restored.
- GitNexus impact for `ChatDelegate.execute_tool_calls` in
  `src-tauri/src/agent/dispatcher.rs` reported LOW risk: 0 direct callers, 0
  affected processes, 0 affected modules.
- This slice changes no `agentic_loop.rs`, tool approval schema,
  `tauri_commands.rs`, Settings IPC, DB migrations, runtime-pack execution,
  Playwright launch, provider selection, or frontend UI.

### Phase 4P Verification Notes

- Focused dispatcher verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::dispatcher::browser_runtime_dispatch_patch_tests`
  returned `4 passed; 0 failed; 2619 filtered out`.
- Default Rust browser-runtime regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed; 0 failed; 2591 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed; 0 failed; 2579 filtered out`; and
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2617 filtered out`.
- `rustfmt --edition 2021 --check src-tauri/src/agent/dispatcher.rs`
  was run and failed on pre-existing whole-file formatting drift in
  `dispatcher.rs` (large import/legacy-code reflow); Phase 4P did not apply
  broad formatting churn to keep the PR reviewable.
- `git diff --check -- src-tauri/src/agent/dispatcher.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase4p-task-time-dispatch-patch.md`
  returned no output.
- `git diff --cached --check` returned no output.
- GitNexus staged detect before PR-number amend reported `risk_level: low`,
  `changed_files: 3`, 21 changed symbols, and `affected_processes: []`; final
  compare detect against `origin/main` after the amend reported
  `risk_level: low`, `changed_files: 3`, 22 changed symbols, and
  `affected_processes: []`.

## Phase 4Q Entry Criteria

Phase 4Q can start because:

- PR #442 merged the dispatcher-side Browser task request patch boundary into
  `main` and `origin/main`;
- the Phase 4Q worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4q-task-time-dispatch-effects`;
- the branch starts from `1fd68675`, the current `origin/main`;
- task-time prompt actions already expose prepare/defer/no-browser choices, but
  integration still needs one typed effect model for those choices;
- this slice is frontend-model only and avoids hot-path agent loop, IPC,
  runtime-pack execution, and provider selection.

Recommended Phase 4Q checks:

- map `prepare_now` to a runtime-prepare-requested effect;
- map checkpointed `defer` to a `browser_task` patch effect;
- map recorded defer to a record-only effect;
- map `continue_without_browser` to an explicit no-browser fallback effect;
- leave rendered prompt UI unchanged.

## Phase 4Q Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4q-task-time-dispatch-effects.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4q-task-time-dispatch-effects`
- Branch:
  `codex/browser-runtime-phase4q-task-time-dispatch-effects`
- Scope:
  pure frontend task-time dispatch-effect model and focused tests.
- Current PR:
  PR #443.
- DMZ files:
  none edited.
- Migration:
  none planned.
- Rollback:
  revert this frontend-model PR.

### Phase 4Q Impact Notes

- This slice adds exported type/function helpers and tests in the existing
  frontend prompt model. It does not modify existing function/class/method
  bodies.
- This slice changes no prompt UI rendering, live agent-loop wiring, Approval
  Modal schema, Settings IPC, backend task execution, runtime-pack execution,
  Playwright launch, provider selection, DB migrations, root `App`,
  `agentic_loop.rs`, or `tauri_commands.rs`.

### Phase 4Q Verification Notes

- Focused prompt-model verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
  returned `1 passed`, `5 passed`.
- Existing prompt UI rendering verification passed:
  `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
  returned `1 passed`, `6 passed`.
- Default Rust browser-runtime regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed`, `0 failed`, `2591 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed`, `0 failed`, `2579 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed`, `0 failed`, `2617 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable:
  Phase 4Q changes no Rust files.
- Whitespace checks passed:
  `git diff --check -- ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts ui/src/lib/browser-runtime/browser-runtime-task-prompt.test.ts docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase4q-task-time-dispatch-effects.md`
  and `git diff --cached --check` returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`,
  24 changed symbols, and `affected_processes: []`.
- Local setup caveat: this worktree used ignored symlinks for
  `ui/node_modules`, `src-tauri/pyembed`, `src-tauri/bunembed`, and
  `src-tauri/gbrain-source`; none are staged or committed.

## Phase 4R Entry Criteria

Phase 4R can start because:

- PR #443 merged the frontend task-time dispatch-effect model into `main` and
  `origin/main`;
- the Phase 4R worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4r-settings-ipc-review-pack`;
- the branch starts from `1db302be`, the current `origin/main`;
- remaining Phase 4 Settings/Doctor status and action wiring likely requires a
  Tauri IPC command and backend command registration in DMZ `tauri_commands.rs`;
- GitNexus impact for shared frontend `getSettings` is HIGH, so future writer
  work must avoid it or receive fresh reviewer acceptance.

Recommended Phase 4R checks:

- keep the phase docs-only and do not edit Tauri commands or frontend invoke
  calls;
- record a writer/reviewer plan for the next read-only status IPC slice;
- define allowed files, non-goals, rollback, and verification before any
  runtime-pack execution work;
- preserve the merged 4Q dispatch-effect tracker state.

## Phase 4R Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4r-settings-ipc-review-pack.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4r-settings-ipc-review-pack`
- Branch:
  `codex/browser-runtime-phase4r-settings-ipc-review-pack`
- Scope:
  docs-only writer/reviewer pack for Settings runtime status/action IPC.
- Current PR:
  PR #444.
- DMZ files:
  none edited; future `tauri_commands.rs` work requires writer/reviewer
  acceptance.
- Migration:
  none planned.
- Rollback:
  revert this docs-only PR.

### Phase 4R Impact Notes

- `npx gitnexus analyze` indexed the Phase 4R worktree before impact analysis.
- `ui/src/lib/tauri-bridge.ts::getSettings`: GitNexus impact HIGH, 6 direct
  callers, 2 affected processes (`App`, `GeneralSettings`), and modules Atoms,
  Settings, and Hooks.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct caller, 2 affected processes
  (`SettingsPanel`, `SettingsContent`).
- `ui/src/components/settings/SettingsPanel.tsx::SettingsPanel`: GitNexus
  impact LOW, 0 direct callers, 0 affected processes.
- `src-tauri/src/tauri_commands.rs::get_settings` was not resolved by the
  current GitNexus index, but `tauri_commands.rs` is a DMZ file; future backend
  IPC command additions must therefore use the Phase 4R reviewer plan.
- This slice changes no Rust, UI, IPC, DB migration, runtime-pack execution,
  Playwright launch, provider selection, root `App`, `agentic_loop.rs`, or
  `tauri_commands.rs`.

### Phase 4R Verification Notes

- Default Rust browser-runtime regressions passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `32 passed`, `0 failed`, `2591 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `44 passed`, `0 failed`, `2579 filtered out`;
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed`, `0 failed`, `2617 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable:
  Phase 4R changes no Rust files.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase4r-settings-ipc-review-pack.md`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 2`,
  13 changed symbols, and `affected_processes: []`.
- Local setup caveat: this worktree used ignored symlinks for
  `src-tauri/pyembed`, `src-tauri/bunembed`, and `src-tauri/gbrain-source`;
  none are staged or committed.

## Phase 4S Entry Criteria

Phase 4S can start because:

- PR #444 merged the Settings IPC review pack into `main` and `origin/main`;
- reviewer Noether returned `REVIEW ACCEPTED` for the Phase 4R gates;
- the Phase 4S worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4s-readonly-status-ipc`;
- the branch starts from `c4a31567`, the current `origin/main`;
- GitNexus impact for `src-tauri/src/main.rs::main` is LOW with 0 affected
  processes;
- GitNexus impact for
  `src-tauri/src/browser/runtime_pack.rs::inspect_runtime_pack_status` is LOW
  with 3 direct test callers and 0 affected processes;
- GitNexus impact for `ui/src/lib/tauri-bridge.ts::getSettings` remains HIGH,
  so this phase must avoid it entirely.

Recommended Phase 4S checks:

- add a dedicated read-only Browser Runtime status command;
- add a standalone frontend bridge wrapper for that command;
- keep Settings live wiring, `getSettings`, root `App`, runtime-pack execution,
  and provider selection out of scope;
- keep the command side-effect free: no downloads, deletes, repairs, launches,
  settings writes, TaskEvents, or DB writes.

## Phase 4S Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4s-readonly-status-ipc.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4s-readonly-status-ipc`
- Branch:
  `codex/browser-runtime-phase4s-readonly-status-ipc`
- Scope:
  read-only Browser Runtime status IPC command plus standalone frontend bridge.
- Current PR:
  PR #445 (`codex/browser-runtime-phase4s-readonly-status-ipc`).
- DMZ files:
  none edited; Phase 4S avoids `tauri_commands.rs`.
- Migration:
  none planned.
- Rollback:
  revert this PR; no persistent side effects.

### Phase 4S Impact Notes

- `npx gitnexus analyze` indexed the Phase 4S worktree before impact analysis.
- `src-tauri/src/main.rs::main`: GitNexus impact LOW, 0 affected processes.
- `src-tauri/src/browser/runtime_pack.rs::inspect_runtime_pack_status`:
  GitNexus impact LOW, 3 direct test callers, 0 affected processes.
- `ui/src/lib/tauri-bridge.ts::getSettings`: GitNexus impact HIGH, 6 direct
  callers, 2 affected processes (`App`, `GeneralSettings`), and modules Atoms,
  Settings, and Hooks. Phase 4S does not edit or call through this function.
- `src-tauri/src/tauri_commands.rs::get_settings` is still unresolved by
  GitNexus; Phase 4S avoids `tauri_commands.rs` entirely.
- This slice changes no Settings rendering, root `App`, shared startup
  initialization, runtime-pack execution, Playwright launch, provider
  selection, DB migrations, `agentic_loop.rs`, or `tauri_commands.rs`.

### Phase 4S Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc`
  passed: 2 tests; 0 failed; 2623 filtered out.
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`
  passed: 1 file; 1 test.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 34 tests; 0 failed; 2591 filtered out. The filter includes the 32
  runtime-pack tests plus the two Phase 4S `runtime_pack_ipc` tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 46 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2619 filtered out.
- `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw` passed with
  pre-existing warnings only.
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs`
  passed.
- A broader
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/browser/mod.rs src-tauri/src/main.rs`
  exposes pre-existing formatting drift in broad existing modules. Phase 4S
  formats only the new Rust file and intentionally avoids mechanical churn in
  `main.rs` or unrelated browser modules.
- `git diff --check -- <changed-files>` passed with no output.
- GitNexus staged detect before the first commit reported `risk_level: medium`,
  7 changed files, 17 changed symbols, and 4 affected `main`-rooted processes.
  After adding PR #445 metadata, GitNexus compare against `origin/main`
  reported `risk_level: medium`, 7 changed files, 18 changed symbols, and the
  same 4 affected `main`-rooted processes. No HIGH or CRITICAL risk was
  reported.

## Phase 4T Entry Criteria

Phase 4T can start because:

- PR #445 merged the read-only status IPC command/bridge into `main` and
  `origin/main`;
- a fresh reviewer sub-agent for PR #427 returned `GitNexus HIGH Assessment:
  ACCEPTABLE`, confirming the HIGH risk was Settings fanout rather than hidden
  runtime mutation;
- the same reviewer flagged the Browser Runtime Settings update-state and
  developer-fallback rows as ambiguous and `Important`;
- the Phase 4T worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4t-settings-status-clarity`;
- the branch starts from `88f552f3`, the current `origin/main`;
- GitNexus impact for `BrowserRuntimeSettings` is LOW, with 1 direct dependent
  (`SettingsContent`) and 2 affected Settings processes;
- GitNexus impact for `deriveBrowserRuntimeSettingsViewModel` is LOW, with 1
  direct dependent (`BrowserRuntimeSettings`) and 2 affected Settings
  processes.

Recommended Phase 4T checks:

- render update state as its own Settings row;
- render developer fallback state as its own Settings row;
- keep the existing view model and action previews intact unless tests require
  a minimal adjustment;
- do not wire live IPC, Settings persistence, runtime-pack execution, provider
  selection, root `App`, `getSettings`, or DMZ files.

## Phase 4T Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4t-settings-status-clarity.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4t-settings-status-clarity`
- Branch:
  `codex/browser-runtime-phase4t-settings-status-clarity`
- Scope:
  Browser Runtime Settings display clarity for update state and developer
  fallback state.
- Current PR:
  PR #446 (`codex/browser-runtime-phase4t-settings-status-clarity`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this PR; no persistent side effects.

### Phase 4T Impact Notes

- `npx gitnexus analyze` indexed the Phase 4T worktree before impact analysis.
  GitNexus rewrote only AGENTS/CLAUDE stats, and those noise changes were
  restored.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct dependent (`SettingsContent`), 2 affected
  Settings processes, 1 affected module (`Settings`).
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts::deriveBrowserRuntimeSettingsViewModel`:
  GitNexus impact LOW, 1 direct dependent (`BrowserRuntimeSettings`), 2
  affected Settings processes, 1 affected module (`Settings`).
- This slice changes no IPC, backend code, runtime-pack execution, provider
  selection, DB migrations, TaskEvents, root `App`, shared `getSettings`, or
  DMZ files.

### Phase 4T Verification Notes

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed: 1 file; 4 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 34 tests; 0 failed; 2591 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 46 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2619 filtered out.
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A; Phase 4T
  changes no Rust files.
- `git diff --check -- <changed-files>` passed with no output.
- GitNexus staged detect reported `risk_level: low`, 4 changed files, 13
  changed symbols, and 0 affected processes.

## Phase 4U Entry Criteria

Phase 4U can start because:

- PR #446 merged the Settings status clarity follow-up into `main` and
  `origin/main`;
- PR #445 already added the dedicated read-only `get_browser_runtime_status`
  backend command and `getBrowserRuntimeStatus` frontend bridge;
- the Phase 4U worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4u-settings-live-status-read`;
- the branch starts from `aa6838d6`, the current `origin/main`;
- GitNexus impact for `BrowserRuntimeSettings` is LOW, with 1 direct dependent
  (`SettingsContent`) and 2 affected Settings processes;
- GitNexus impact for `getBrowserRuntimeStatus` is LOW, with 0 direct
  dependents and 0 affected processes before this phase.

Recommended Phase 4U checks:

- load status through `getBrowserRuntimeStatus` only when no explicit status
  prop is supplied;
- preserve tests and preview paths by letting explicit `status` props bypass the
  live read;
- keep failures non-mutating and local to the readonly default state;
- do not wire action execution, Startup Doctor, shared `getSettings`, root
  `App`, backend changes, runtime-pack execution, provider selection, or DMZ
  files.

## Phase 4U Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4u-settings-live-status-read.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4u-settings-live-status-read`
- Branch:
  `codex/browser-runtime-phase4u-settings-live-status-read`
- Scope:
  Browser Runtime Settings consumes the dedicated read-only status bridge.
- Current PR:
  PR #447 (`codex/browser-runtime-phase4u-settings-live-status-read`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this PR; no persistent side effects.

### Phase 4U Impact Notes

- `npx gitnexus analyze` indexed the Phase 4U worktree before impact analysis.
  GitNexus rewrote only AGENTS/CLAUDE stats, and those noise changes were
  restored.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct dependent (`SettingsContent`), 2 affected
  Settings processes, 1 affected module (`Settings`).
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact LOW,
  0 direct dependents and 0 affected processes before this phase.
- This slice changes no backend code, runtime-pack execution, provider
  selection, DB migrations, TaskEvents, root `App`, shared `getSettings`,
  Startup Doctor wiring, or DMZ files.

### Phase 4U Verification Notes

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed: 1 file; 5 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 34 tests; 0 failed; 2591 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 46 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2619 filtered out.
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A; Phase 4U
  changes no Rust files.
- `git diff --check -- <changed-files>` passed with no output.
- GitNexus staged detect reported `risk_level: low`, 4 changed files, 15
  changed symbols, and 0 affected processes.

## Phase 4V Entry Criteria

Phase 4V can start because:

- PR #447 merged the Browser Runtime Settings live status read into `main` and
  `origin/main`;
- PR #445 already added the dedicated read-only `get_browser_runtime_status`
  backend command and `getBrowserRuntimeStatus` frontend bridge;
- Startup Doctor already has
  `deriveStartupDoctorViewModelFromRuntimePackStatus` as the pure adapter for
  runtime-pack reports;
- the Phase 4V worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4v-startup-doctor-live-status-read`;
- the branch starts from `ffc7b811`, the current `origin/main`;
- GitNexus impact for `StartupSplash` is LOW, with 2 direct dependents
  (`App` and `startup-splash-preview.tsx`) and 1 affected process;
- GitNexus impact for `deriveStartupDoctorViewModelFromRuntimePackStatus` is
  LOW, with 1 direct dependent and 0 affected processes;
- GitNexus impact for `getBrowserRuntimeStatus` is LOW, with 0 direct
  dependents and 0 affected processes before this phase.

Recommended Phase 4V checks:

- load status through `getBrowserRuntimeStatus` only when no explicit
  `viewModel` prop is supplied;
- preserve previews/tests by letting explicit `viewModel` props bypass the live
  read;
- keep bridge failures non-mutating and local to the readonly default startup
  state;
- do not wire runtime action execution, Settings actions, shared `getSettings`,
  backend changes, runtime-pack execution, provider selection, TaskEvents, or
  DMZ files.

## Phase 4V Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4v-startup-doctor-live-status-read.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4v-startup-doctor-live-status-read`
- Branch:
  `codex/browser-runtime-phase4v-startup-doctor-live-status-read`
- Scope:
  Startup Splash consumes the dedicated read-only runtime status bridge for
  Startup Doctor checks when no explicit preview model is supplied.
- Current PR:
  PR #448 (`codex/browser-runtime-phase4v-startup-doctor-live-status-read`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this PR; no persistent side effects.

### Phase 4V Impact Notes

- `npx gitnexus analyze` indexed the Phase 4V worktree before impact analysis.
  GitNexus rewrote only AGENTS/CLAUDE stats, and those noise changes were
  restored.
- `ui/src/components/startup/StartupSplash.tsx::StartupSplash`: GitNexus
  impact LOW, 2 direct dependents (`App` and `startup-splash-preview.tsx`), 1
  affected process, and 1 affected module (`Hooks`).
- `ui/src/lib/startup/startup-doctor.ts::deriveStartupDoctorViewModelFromRuntimePackStatus`:
  GitNexus impact LOW, 1 direct dependent, 0 affected processes, and 1 affected
  module (`Startup`).
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact LOW,
  0 direct dependents and 0 affected processes before this phase.
- This slice changes no backend code, runtime-pack execution, provider
  selection, DB migrations, TaskEvents, shared `getSettings`, Settings action
  execution, or DMZ files.

### Phase 4V Verification Notes

- `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx`
  passed: 1 file; 11 tests.
- Initial focused UI test attempt failed with `vitest: command not found`
  because the new worktree did not have `ui/node_modules`; linking the ignored
  `ui/node_modules` from the primary worktree fixed local dependency setup.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 34 tests; 0 failed; 2591 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 46 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2619 filtered out.
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A; Phase 4V
  changes no Rust files.
- `git diff --check -- <changed-files>` passed with no output.
- GitNexus staged detect reported `risk_level: low`, 4 changed files, 20
  changed symbols, and 0 affected processes before PR creation; after reviewer
  minor fixes, staged detect reported `risk_level: low`, 2 changed files, 5
  changed symbols, and 0 affected processes.

## Phase 4W Entry Criteria

Phase 4W can start because:

- PR #448 merged the Startup Doctor live status read into `main` and
  `origin/main`;
- Browser Runtime Settings already loads the dedicated read-only status bridge
  on mount and renders a `run_doctor` action preview;
- the Phase 4W worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4w-settings-run-doctor-refresh`;
- the branch starts from `5bd70bd4`, the current `origin/main`;
- GitNexus impact for `BrowserRuntimeSettings` is LOW, with 1 direct dependent
  (`SettingsContent`) and 2 affected Settings processes;
- GitNexus impact for `actionSummary` is LOW, with only the existing Browser
  Runtime settings model chain affected;
- GitNexus impact for `getBrowserRuntimeStatus` is HIGH after Phase 4V because
  it is shared by Settings and Startup Splash/root `App`. Fresh reviewer Carver
  returned `REVIEW ACCEPTED` for a narrow Settings-local read-only refresh that
  does not change the bridge implementation.

Recommended Phase 4W checks:

- clicking `运行诊断` after a successful live status read calls
  `getBrowserRuntimeStatus` exactly once beyond the mount read and updates the
  displayed status;
- explicit `status` props keep preview/test behavior deterministic and do not
  call the bridge on mount or on `run_doctor`;
- rejected manual refreshes keep the last displayed status and do not throw;
- do not wire runtime action execution, backend IPC changes, shared
  `getSettings`, Startup Splash changes, provider selection, TaskEvents, DB
  migrations, or DMZ files.

## Phase 4W Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4w-settings-run-doctor-refresh.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4w-settings-run-doctor-refresh`
- Branch:
  `codex/browser-runtime-phase4w-settings-run-doctor-refresh`
- Scope:
  Browser Runtime Settings `run_doctor` performs a read-only status refresh
  using the existing bridge.
- Current PR:
  PR #449 (`codex/browser-runtime-phase4w-settings-run-doctor-refresh`).
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this PR; no persistent side effects.

### Phase 4W Impact Notes

- `npx gitnexus analyze` indexed the Phase 4W worktree before impact analysis.
  GitNexus rewrote only AGENTS/CLAUDE stats, and those noise changes were
  restored.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct dependent (`SettingsContent`), 2 affected
  Settings processes, 1 affected module (`Settings`).
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts::actionSummary`:
  GitNexus impact LOW, 1 direct dependent and 0 affected processes.
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact HIGH,
  2 direct dependents (`BrowserRuntimeSettings` and `StartupSplash`), 3
  affected processes (`SettingsPanel`, `SettingsContent`, `App`), and 3
  affected modules (`Settings`, `Startup`, `Hooks`). Fresh reviewer Carver
  accepted this HIGH signal as expected for shared read-only bridge consumption
  as long as the bridge implementation remains unchanged.
- This slice changes no backend code, `tauri-bridge.ts`, runtime-pack
  execution, provider selection, DB migrations, TaskEvents, shared
  `getSettings`, Startup Splash code, or DMZ files.

### Phase 4W Verification Notes

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed: 1 file; 8 tests.
- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts`
  passed: 1 file; 5 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 34 tests; 0 failed; 2591 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 46 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2619 filtered out.
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A; Phase 4W
  changes no Rust files.
- `git diff --check -- <changed-files>` and `git diff --cached --check` passed
  with no output.
- GitNexus staged detect reported `risk_level: low`, 5 changed files, 18
  changed symbols, and 0 affected processes.

## Phase 4X Entry Criteria

Phase 4X can start because:

- PR #449 merged the Settings read-only run-doctor refresh into `main` and
  `origin/main`;
- Browser Runtime Settings already renders local action controls and can read
  current runtime-pack status through a dedicated read-only bridge;
- the backend has a tested runtime-pack planner and dry-run executor boundary
  from Phase 2B/2C/2F;
- the Phase 4X worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4x-settings-action-dry-run-ipc`;
- the branch starts from `f24a88b4`, the current `origin/main`;
- GitNexus impact for `runtime_pack_ipc.rs`, `main`, and
  `BrowserRuntimeSettings` is LOW; GitNexus impact for
  `getBrowserRuntimeStatus` is HIGH, but this phase does not edit that shared
  bridge symbol.

Recommended Phase 4X checks:

- Settings action buttons for prepare, repair, reinstall, cleanup, rollback,
  and keep-current call a backend dry-run bridge and render visible execution
  evidence;
- `retry_when_online` remains a local preview until it has a distinct
  retry/deferred dry-run evidence contract;
- explicit `status` props keep preview/test behavior deterministic and do not
  call the dry-run bridge;
- the backend dry-run command returns `BrowserRuntimePackExecutionReport`
  without creating runtime files or invoking real runners;
- do not wire real prepare/repair/reinstall/cleanup/rollback side effects,
  provider promotion, TaskEvents, DB migrations, shared `getSettings`,
  task-time prompt dispatch, no-browser fallback execution, or DMZ files.

## Phase 4X Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase4x-settings-action-dry-run-ipc.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4x-settings-action-dry-run-ipc`
- Branch:
  `codex/browser-runtime-phase4x-settings-action-dry-run-ipc`
- Scope:
  add a dedicated no-side-effect Tauri dry-run action command, frontend bridge
  DTOs, and Settings rendering of dry-run execution report evidence.
- Current PR:
  PR #450 (`codex/browser-runtime-phase4x-settings-action-dry-run-ipc`).
- DMZ files:
  `src-tauri/src/main.rs` command registration only. This is a narrow DMZ touch
  to expose the dedicated command; it does not edit `tauri_commands.rs`,
  `agentic_loop.rs`, DB migrations, or task-loop files.
- Migration:
  none planned.
- Rollback:
  revert this PR; no runtime files, provider state, task checkpoints, settings,
  or user data are changed.

### Phase 4X Impact Notes

- `src-tauri/src/browser/runtime_pack_ipc.rs`: GitNexus file impact LOW with 0
  affected processes before adding the dry-run command.
- `src-tauri/src/main.rs::main`: GitNexus impact LOW with 0 affected processes
  before registering the command. This is a narrow DMZ command-registration
  touch and is covered by `cargo check --bin uclaw` plus reviewer verification.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct dependent (`SettingsContent`), and 2 affected
  Settings processes.
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact HIGH
  because it is shared by Settings, Startup Splash, and root `App`; Phase 4X
  adds a separate bridge method and does not edit that symbol.
- This slice changes no real runtime-pack executor side effects, provider
  selection, DB migrations, TaskEvents, shared `getSettings`, task-loop DMZ
  files, or production downloader/extractor/delete/promote paths.
- Fresh reviewer Galileo flagged three P2 issues on PR #450: stale dry-run
  evidence after failure, ambiguous `retry_when_online` prepare evidence, and
  missing DMZ documentation for `main.rs`. The PR was updated to clear stale
  evidence, keep retry as a local preview, add focused tests, and document the
  `main.rs` DMZ touch.

### Phase 4X Verification Notes

- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`
  passed: 1 file; 2 tests.
- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed before reviewer fixes: 1 file; 10 tests. After reviewer fixes, it
  passed: 1 file; 12 tests.
- Initial
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc`
  failed in the fresh worktree before compiling source because the ignored
  local runtime resources were not linked:
  `resource path bunembed/bun doesn't exist`.
- The worktree now links ignored local `src-tauri/pyembed`,
  `src-tauri/bunembed`, `src-tauri/gbrain-source`, and `ui/node_modules` from
  the primary checkout for verification only.
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs`
  passed with no output.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc`
  passed: 4 tests; 0 failed; 2623 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 36 tests; 0 failed; 2591 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 48 tests; 0 failed; 2579 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2621 filtered out.
- `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw` passed with
  existing repository warnings only.
- `git diff --check -- <changed-files>` and `git diff --cached --check` passed
  with no output.
- GitNexus staged detect reported `risk_level: medium`, 9 changed files, 36
  changed symbols, and 4 affected `main` command-registration processes; no
  HIGH or CRITICAL risk was reported.

## Phase 5A Entry Criteria

Phase 5A can start because:

- PR #450 merged Phase 4X into `main` and `origin/main`;
- the Phase 4 exit audit maps remaining real runtime/provider execution to
  ADR Phase 5+ instead of more Settings UX slices;
- Phase 2 runtime-pack status/planner/executor contracts exist and stay
  app-managed/local-first;
- Phase 1 supervisor contracts and Phase 0 provider capability cards already
  include the disabled `browser.playwright_cli` lane;
- the Phase 5A worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5a-cli-provider-contract`;
- the branch starts from `3dbd9500`, the current `origin/main`;
- GitNexus impact for the `src-tauri/src/browser/mod.rs` export path checked
  through `BrowserService` is LOW with 0 affected processes.

Recommended Phase 5A checks:

- the provider remains disabled unless `BrowserRuntimeFeatureFlags.playwright_cli`
  is true;
- readiness is unavailable when the feature flag is off, needs setup when the
  runtime pack is not ready, and ready only when both flag and pack are ready;
- the JSON envelope supports only declarative v1 actions: `navigate`, `click`,
  `type`, `screenshot`, `extract`, and `wait`;
- addressing order is explicit: semantic locator first, uClaw DOM element id
  second, coordinate fallback last;
- raw arbitrary Playwright scripts are unrepresentable;
- do not spawn Node/Playwright, add IPC, emit TaskEvents, promote providers,
  mutate runtime packs, or touch DB migrations/DMZ files.

## Phase 5A Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5a-cli-provider-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5a-cli-provider-contract`
- Branch:
  `codex/browser-runtime-phase5a-cli-provider-contract`
- Scope:
  pure Playwright CLI provider readiness and JSON action-envelope contracts.
- Current PR:
  PR #451: `https://github.com/novolei/uclaw-new/pull/451`
- Implementation:
  adds `browser::playwright_cli` as a pure, disabled-by-default contract shell
  with typed declarative actions, addressing priority, feature-flag/runtime-pack
  readiness, and request-envelope serialization tests.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert this PR; no runtime files, provider selection, settings, task
  checkpoints, browser sessions, database rows, or user data are changed.

### Phase 5A Impact Notes

- `npx gitnexus analyze` indexed the Phase 5A worktree. It updated
  AGENTS/CLAUDE GitNexus stats, and those noise changes were restored.
- `src-tauri/src/browser/mod.rs` export path: GitNexus impact checked via
  `BrowserService` in the same file and reported LOW, 0 direct callers, 0
  affected processes, and 0 affected modules.
- This slice adds a new pure module and does not edit browser action dispatch,
  task runtime, Settings UI, IPC command registration, runtime-pack mutation,
  provider default selection, or DMZ files.

### Phase 5A Verification Notes

- Linked ignored local resources for verification only:
  `src-tauri/pyembed`, `src-tauri/bunembed`, and `src-tauri/gbrain-source`
  point at the primary worktree resources because a fresh worktree otherwise
  cannot compile the embedded-resource checks.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  passed before reviewer follow-up: 7 tests; 0 failed; 2627 filtered out.
- Fresh reviewer Mencius returned `REVIEW ACCEPTED` for PR #451 and noted a
  non-blocking test gap for `ready == true && can_run_browser_tasks == false`;
  Phase 5A added focused coverage before merge.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  passed after reviewer follow-up: 8 tests; 0 failed; 2627 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 36 tests; 0 failed; 2598 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 48 tests; 0 failed; 2586 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests; 0 failed; 2628 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`
  passed with no output.
- `rustfmt --edition 2021 --check src-tauri/src/browser/mod.rs` was attempted
  but fails on pre-existing unformatted `browser/*` legacy modules because
  rustfmt walks child modules from `mod.rs`; Phase 5A keeps `mod.rs` to a
  minimal export-only diff rather than formatting unrelated browser files.
- `git diff --check -- <changed-files>` passed with no output.
- GitNexus staged `detect_changes` reported LOW risk, 4 changed files, 14
  changed symbols, and 0 affected processes; no HIGH or CRITICAL risk was
  reported.

## Goal-Mode Docs Hygiene Entry Criteria

This sidecar can start because:

- PR #451 merged Phase 5A into `main` and `origin/main`;
- the user explicitly requested a review of `AGENTS.md`, `BEHAVIOR.md`, and
  `CONTEXT.md` for goal-mode conflicts or unreasonable design;
- the current behavior docs still had obsolete branch/plan templates and
  over-strict DMZ wording that could turn review discipline into false
  blockers during phase-pack work;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-goal-mode-docs-hygiene`;
- the branch starts from `947b3aee`, the current `origin/main`.

## Goal-Mode Docs Hygiene Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-goal-mode-docs-hygiene.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-goal-mode-docs-hygiene`
- Branch:
  `codex/browser-runtime-goal-mode-docs-hygiene`
- Scope:
  docs-only alignment of goal-mode branch naming, plan naming, primary-worktree
  sync, docs-only GitNexus expectations, high-attention policy-file wording,
  and removal of special DMZ constraints for `agentic_loop.rs` /
  `tauri_commands.rs`.
- Current PR:
  PR #452: `https://github.com/novolei/uclaw-new/pull/452`
- Non-goal:
  no Phase 5B implementation, code changes, IPC, provider promotion, runtime
  pack mutation, or DB migration.
- Rollback:
  revert this docs PR; no runtime state, browser sessions, provider selection,
  database rows, or user data are changed.

### Goal-Mode Docs Hygiene Impact Notes

- `AGENTS.md` and `BEHAVIOR.md` now describe high-attention policy files as
  reviewable boundaries rather than forbidden files.
- `agentic_loop.rs` and `tauri_commands.rs` are no longer special DMZ files;
  they follow normal code discipline: plan, GitNexus impact, narrow diff,
  focused tests, and fresh review only when broad/risky/HIGH-impact.
- Because both files are already large, new behavior should generally go into
  focused modules, leaving `agentic_loop.rs` as orchestration and
  `tauri_commands.rs` as thin IPC delegation.
- Before continuing Phase 5B, audit Phase 1 through Phase 5A for any target or
  design drift caused by the previous `agentic_loop.rs` / `tauri_commands.rs`
  constraints. Pay special attention to Phase 4O-4P prompt dispatch choices and
  Phase 4R-4X Settings/IPC choices. Also audit whether any code remained in
  dry-run lanes only because the old constraints made real `agentic_loop.rs` or
  `tauri_commands.rs` integration feel too risky. If the audit finds drift,
  create a dedicated corrective phase before resuming later Browser Runtime
  phases.
- Explicit goal-mode DRI/user authorization permits proceeding through
  high-attention or HIGH/CRITICAL gates, while tests, fresh review, unclear
  scope, and unsafe side effects remain real blockers.
- Docs-only edits that do not modify code symbols do not require symbol impact,
  but still require GitNexus detect before commit.

### Goal-Mode Docs Hygiene Verification Notes

- `rg -n "prep/codex-absorption|<M\\*-T\\*>|requires a writer/reviewer|tauri_commands\\.rs.*special DMZ|agentic_loop\\.rs.*special DMZ|once uclaw-utils-home lands" AGENTS.md BEHAVIOR.md CONTEXT.md`
  returned no stale-rule matches.
- `git diff --check -- AGENTS.md BEHAVIOR.md CONTEXT.md docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-goal-mode-docs-hygiene.md`
  passed with no output.
- `npx gitnexus analyze` indexed the docs hygiene worktree. It updated
  auto-managed GitNexus stats in `AGENTS.md` / `CLAUDE.md`; those noise changes
  were restored.
- GitNexus staged `detect_changes` reported LOW risk, 5 changed files, 31
  changed symbols, and 0 affected processes.

---

## Dry-Run Drift Audit Entry Criteria

This audit can start because:

- PR #452 merged the goal-mode docs hygiene sidecar into `main` and
  `origin/main`;
- the user explicitly asked to review Phase 1 through Phase 5A for target or
  design drift caused by previous `agentic_loop.rs` / `tauri_commands.rs`
  constraints;
- the user also asked to catch any code stuck in dry-run lanes only because
  those two large files felt too risky to modify;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-dry-run-drift-audit`;
- the branch starts from `8608b694`, the current `origin/main`.

## Dry-Run Drift Audit Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-dry-run-drift-audit.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-dry-run-drift-audit`
- Branch:
  `codex/browser-runtime-dry-run-drift-audit`
- Scope:
  docs-only Phase 1-5A audit of dry-run/contract-only lanes, especially Phase
  2C-2F runtime-pack execution, Phase 4O-4Q prompt dispatch, Phase 4R-4X
  Settings/IPC, and Phase 5A provider contracts.
- Current PR:
  #454, `https://github.com/novolei/uclaw-new/pull/454`.
- Non-goal:
  no code changes, provider execution, runtime-pack mutation, UI behavior
  change, IPC mutation, DB migration, or provider promotion.
- Rollback:
  revert this audit PR; no runtime state, browser sessions, provider selection,
  database rows, or user data are changed.

### Dry-Run Drift Audit Impact Notes

- Two fresh read-only reviewers were spawned for the audit:
  - one reviewer owns Phase 2C-2F and Phase 4R-4X runtime-pack/IPC drift
    questions;
  - one reviewer owns Phase 4O-4Q and Phase 5A prompt/provider drift
    questions.
- Preliminary local audit finds no bad architecture caused by avoiding
  `tauri_commands.rs`: Phase 4S/4X used a focused
  `browser::runtime_pack_ipc` module with narrow command registration, which
  matches the thin-file goal.
- Preliminary local audit finds no bad architecture caused by avoiding
  `agentic_loop.rs`: Phase 4P's dispatcher-level patching and
  `browser_task` request parsing already let task-time defer decisions reach
  the browser task pause gate without putting more logic into the global agent
  loop.
- Phase 5A is intentionally contract-only per ADR Phase 5. The next provider
  implementation should leave the contract lane and add supervised short-lived
  child-worker execution behind `playwright_cli`.
- The real remaining dry-run lane is the runtime-pack executor. Phase 2C-2F
  intentionally stopped at dry-run and abstract managed-runner boundaries. That
  was a safe ADR-aligned sequence, but the Browser Runtime program still lacks
  real app-managed download, verify, extract, promote, cleanup, and rollback
  adapters. This is not a `tauri_commands.rs` workaround; it is unfinished ADR
  Phase 2 work that should be closed before relying on the app-managed pack for
  real Playwright CLI execution.
- Fresh reviewer Heisenberg found an additional readiness drift: the read-only
  status IPC uses default filesystem probe options where `worker_startup_ok` and
  `real_page_probe_ok` are true by default, so a pack can report
  `ready && can_run_browser_tasks` without proving worker startup or a real page
  probe. Phase 5A consumes that readiness for `browser.playwright_cli`, so this
  must be fixed before child-worker execution can rely on the status.
- Fresh reviewer Heisenberg also found a UI/IPC mapping drift: Rust returns
  `runtime_root` and `current_pack_dir`, but the frontend live status type and
  Settings view model do not use those fields for the runtime-pack path row.
  This should be fixed before goal completion, preferably beside the readiness
  preflight.
- Fresh reviewer Banach confirmed Phase 4O-4Q are not stuck because
  `agentic_loop.rs` was avoided. The correct live boundary is the
  dispatcher/tool path: prompt patch normalization before approval/execution,
  browser task request parsing, and the browser task pause gate.
- Historical Phase 4O/4R tracker and plan rows still contain pre-PR-#452 DMZ
  vocabulary. Treat those rows as historical evidence explaining earlier
  caution, not active stop rules. Current active behavior comes from PR #452:
  `agentic_loop.rs` and `tauri_commands.rs` are normal hot-path files governed
  by GitNexus impact, narrow module design, focused tests, and fresh review
  when broad, risky, or HIGH-impact.

### Dry-Run Drift Audit Verification Notes

- Stale active-DMZ grep passed with no matches:
  `rg -n 'special DMZ constraints for agentic_loop.rs|special DMZ constraints for tauri_commands.rs|agentic_loop\.rs.*active.*DMZ|tauri_commands\.rs.*active.*DMZ' docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
  exited with no output.
- Whitespace check passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-dry-run-drift-audit.md`
  returned no output.
- Browser runtime pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `36 passed; 0 failed; 2599 filtered out`.
- Browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `48 passed; 0 failed; 2587 filtered out`.
- Browser provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2629 filtered out`.
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A; this audit
  changes no Rust files.
- `npx gitnexus analyze` indexed the audit worktree successfully
  (`36,879 nodes | 60,850 edges | 993 clusters | 300 flows`). It changed
  auto-managed `AGENTS.md` / `CLAUDE.md` stats blocks; those noise changes were
  restored before commit.
- GitNexus staged detect reported LOW risk, 2 changed docs files, 23 changed
  symbols, and 0 affected processes.

### Dry-Run Drift Audit Next Action

- Run docs/Rust/GitNexus verification, then open PR.
- Record PR/commit after verification.
- The next implementation phase should be **Phase 5B-preflight A: real
  runtime-pack step-runner and readiness-probe adapters** before the Playwright
  CLI child-worker PR.
- Follow with **Phase 5B-preflight B: Settings live runtime path mapping** if it
  is not folded into the readiness preflight by scope.

---

## Phase 5B-Preflight A Entry Criteria

Phase 5B-preflight A can start because:

- PR #453 merged the Phase 1-5A dry-run drift audit into `main` and
  `origin/main`;
- the audit found no design drift from avoiding `agentic_loop.rs` or
  `tauri_commands.rs`, but did find runtime-pack readiness can be over-reported
  and real runner/probe adapters are still missing;
- Playwright CLI child-worker execution must not rely on global npm,
  user-installed Playwright, or optimistic file-presence-only readiness;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-runtime-pack-runner`;
- the branch starts from `cd6ccc61`, the current `origin/main`.

## Phase 5B-Preflight A Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-runtime-pack-runner.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-runtime-pack-runner`
- Branch:
  `codex/browser-runtime-phase5b-preflight-runtime-pack-runner`
- Scope:
  make runtime-pack readiness strict by default and add a focused local managed
  step-runner/probe boundary for policy-gated file-backed prepare, cleanup, and
  rollback tests.
- Current PR:
  #455, `https://github.com/novolei/uclaw-new/pull/455`.
- Non-goal:
  no Settings mutation, Playwright CLI child-worker execution, provider
  promotion, DB migration, `agentic_loop.rs`, or `tauri_commands.rs` edits.
- Rollback:
  revert this PR; no runtime pack files outside test temp directories are
  touched by verification.

### Phase 5B-Preflight A Impact Notes

- `npx gitnexus analyze` indexed the Phase 5B-preflight A worktree
  (`36,879 nodes | 60,850 edges | 993 clusters | 300 flows`). It changed
  auto-managed `AGENTS.md` / `CLAUDE.md` stats blocks; those noise changes were
  restored before editing.
- GitNexus impact for `BrowserRuntimePackFilesystemProbeOptions` reported LOW
  risk, 2 direct test callers, 0 affected processes.
- GitNexus impact for `probe_runtime_pack_filesystem` reported LOW risk, 4
  direct callers, 15 impacted symbols, 0 affected processes.
- GitNexus impact for `inspect_runtime_pack_status` reported LOW risk, 4 direct
  callers, 8 impacted symbols, 0 affected processes.
- GitNexus impact for `execute_runtime_pack_plan_with_runner` reported LOW
  risk, 4 direct test callers, 0 affected processes.

### Phase 5B-Preflight A Verification Notes

- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2599 filtered out`.
- Focused browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2587 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2634 filtered out`.
- Formatting check passed for the substantive changed Rust files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/runtime_pack_tests.rs`.
- `rustfmt --edition 2021 --config skip_children=true --check src-tauri/src/browser/mod.rs`
  still reports pre-existing legacy formatting changes throughout `mod.rs`;
  this PR keeps `mod.rs` to two additive export lines to avoid unrelated
  formatting churn.
- Whitespace check passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-runtime-pack-runner.md src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/mod.rs`.
- GitNexus staged detect reported LOW risk: 6 files, 19 symbols, 0 affected
  processes.

### Phase 5B-Preflight A Next Action

- PR #454 merged. Continue to Phase 5B-preflight B Settings live path mapping
  before Playwright child-worker execution.

## Phase 5B-Preflight B Entry Criteria

Phase 5B-preflight B can start because:

- PR #454 merged the strict runtime-pack readiness defaults and local managed
  runner boundary into `main` and `origin/main`;
- the dry-run drift audit found the remaining UI/IPC drift: Rust already returns
  `runtime_root` / `current_pack_dir`, but the frontend live Settings status did
  not display that path;
- exposing path evidence is read-only and should land before Playwright CLI
  child-worker execution or provider promotion;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-settings-paths`;
- the branch starts from `6694d888`, the current `origin/main`.

## Phase 5B-Preflight B Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-settings-paths.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-preflight-settings-paths`
- Branch:
  `codex/browser-runtime-phase5b-preflight-settings-paths`
- Scope:
  frontend type/view-model/Settings display mapping for live runtime root and
  current pack directory.
- Current PR:
  PR #455 merged.
- Non-goal:
  no runtime-pack mutation, Playwright CLI child-worker execution, provider
  promotion, backend IPC change, DB migration, `agentic_loop.rs`, or
  `tauri_commands.rs` edits.
- Rollback:
  revert this PR; the backend status report remains unchanged and already
  serialized the path fields before this phase.

### Phase 5B-Preflight B Impact Notes

- `npx gitnexus analyze` indexed the Phase 5B-preflight B worktree
  (`36,936 nodes | 61,006 edges | 999 clusters | 300 flows`). Existing UI test
  scope extraction warnings appeared for automation tests unrelated to this
  phase.
- GitNexus impact for `BrowserRuntimeSettings` reported LOW risk, 2 direct
  callers, 2 affected Settings processes.
- GitNexus impact for `deriveBrowserRuntimeSettingsViewModel` reported HIGH
  risk because it feeds `BrowserRuntimeSettings` and `SettingsPanel`; this
  phase keeps behavior additive and covered by focused Settings/view-model
  tests.
- GitNexus impact for `StartupRuntimePackStatusReport` reported CRITICAL risk
  because the shared TypeScript interface has broad import fan-out; this phase
  adds optional path fields matching the Rust JSON contract and covers bridge,
  Startup Doctor, Settings, and view-model fixtures.

### Phase 5B-Preflight B Verification Notes

- Focused UI verification passed:
  `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx src/lib/tauri-bridge.browser-runtime.test.ts src/lib/startup/startup-doctor.test.ts`
  returned `4 passed`, `28 passed`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2599 filtered out`.
- Focused browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2587 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2634 filtered out`.
- Rustfmt is not applicable; no Rust files changed in this phase.
- Whitespace check passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-settings-paths.md ui/src/lib/startup/startup-doctor.ts ui/src/lib/startup/startup-doctor.test.ts ui/src/lib/browser-runtime/browser-runtime-settings.ts ui/src/lib/browser-runtime/browser-runtime-settings.test.ts ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx ui/src/lib/tauri-bridge.browser-runtime.test.ts`.
- GitNexus staged detect reported MEDIUM risk: 9 files, 24 symbols, 4 affected
  BrowserRuntimeSettings flows, no HIGH/CRITICAL.

### Phase 5B-Preflight B Next Action

- PR #455 merged. Continue to Phase 5B proper: Playwright CLI child-worker
  execution boundary.

## Phase 5B Child Worker Entry Criteria

Phase 5B child-worker execution can start because:

- PR #455 merged the live runtime root/current-pack path display into `main`
  and `origin/main`;
- Phase 5A already defined the `browser.playwright_cli` provider readiness and
  JSON request envelope contract;
- Phase 5B-preflight A added the app-managed local runtime-pack runner boundary
  and strict readiness probes, so the CLI worker can depend on
  `current_pack_dir` rather than global Node or user-installed Playwright;
- this slice is behind the existing feature/provider contract and does not
  promote `browser.playwright_cli` into task routing;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-cli-child-worker`;
- the branch starts from `681070db`, the current `origin/main`.

## Phase 5B Child Worker Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-cli-child-worker.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5b-cli-child-worker`
- Branch:
  `codex/browser-runtime-phase5b-cli-child-worker`
- Scope:
  supervised short-lived child-worker process boundary for the existing
  Playwright CLI request envelope, including app-managed Node/worker paths,
  stdin/stdout protocol, timeout kill, nonzero-exit handling, and fail-closed
  path validation.
- Current PR:
  PR #456 (`https://github.com/novolei/uclaw-new/pull/456`).
- Non-goal:
  no provider promotion, task routing, IPC, Settings/UI change, DB migration,
  `agentic_loop.rs`, `tauri_commands.rs`, global npm, or user-installed
  Playwright production path.
- Rollback:
  revert this PR; the Phase 5A provider contract and Phase 5B preflight
  runtime-pack work remain usable and the CLI provider stays unavailable until
  the feature/runtime probes pass.

### Phase 5B Child Worker Impact Notes

- `npx gitnexus analyze` indexed the Phase 5B child-worker worktree
  (`36,951 nodes | 61,021 edges | 999 clusters | 300 flows`). Existing UI test
  scope extraction warnings appeared for automation tests unrelated to this
  phase. GitNexus-updated `AGENTS.md` / `CLAUDE.md` statistic noise was
  restored before implementation.
- GitNexus impact for `PlaywrightCliRequestEnvelope` reported LOW risk, 1
  direct caller, 0 affected processes.
- GitNexus impact for `playwright_cli_provider_status` reported LOW risk, 4
  direct test callers, 0 affected processes.
- GitNexus impact for `build_playwright_cli_request_envelope` reported LOW
  risk, 2 direct test callers, 0 affected processes.
- `src-tauri/src/browser/mod.rs` receives additive exports only. Its broader
  legacy formatting drift is intentionally left untouched.

### Phase 5B Child Worker Verification Notes

- Focused Playwright CLI verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `13 passed; 0 failed; 2632 filtered out`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2604 filtered out`.
- Focused browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2592 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2639 filtered out`.
- Formatting check passed for the substantive changed Rust file:
  `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`.
- `rustfmt --edition 2021 --check src-tauri/src/browser/mod.rs` still reports
  pre-existing legacy formatting changes throughout sibling browser modules;
  this PR keeps `mod.rs` to additive exports to avoid unrelated churn.
- Whitespace check passed:
  `git diff --check -- src-tauri/src/browser/playwright_cli.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-cli-child-worker.md`.
- GitNexus staged detect reported LOW risk: 4 files, 26 symbols, 0 affected
  processes.

### Phase 5B Child Worker Next Action

- PR #456 merged into `main` and `origin/main`.
- Continue with Phase 5C worker script contract from merge commit `a5141cac`.

## Phase 5C Worker Script Entry Criteria

Phase 5C worker script execution can start because:

- PR #456 merged the supervised child-worker process boundary and fail-closed
  app-managed Node/worker path validation into `main` and `origin/main`;
- Phase 5A already defined the `browser.playwright_cli` request envelope and
  provider readiness gate;
- Phase 5B-preflight A/B ensured the runtime pack has strict readiness probes
  and visible app-managed runtime paths;
- this slice can now add the managed worker script contract without routing
  real tasks to the provider;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5c-cli-worker-script`;
- the branch starts from `a5141cac`, the current `origin/main`.

## Phase 5C Worker Script Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5c-cli-worker-script.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5c-cli-worker-script`
- Branch:
  `codex/browser-runtime-phase5c-cli-worker-script`
- Scope:
  add the app-managed Playwright worker script, preserve the existing JSON
  request/result protocol, support declarative navigate/click/type/screenshot/
  extract/wait actions, write artifact-visible screenshot output, and prove the
  script through the Rust child-worker runner using a pack-local fake Playwright
  module.
- Current PR:
  PR #457 (`https://github.com/novolei/uclaw-new/pull/457`).
- Non-goal:
  no provider promotion, task routing, IPC, Settings/UI change, DB migration,
  `agentic_loop.rs`, `tauri_commands.rs`, global npm, user-installed Playwright
  production path, or real network/browser dependency in tests.
- Rollback:
  revert this PR; the Phase 5B child-worker boundary remains in place but no
  managed worker script is shipped yet.

### Phase 5C Worker Script Impact Notes

- `npx gitnexus analyze` indexed the Phase 5C worktree
  (`36,999 nodes | 61,127 edges | 996 clusters | 300 flows`). Existing UI test
  scope extraction warnings appeared for automation tests unrelated to this
  phase. GitNexus-updated `AGENTS.md` / `CLAUDE.md` statistic noise was
  restored before implementation.
- GitNexus impact for `run_playwright_cli_child_worker` reported MEDIUM risk,
  5 direct test callers, and 0 affected processes.
- GitNexus impact for `PlaywrightCliRequestEnvelope` reported LOW risk, 2
  direct callers, 9 impacted symbols, and 0 affected processes.
- GitNexus impact for `PlaywrightCliWorkerResultEnvelope` reported LOW risk and
  0 affected processes.
- Context7 Playwright docs were checked for the worker-side API surface:
  `chromium.launch`, `page.goto`, locators, `click`, `fill`, `screenshot`,
  `textContent`, and timeout handling.

### Phase 5C Worker Script Verification Notes

- Focused Playwright CLI verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `15 passed; 0 failed; 2632 filtered out`.
- Worker script syntax check passed:
  `node --check src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2606 filtered out`.
- Focused browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2594 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2641 filtered out`.
- Formatting check passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`.
- Whitespace check passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5c-cli-worker-script.md src-tauri/src/browser/playwright_cli.rs src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`.
- GitNexus staged detect reported LOW risk: 4 files, 24 symbols, 0 affected
  processes.

### Phase 5C Worker Script Next Action

- PR #457 merged into `main` and `origin/main`.
- Continue with Phase 5D provider execution adapter from merge commit
  `96a8b5bd`.

## Phase 5D Provider Adapter Entry Criteria

Phase 5D provider execution adapter can start because:

- PR #457 merged the app-managed Playwright worker script into `main` and
  `origin/main`;
- Phase 5A already defined the feature-flagged `browser.playwright_cli`
  provider readiness and action envelope;
- Phase 5B added supervised child-worker execution with timeout/kill and
  fail-closed runtime path validation;
- Phase 5C proved the worker script contract through pack-local Node and a fake
  Playwright module;
- this slice can now add a callable adapter without routing agent tasks or
  promoting the provider;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5d-provider-execution-adapter`;
- the branch starts from `96a8b5bd`, the current `origin/main`.

## Phase 5D Provider Adapter Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5d-provider-execution-adapter.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5d-provider-execution-adapter`
- Branch:
  `codex/browser-runtime-phase5d-provider-execution-adapter`
- Scope:
  typed provider execution result/error DTOs and a callable Playwright CLI
  adapter that gates on feature flag/runtime readiness, builds the existing
  request envelope, invokes the supervised child worker, and maps worker runner
  failures into structured provider errors.
- Current PR:
  PR #458 (`https://github.com/novolei/uclaw-new/pull/458`).
- Non-goal:
  no provider promotion, task routing, IPC, Settings/UI change, DB migration,
  `agentic_loop.rs`, `tauri_commands.rs`, global npm, user-installed Playwright
  production path, or arbitrary raw-script escape hatch.
- Rollback:
  revert this PR; Phase 5A-5C contracts, runner, and worker script remain in
  place, but the CLI lane is not exposed as a callable provider adapter.

### Phase 5D Provider Adapter Impact Notes

- `npx gitnexus analyze` indexed the Phase 5D worktree
  (`37,063 nodes | 61,244 edges | 995 clusters | 300 flows`). Existing UI test
  scope extraction warnings appeared for automation tests unrelated to this
  phase. GitNexus-updated `AGENTS.md` / `CLAUDE.md` statistic noise was
  restored before implementation.
- GitNexus impact for `build_playwright_cli_request_envelope` reported LOW
  risk, 2 direct test callers, and 0 affected processes.
- GitNexus impact for `run_playwright_cli_child_worker` reported MEDIUM risk,
  7 direct test callers, and 0 affected processes.
- GitNexus impact for `PlaywrightCliWorkerResultEnvelope` reported LOW risk and
  0 affected processes.
- GitNexus impact for `PlaywrightCliWorkerError` reported LOW risk and 0
  affected processes.

### Phase 5D Provider Adapter Verification Notes

- Focused Playwright CLI verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `20 passed; 0 failed; 2632 filtered out`.
- Focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2611 filtered out`.
- Focused browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2599 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2646 filtered out`.
- Formatting check passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`.
- Whitespace check passed:
  `git diff --check -- src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5d-provider-execution-adapter.md`.
- GitNexus staged detect reported LOW risk: 3 files, 33 symbols, 0 affected
  processes.

### Phase 5D Provider Adapter Next Action

- PR #458 merged into `main` and `origin/main`.
- Continue with Phase 5E fixture gates from merge commit `78561429`.

## Phase 5E Fixture Gates Entry Criteria

Phase 5E fixture gates can start because:

- PR #458 merged the callable Playwright CLI provider adapter into `main` and
  `origin/main`;
- Phase 5A-5D now cover provider readiness, app-managed runtime pack readiness,
  supervised child-worker execution, the managed worker script, and structured
  provider execution results;
- ADR Phase 5 gate still needs explicit fixture evidence for locator fallback,
  coordinate fallback, risk screenshot policy, and remaining declarative action
  outputs before moving to Phase 6;
- this slice is tests-only for production behavior and does not route tasks or
  promote the provider;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5e-cli-fixture-gates`;
- the branch starts from `78561429`, the current `origin/main`.

## Phase 5E Fixture Gates Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5e-cli-fixture-gates.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5e-cli-fixture-gates`
- Branch:
  `codex/browser-runtime-phase5e-cli-fixture-gates`
- Scope:
  worker-script fixture coverage for semantic locator, uClaw DOM id,
  coordinate fallback, non-screenshot actions avoiding screenshot artifact refs,
  and type/extract/wait outputs.
- Current PR:
  PR #459: https://github.com/novolei/uclaw-new/pull/459
- Non-goal:
  no provider promotion, task routing, IPC, Settings/UI change, DB migration,
  `agentic_loop.rs`, `tauri_commands.rs`, global npm, user-installed Playwright
  production path, or production worker behavior change.
- Rollback:
  revert this PR; Phase 5A-5D implementation remains unchanged.

### Phase 5E Fixture Gates Impact Notes

- This slice adds tests around existing worker behavior. No production symbol
  change is planned.
- If a production symbol must change, run GitNexus impact before editing and
  stop on HIGH/CRITICAL.

### Phase 5E Fixture Gates Verification Notes

- Focused Playwright CLI verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `22 passed; 0 failed; 2632 filtered out`.
- Formatting check passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`.
- Default runtime-pack, runtime, provider, whitespace, and GitNexus staged
  checks:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
    returned `41 passed; 0 failed; 2613 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
    returned `53 passed; 0 failed; 2601 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
    returned `6 passed; 0 failed; 2648 filtered out`.
  - `git diff --check -- src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5e-cli-fixture-gates.md`
    returned no whitespace errors.
  - `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5e-cli-fixture-gates`
    reported LOW risk, 3 files, 32 symbols, 0 affected processes.

### Phase 5E Fixture Gates Next Action

- PR #459 merged into `main` and `origin/main`.
- Continue with Phase 5F action state diff from merge commit `e3e57f72`.

## Phase 5F Action State Diff Entry Criteria

Phase 5F action state diff can start because:

- PR #459 merged the remaining CLI fixture gate evidence into `main` and
  `origin/main`;
- a Phase 5 exit audit found one explicit ADR Phase 5 bullet still missing:
  action result plus DOM/state diff for stable locator clicks, `type`, and
  `wait`;
- this slice can close that gap in the managed worker output without provider
  promotion, task routing, IPC, UI, or DB changes;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`;
- the branch starts from `e3e57f72`, the current `origin/main`.

## Phase 5F Action State Diff Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase5f-action-state-diff.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`
- Branch:
  `codex/browser-runtime-phase5f-action-state-diff`
- Scope:
  compact before/after state-diff evidence for Playwright CLI click, type, and
  wait outputs.
- Current PR:
  PR #460: https://github.com/novolei/uclaw-new/pull/460
- Non-goal:
  no provider promotion, task routing, IPC, Settings/UI change, DB migration,
  browser identity/profile UX, Playwright MCP sidecar, raw page text in action
  diffs, global npm, or user-installed Playwright production path.
- Rollback:
  revert this PR; Phase 5A-5E implementation remains unchanged.

### Phase 5F Action State Diff Impact Notes

- `npx gitnexus impact runAction --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`
  reported LOW risk, 1 direct caller (`main`), 0 affected processes.
- `npx gitnexus impact write_fake_playwright_module --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`
  reported LOW risk, 1 direct caller, 0 affected processes.
- Focused test function impacts reported LOW risk with 0 affected processes.

### Phase 5F Action State Diff Verification Notes

- JavaScript syntax check passed:
  `node --check src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs`
  returned exit 0.
- Focused Playwright CLI verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `22 passed; 0 failed; 2632 filtered out`.
- Formatting check passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli.rs`.
- Default browser-runtime regression checks passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
    returned `41 passed; 0 failed; 2613 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
    returned `53 passed; 0 failed; 2601 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
    returned `6 passed; 0 failed; 2648 filtered out`.
- `git diff --check -- src-tauri/resources/browser-runtime/worker/uclaw-playwright-worker.mjs src-tauri/src/browser/playwright_cli.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase5f-action-state-diff.md`
  returned no whitespace errors.
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase5f-action-state-diff`
  reported LOW risk, 4 files, 30 symbols, 0 affected processes.

### Phase 5F Action State Diff Next Action

- PR #460 merged into `main` and `origin/main`.
- Continue with Phase 6A identity revocation contract from merge commit
  `76fea14c`.

## Phase 6A Identity Revocation Contract Entry Criteria

Phase 6A can start because:

- PR #460 closed the remaining ADR Phase 5 CLI gate and merged into `main` /
  `origin/main`;
- ADR Phase 6 requires visible, revocable browser identity before Settings
  connect/status UX and task-drain behavior;
- existing identity modules already provide storage-state import/list/resolve
  and secret-store boundaries, so a small revocation contract can be added
  without IPC, UI, or task mutation;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`;
- the branch starts from `76fea14c`, the current `origin/main`.

## Phase 6A Identity Revocation Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6a-identity-revocation-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
- Branch:
  `codex/browser-runtime-phase6a-identity-revocation-contract`
- Scope:
  revoked-visible identity metadata, secret deletion, and resolve/load blocking
  for revoked profiles.
- Current PR:
  PR #461: https://github.com/novolei/uclaw-new/pull/461
- Non-goal:
  no Settings connect UI, in-app authorization window, Tauri IPC,
  `tauri_commands.rs`, `agentic_loop.rs`, task drain, paused checkpoint,
  payment confirmation, external Chrome attach, DB migration, or provider
  promotion.
- Rollback:
  revert this PR; Phase 6 can still continue from the pre-existing
  import/list/resolve/delete identity primitives.

### Phase 6A Identity Revocation Contract Impact Notes

- `npx gitnexus impact BrowserIdentityStatus --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 0 affected processes.
- `npx gitnexus impact BrowserIdentityProfile --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 0 affected processes.
- `npx gitnexus impact import_storage_state --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 2 direct test callers, 0 affected processes.
- `npx gitnexus impact resolve_for_origin --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 1 direct test caller, 0 affected processes.
- `npx gitnexus impact load_storage_state --direction upstream --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 0 affected processes.

### Phase 6A Identity Revocation Contract Verification Notes

- Focused identity verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
  returned `7 passed; 0 failed; 2648 filtered out`.
- Formatting check passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/identity/types.rs src-tauri/src/browser/identity/profile_store.rs src-tauri/src/browser/identity/broker.rs`.
- Default browser-runtime regression checks passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
    returned `41 passed; 0 failed; 2614 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
    returned `53 passed; 0 failed; 2602 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
    returned `6 passed; 0 failed; 2649 filtered out`.
- `git diff --check -- src-tauri/src/browser/identity/types.rs src-tauri/src/browser/identity/profile_store.rs src-tauri/src/browser/identity/broker.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6a-identity-revocation-contract.md`
  returned no whitespace errors.
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
  reported LOW risk, 5 files, 29 symbols, 0 affected processes.

### Phase 6A Identity Revocation Contract Next Action

- PR #461 merged into `main` and `origin/main` as `a5fff49e`.
- Continue with Phase 6B identity IPC from merge commit `a5fff49e`.

## Phase 6B Identity IPC Contract Entry Criteria

Phase 6B can start because:

- PR #461 merged the revoked-visible backend identity contract into `main` /
  `origin/main`;
- ADR Phase 6 requires Settings to show authorized identity status, last-used
  time, active tasks, and one-click revoke;
- a safe IPC/bridge contract should exist before rendering Settings UI, so the
  UI never consumes raw backend identity metadata with `secret_handle`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6b-identity-ipc`;
- the branch starts from `a5fff49e`, the current `origin/main`.

## Phase 6B Identity IPC Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6b-identity-ipc.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6b-identity-ipc`
- Branch:
  `codex/browser-runtime-phase6b-identity-ipc`
- Scope:
  safe browser identity status/revocation IPC and frontend bridge types/calls.
- Current PR:
  PR #462: https://github.com/novolei/uclaw-new/pull/462 merged.
- Non-goal:
  no Settings UI, connect flow, authorization WebView, `tauri_commands.rs`,
  `agentic_loop.rs`, task drain, paused checkpoint, payment confirmation,
  external Chrome attach, DB migration, provider promotion, or raw
  storage-state/secret-handle exposure.
- Rollback:
  revert this PR; Phase 6A backend identity metadata and revocation behavior
  remain available to internal callers.

### Phase 6B Identity IPC Contract Impact Notes

- GitNexus impact for `src-tauri/src/main.rs::main` before command registration
  reported LOW risk, 0 direct callers, and 0 affected processes.
- `src-tauri/src/main.rs` is touched only to register additive Tauri commands.
- `src-tauri/src/browser/identity_ipc.rs` returns frontend-safe summaries that
  omit `secret_handle`; active task counts are explicitly `null` until a later
  task-drain phase owns active-task tracking.
- This slice does not emit TaskEvents, mutate tasks, launch a browser, or read
  storage-state secrets over IPC.

### Phase 6B Identity IPC Contract Verification Notes

- Focused identity IPC verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
  returned `3 passed; 0 failed; 2655 filtered out`.
- Focused identity verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
  returned `10 passed; 0 failed; 2648 filtered out`.
- Focused frontend bridge verification passed:
  `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
  returned `1 passed`, `2 passed`.
- Formatting check passed for the new Rust module:
  `rustfmt --edition 2021 --check src-tauri/src/browser/identity_ipc.rs`.
- Whole-file `rustfmt --edition 2021 --check --config skip_children=true`
  against `src-tauri/src/main.rs` and `src-tauri/src/browser/mod.rs` still
  reports pre-existing legacy formatting churn outside this PR's edited lines;
  that unrelated reformat was intentionally not accepted into this phase.
- Default browser-runtime regression checks passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
    returned `41 passed; 0 failed; 2617 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
    returned `53 passed; 0 failed; 2605 filtered out`.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
    returned `6 passed; 0 failed; 2652 filtered out`.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6b-identity-ipc.md src-tauri/src/browser/identity_ipc.rs src-tauri/src/browser/mod.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts ui/src/lib/tauri-bridge.browser-identity.test.ts`
  returned no whitespace errors.
- GitNexus staged detect after indexing the Phase 6B worktree reported MEDIUM
  risk, 7 changed files, 67 changed symbols, and 4 affected processes rooted in
  `src-tauri/src/main.rs::main` command registration:
  `Main -> PinnedSpecSortableId`, `Main -> Parent`, `Main -> Home_dir`, and
  `Main -> ProcessLock`. No HIGH/CRITICAL risk was reported.

### Phase 6B Identity IPC Contract Next Action

- PR #462 merged into `main` and `origin/main` as `e824ef07`.
- Continue with Phase 6C Settings identity status from merge commit
  `e824ef07`.

## Phase 6C Settings Identity Status Entry Criteria

Phase 6C can start because:

- PR #462 merged safe browser identity list/revoke IPC and frontend bridge
  types into `main` / `origin/main`;
- ADR Phase 6 requires Settings to show authorized identity status, last-used
  time, active tasks, and one-click revoke;
- the Settings surface can now consume safe summaries instead of raw backend
  identity metadata;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6c-settings-identity-status`;
- the branch starts from `e824ef07`, the current `origin/main`.

## Phase 6C Settings Identity Status Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6c-settings-identity-status.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6c-settings-identity-status`
- Branch:
  `codex/browser-runtime-phase6c-settings-identity-status`
- Scope:
  Settings Browser Identity status rows, safe profile list rendering, unknown
  active-task state, and user-triggered one-click revoke.
- Current PR:
  PR #463: https://github.com/novolei/uclaw-new/pull/463 merged.
- Non-goal:
  no backend changes, connect/import flow, authorization WebView,
  `tauri_commands.rs`, `agentic_loop.rs`, task drain, paused checkpoint,
  TaskEvent writes, payment confirmation, external Chrome attach, DB migration,
  provider promotion, or Space/Workspace identity scoping.
- Rollback:
  revert this PR; Phase 6B IPC and Phase 6A backend identity revocation remain
  available.

### Phase 6C Settings Identity Status Impact Notes

- GitNexus impact for `BrowserRuntimeSettings` reported LOW risk, direct caller
  `SettingsContent`, 2 affected settings processes, and 1 affected module.
- Phase 6C consumes only `BrowserIdentityStatusReport` /
  `BrowserIdentityProfileSummary` from the bridge; it does not render
  `secret_handle` or raw auth material.
- Active task count is shown as `等待任务状态` when the IPC report returns
  `null`; real active-task tracking and drain remain a later phase.

### Phase 6C Settings Identity Status Verification Notes

- Focused Settings UI verification passed:
  `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  returned `1 passed`, `14 passed`.
- Browser identity bridge regression passed:
  `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
  returned `1 passed`, `2 passed`.
- Identity backend regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
  returned `10 passed; 0 failed; 2648 filtered out`.
- Runtime-pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2617 filtered out`.
- Runtime supervisor/contract regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2605 filtered out`.
- Provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2652 filtered out`.
- Whitespace verification passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6c-settings-identity-status.md ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx`.
- GitNexus staged detect reported MEDIUM risk, `changed_files: 4`,
  `changed_count: 25`, `affected_count: 4`, and 4 affected
  `BrowserRuntimeSettings` Settings render processes; no HIGH/CRITICAL risk.

### Phase 6C Settings Identity Status Next Action

- PR #463 merged into `main` and `origin/main` as `367a9361`.
- Continue with Phase 6D identity active-task drain tracker from merge commit
  `367a9361`.

## Phase 6D Identity Active-Task Drain Tracker Entry Criteria

Phase 6D can start because:

- PR #463 merged Settings identity status/revoke UI into `main` /
  `origin/main`;
- ADR Phase 6 still requires active task display, bounded revoke drain, and
  paused checkpoint behavior;
- existing `BrowserAgentLoop` already resolves/uses authorized browser identity
  profiles and already persists `PausedCheckpointed` browser task state;
- `agentic_loop.rs` and `tauri_commands.rs` are not special DMZ files, but new
  behavior should still stay in focused browser modules with only thin
  large-file wiring when the runtime path needs it;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6d-identity-drain-tracker`;
- the branch starts from `367a9361`, the current `origin/main`.

## Phase 6D Identity Active-Task Drain Tracker Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6d-identity-drain-tracker.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6d-identity-drain-tracker`
- Branch:
  `codex/browser-runtime-phase6d-identity-drain-tracker`
- Scope:
  process-local identity active-task registry, safe IPC active-task summaries,
  bounded revocation drain deadline, and browser task checkpoint at safe action
  boundaries after revoke.
- Current PR:
  PR #464, `https://github.com/novolei/uclaw-new/pull/464`.
- Non-goal:
  no authorization WebView, Settings connect/import flow, global TaskEvent DB
  emission, user-choice UI after checkpoint, isolated-profile fallback,
  reauthorize flow, payment confirmation, provider promotion, DB migration,
  external Chrome attach, or raw auth material exposure.
- Rollback:
  revert this PR; identity revoke returns to Phase 6B/6C behavior where
  revoked profiles block new use but active running tasks are not counted or
  drain-checkpointed.

### Phase 6D Identity Active-Task Drain Tracker Impact Notes

- GitNexus impact before edits reported LOW risk for `AppState` struct and
  impl, `BrowserAgentLoop` struct and impl, `BrowserTaskTool`,
  `BrowserTaskResumeTool`, `list_browser_identities`, and
  `revoke_browser_identity`: 0 direct callers and 0 affected processes for each
  resolved symbol.
- This phase deliberately audits the earlier dry-run concern: if implementation
  needs large-file integration, it should make a narrow, tested integration
  rather than creating a docs-only permission PR. The final shape keeps business
  logic in browser modules and uses `tauri_commands.rs` only for six lines of
  tool-construction wiring that passes the shared identity task registry into
  the existing browser task tools.
- GitNexus could not resolve `send_message` / `send_agent_message` as impact
  targets in `tauri_commands.rs`; this is recorded as a high-attention
  large-file integration note, not a reason to keep the behavior in a dry-run
  lane. The resolved existing symbols above all reported LOW pre-edit impact.
- Fresh reviewer Mendel found two real safety gaps in PR #464 before merge:
  revoked-identity checkpoints could be resumed without an identity guard, and
  revocation between task registration and startup auth injection could still
  allow storage state to be applied. The PR now persists `identityProfileId` /
  `identityRevoked` checkpoint metadata, blocks implicit resume of revoked
  identity checkpoints until explicit replacement auth is provided, and checks
  revocation immediately after registration before context/storage injection.

### Phase 6D Identity Active-Task Drain Tracker Verification Notes

- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
  returned `2 passed` files and `16 passed` tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
  returned `15 passed; 0 failed; 2651 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_tasks`
  returned `4 passed; 0 failed; 2662 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
  returned `4 passed; 0 failed; 2662 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  returned `8 passed; 0 failed; 2658 filtered out` after the reviewer-finding
  fixes.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `41 passed; 0 failed; 2625 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `53 passed; 0 failed; 2613 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `6 passed; 0 failed; 2660 filtered out`.
- `rustfmt --edition 2021 --check src-tauri/src/browser/identity_tasks.rs src-tauri/src/browser/identity_ipc.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs`
  passed.
- `git diff --check` passed.
- GitNexus staged `detect_changes` reported `risk_level: medium`,
  `changed_files: 13`, `changed_count: 73`, `affected_count: 1`; the only
  affected process was the expected
  `BrowserRuntimeSettings -> ListBrowserIdentities` frontend identity status
  read flow.
- After reviewer-finding fixes, GitNexus staged `detect_changes` on the
  incremental patch reported `risk_level: low`, `changed_files: 2`,
  `changed_count: 16`, `affected_count: 0`.
- Whole-file `rustfmt --edition 2021 --check` including `app.rs`,
  `browser/mod.rs`, and `tauri_commands.rs` is not clean because those legacy
  large files/modules pull in broad pre-existing formatting drift. This phase
  avoids a mechanical rewrite of unrelated code and verifies the focused
  browser modules instead.
- The Phase 6D worktree needed local ignored links to `ui/node_modules`,
  `src-tauri/pyembed`, `src-tauri/bunembed`, and
  `src-tauri/gbrain-source` from the primary worktree for focused verification;
  these links remain ignored and are not part of the PR.

### Phase 6D Identity Active-Task Drain Tracker Next Action

- Closed. PR #464 merged as `f4f8788f`; continue with Phase 6E from
  `origin/main`.

## Phase 6E Settings Active-Task Details Entry Criteria

Phase 6E can start because:

- PR #464 merged Phase 6D active identity task summaries and revoke drain
  checkpointing into `main` / `origin/main`;
- ADR Phase 6 says Settings must show active tasks, not only authorized/revoked
  identity counts;
- Settings already reads `list_browser_identities`, and the frontend bridge now
  includes `activeTasks` with safe run/session/task/status metadata;
- this slice consumes real live IPC data and does not create another dry-run
  permission lane;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6e-settings-active-task-details`;
- the branch starts from `f4f8788f`, the current `origin/main`.

## Phase 6E Settings Active-Task Details Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6e-settings-active-task-details.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6e-settings-active-task-details`
- Branch:
  `codex/browser-runtime-phase6e-settings-active-task-details`
- Scope:
  render identity active-task rows from `BrowserIdentityStatusReport.activeTasks`
  in Browser Runtime Settings, including task, status, run/session id, and drain
  deadline when present.
- Current PR:
  PR #465 `https://github.com/novolei/uclaw-new/pull/465`, merged as
  `313b7e83`.
- Non-goal:
  no authorization WebView, Settings connect/import flow, reauthorize flow,
  isolated-profile fallback, end-task UI, payment confirmation, backend IPC
  changes, provider promotion, DB migration, or raw auth material display.
- Rollback:
  revert this PR; Settings returns to active-task count-only display while the
  Phase 6D backend registry/IPC contract remains intact.

### Phase 6E Settings Active-Task Details Impact Notes

- GitNexus impact before edits reported LOW risk for `BrowserRuntimeSettings`:
  0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for `identityActiveTaskLabel`:
  one direct caller, the local `BrowserRuntimeSettings` function.

### Phase 6E Settings Active-Task Details Verification Notes

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed: 1 file, 15 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed before reviewer fix: 41 tests, 0 failed. Passed after reviewer fix:
  42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed before reviewer fix: 53 tests, 0 failed. Passed after reviewer fix:
  54 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6e-settings-active-task-details.md ui/src/components/settings/BrowserRuntimeSettings.tsx ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
  passed.
- GitNexus staged detect reported `risk_level: none`, `changed_count: 0`,
  `affected_count: 0`; no HIGH or CRITICAL risk.
- The first focused UI test attempt needed the ignored `ui/node_modules`
  symlink in this worktree; Rust regressions used ignored runtime-resource
  symlinks from the primary worktree. These local verification aids are not
  staged.

### Phase 6E Settings Active-Task Details Next Action

- Closed. PR #465 merged as `313b7e83`; continue with Phase 6F from
  `origin/main`.

## Phase 6F Identity Boundary Actions Entry Criteria

Phase 6F can start because:

- PR #465 merged Settings active-task details into `main` / `origin/main`;
- Phase 6D already blocks implicit resume of revoked identity checkpoints, but
  ADR Phase 6 also requires asking whether to switch to isolated profile,
  reauthorize, or end the task;
- `browser_task_resume` already supports explicit replacement auth through
  `auth_profile_id` / `auth_origin`;
- the remaining gap is a typed backend decision contract, not auth WebView,
  Settings recovery UI, payment confirmation, or provider promotion;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6f-identity-boundary-actions`;
- the branch starts from `313b7e83`, the current `origin/main`.

## Phase 6F Identity Boundary Actions Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6f-identity-boundary-actions.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6f-identity-boundary-actions`
- Branch:
  `codex/browser-runtime-phase6f-identity-boundary-actions`
- Scope:
  add an explicit `browser_task_resume` identity-boundary decision for
  `isolated_profile`, `reauthorize`, and `end_task`, while preserving blocked
  implicit resume of revoked checkpoints.
- Current PR:
  PR #466 `https://github.com/novolei/uclaw-new/pull/466`.
- Current commit:
  PR #466 branch tip (`feat(browser): add identity boundary resume decisions`),
  amended after reviewer fixes.
- Non-goal:
  no authorization WebView, Settings connect/import flow, Settings recovery UI,
  payment confirmation, TaskEvent projection, provider promotion, DB migration,
  hosted provider, or raw auth material display.
- Rollback:
  revert this PR; revoked identity checkpoints return to the Phase 6D behavior
  where implicit resume is blocked until explicit replacement auth is supplied.

### Phase 6F Identity Boundary Actions Impact Notes

- GitNexus impact before edits reported LOW risk for `BrowserTaskRequest`:
  0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for `BrowserAgentLoop::run`:
  0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserTaskResumeTool::parameters_schema`: 0 direct callers and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserTaskResumeTool::execute`: 0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserTaskTool::parameters_schema`: 0 direct callers and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for `BrowserTaskTool::execute`:
  0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserParityCase::to_task_request`: 0 direct callers and 0 affected
  processes.
- GitNexus did not resolve local helper
  `identity_revocation_resume_blocked_step`; it is only called from
  `BrowserAgentLoop::run` and will remain a local step-shape helper.
- Fresh reviewer Epicurus (`019e593e-a682-7550-8c8d-81b026be6be5`) reviewed
  PR #466 and found three P1 issues: context switches reused checkpoint tabs,
  syntactic auth fields could bypass revoked checkpoint blocking without a
  resolved replacement profile, and `reauthorize` resolved but did not apply
  replacement storage state on resume.
- Follow-up fix keeps `require_auth` conservative, clears inherited checkpoint
  auth for `isolated_profile`, requires a resolved replacement auth profile for
  `reauthorize`, avoids checkpoint tab reuse when switching identity context,
  and applies replacement storage state only after replacement auth resolves.
- Follow-up fresh-review attempt was explicitly authorized, but local external
  reviewer transports were unavailable: `codex exec` rejected the configured
  and fallback models for this CLI/account pairing, and `claude -p` returned a
  provider quota 403. A targeted controller review of the current PR diff found
  no remaining blockers: `isolated_profile` and resolved `reauthorize` no longer
  reuse checkpoint tabs, explicit auth fields only bypass revoked checkpoint
  blocking after `resolve_auth_profile` returns a replacement profile, and
  resolved reauthorize resumes apply replacement storage state before task
  continuation.

### Phase 6F Identity Boundary Actions Verification Notes

- Initial focused Rust verification was blocked by missing ignored
  `src-tauri/bunembed/bun`; linked `src-tauri/pyembed`, `src-tauri/bunembed`,
  and `src-tauri/gbrain-source` from the primary worktree and reran.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  passed after reviewer fixes: 14 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
  passed: 14 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 41 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 53 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs src-tauri/src/harness/adapters/browser.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6f-identity-boundary-actions.md src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs src-tauri/src/harness/adapters/browser.rs`
  passed.
- GitNexus staged detect reported `risk_level: none`, `changed_count: 0`,
  `affected_count: 0`; no HIGH or CRITICAL risk.
- PR #466 remained `CLEAN` / `MERGEABLE`, with no GitHub checks, comments, or
  reviews reported after the follow-up fix.
- Fresh reviewer Copernicus (`019e5951-30f6-7063-9126-a177b32d953e`)
  returned `REVIEW ACCEPTED` after the follow-up fix. Residual gap: no live
  browser integration test resumes a real checkpoint through isolated-profile
  and reauthorize context switches.

### Phase 6F Identity Boundary Actions Next Action

- Closed. PR #466 merged as `ad088ed1`; continue with Phase 7A from
  `origin/main`.

## Phase 7A MCP Provider Contract Entry Criteria

Phase 7A can start because:

- PR #466 merged the final Phase 6 identity-boundary backend contract to
  `main` / `origin/main`;
- ADR Phase 7 requires Playwright MCP as a second provider lane, but the full
  sidecar, artifact routing, TaskEvents, and harness gates are too broad for
  one PR;
- the safe first slice is a typed contract around provider status, sidecar
  launch specification, and uClaw-level action envelope;
- raw MCP tool exposure remains blocked by default;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7a-mcp-provider-contract`;
- the branch starts from `ad088ed1`, the current `origin/main`.

## Phase 7A MCP Provider Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7a-mcp-provider-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7a-mcp-provider-contract`
- Branch:
  `codex/browser-runtime-phase7a-mcp-provider-contract`
- Scope:
  add pure Rust Playwright MCP provider readiness, controlled sidecar spec, and
  uClaw-level request envelope contracts.
- Merged PR:
  PR #467 `https://github.com/novolei/uclaw-new/pull/467`.
- Implementation commit:
  `bec34855 feat(browser): add playwright mcp provider contract`; merge commit
  `2b1e7f77`.
- Non-goal:
  no MCP spawn, MCP manager registration, raw MCP tool exposure, Settings UI,
  Tauri IPC, TaskEvents, artifact writes, provider promotion, DB migration,
  hosted provider, or global npm/user-installed Playwright path.
- Rollback:
  revert this PR; Phase 7 returns to the existing disabled capability-card row
  with no MCP provider contract module.

### Phase 7A MCP Provider Contract Impact Notes

- GitNexus impact before edits reported LOW risk for
  `BrowserRuntimeFeatureFlags`: 0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `browser_provider_capability_card`: 1 direct test caller and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for
  `BROWSER_PROVIDER_CAPABILITY_CARDS`: 0 direct callers and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for
  `local_chromium_capabilities`: 2 direct callers, 0 affected processes; used
  only as a provider-contract comparison point.

### Phase 7A MCP Provider Contract Verification Notes

- Official Playwright MCP configuration docs were checked for the command-line
  option names used in the pure sidecar spec (`--caps`, `--user-data-dir`,
  `--storage-state`, `--timeout-action`, `--timeout-navigation`, and
  `--output-dir`).
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 7 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 41 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 53 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs`
  passed.
- Full-file `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/browser/mod.rs`
  would require unrelated formatting of pre-existing legacy `BrowserService`
  code. The Phase 7A diff keeps `mod.rs` to the module declaration and exports
  only; no unrelated `mod.rs` formatting is included.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7a-mcp-provider-contract.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp.rs`
  passed.
- The first focused Rust test attempt needed ignored `src-tauri/gbrain-source`,
  `src-tauri/pyembed`, and `src-tauri/bunembed` symlinks from the primary
  worktree. These local verification aids are not staged.
- GitNexus staged detect reported `risk_level: none`, `changed_count: 0`,
  `affected_count: 0`; no HIGH or CRITICAL risk.

### Phase 7A MCP Provider Contract Next Action

- Closed. PR #467 merged as `2b1e7f77`; continue with Phase 7B from
  `origin/main`.

## Phase 7B MCP Runtime-Pack Probe Entry Criteria

Phase 7B can start because:

- PR #467 merged the pure MCP provider contract to `main` / `origin/main`;
- ADR Phase 7 needs a Playwright MCP lane that remains app-managed and does
  not silently depend on global npm or user-installed Playwright packages;
- the app-managed runtime-pack manifest and filesystem probe already track
  Node, Playwright, worker, and Chromium evidence, making `@playwright/mcp`
  package evidence the next narrow boundary;
- Phase 7B deliberately keeps missing MCP package evidence non-blocking for
  existing Playwright CLI readiness so earlier Phase 5 behavior does not
  regress;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7b-mcp-runtime-pack-probe`;
- the branch starts from `2b1e7f77`, the current `origin/main`.

## Phase 7B MCP Runtime-Pack Probe Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7b-mcp-runtime-pack-probe.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7b-mcp-runtime-pack-probe`
- Branch:
  `codex/browser-runtime-phase7b-mcp-runtime-pack-probe`
- Scope:
  add `@playwright/mcp` version, managed package path, filesystem snapshot, and
  runtime-pack probe evidence.
- Merged PR:
  PR #468 `https://github.com/novolei/uclaw-new/pull/468`.
- Implementation commit:
  `7ea2453f feat(browser): track playwright mcp runtime pack`; merge commit
  `90fe28d7`.
- Non-goal:
  no MCP sidecar spawn, package download/install, raw MCP tool exposure, IPC,
  Settings UI, TaskEvents, artifact writes, provider routing/promotion, DB
  migration, hosted provider, or global npm/user-installed Playwright path.
- Rollback:
  revert this PR; the runtime pack stops reporting MCP package evidence while
  existing CLI readiness semantics remain unchanged.
- Dry-run drift check:
  Phase 1-5A dry-run drift was audited in PR #453 and did not identify
  `agentic_loop.rs` / `tauri_commands.rs` avoidance as a design drift source.
  Phase 7B also avoids a dry-run trap by adding real filesystem/package
  evidence now, while keeping actual MCP process execution for the next
  supervised sidecar slice.

### Phase 7B MCP Runtime-Pack Probe Impact Notes

- GitNexus impact before edits reported LOW risk for
  `BrowserRuntimePackManifest`: 4 direct references, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserRuntimePackPaths`: 0 direct references, 0 affected processes.
- GitNexus impact before edits reported MEDIUM risk for
  `BrowserRuntimePackProbe`: 6 direct references, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `probe_runtime_pack_filesystem`: 3 direct references, 0 affected processes.
- GitNexus impact before edits reported MEDIUM risk for
  `diagnose_runtime_pack`: 10 direct references, 0 affected processes.
- No HIGH or CRITICAL impact was reported before edits.

### Phase 7B MCP Runtime-Pack Probe Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 41 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 7 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 53 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/playwright_cli.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7b-mcp-runtime-pack-probe.md src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/runtime_pack_runner.rs src-tauri/src/browser/playwright_cli.rs`
  passed.
- The first focused Rust test attempt needed ignored `src-tauri/gbrain-source`,
  `src-tauri/pyembed`, and `src-tauri/bunembed` symlinks from the primary
  worktree. These local verification aids are not staged.
- Fresh reviewer Helmholtz initially returned `REVIEW BLOCKED` for PR #468
  because adding `playwright_mcp_version` as a required serde field would make
  older `browser-runtime-pack-v1` manifests deserialize as invalid JSON and
  regress existing CLI readiness. The fix makes the field deserialize with the
  pinned default and adds
  `legacy_manifest_without_mcp_version_stays_cli_ready`, covering old manifest
  plus missing MCP package while other CLI runtime evidence is ready.
- Fresh reviewer Kierkegaard returned `REVIEW ACCEPTED` after the fix. One
  non-blocking tracker hygiene note about Phase 6E counts was corrected before
  merge.
- GitNexus `npx gitnexus analyze` refreshed the index for this worktree after
  the first detect warned about a stale sibling index. Final
  `npx gitnexus detect-changes --scope staged --repo uclaw-new` reported `No
  changes detected`; no HIGH or CRITICAL risk.

### Phase 7B MCP Runtime-Pack Probe Next Action

- Closed. PR #468 merged as `90fe28d7`; continue with Phase 7C from
  `origin/main`.

## Phase 7C MCP Package Pin Correction Entry Criteria

Phase 7C can start because:

- PR #468 merged the runtime-pack MCP package evidence to `main` /
  `origin/main`;
- Phase 7D sidecar execution needs a real app-managed package pin, not a
  placeholder or Playwright-core version;
- `npm view @playwright/mcp@1.53.0` returned 404, proving the Phase 7B pin is
  not a valid stable npm package version;
- `npm view @playwright/mcp version` returned `0.0.75`, and
  `npm view @playwright/mcp@0.0.75 bin --json` returned
  `playwright-mcp: cli.js`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7c-mcp-package-pin`;
- the branch starts from `90fe28d7`, the current `origin/main`.

## Phase 7C MCP Package Pin Correction Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7c-mcp-package-pin.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7c-mcp-package-pin`
- Branch:
  `codex/browser-runtime-phase7c-mcp-package-pin`
- Scope:
  correct the runtime-pack MCP package version default and MCP sidecar package
  spec expectations from `1.53.0` to `0.0.75`.
- Current PR:
  PR #470 `https://github.com/novolei/uclaw-new/pull/470`.
- Current commit:
  PR #470 branch tip (`fix(browser): correct playwright mcp package pin`).
- Non-goal:
  no MCP sidecar spawn, package download/install, raw MCP tool exposure, IPC,
  Settings UI, TaskEvents, artifact writes, provider routing/promotion, DB
  migration, hosted provider, or global npm/user-installed Playwright path.
- Rollback:
  revert this PR; MCP returns to the prior package pin, while the provider stays
  feature-flagged and disabled by default.

### Phase 7C MCP Package Pin Correction Impact Notes

- GitNexus index was refreshed for the Phase 7C worktree before impact checks.
- GitNexus impact before edits reported LOW risk for
  `default_playwright_mcp_version`: 0 direct dependants, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `build_playwright_mcp_sidecar_spec`: 4 direct test callers, 0 affected
  processes.
- GitNexus impact for direct edits to `runtime_pack_manifest_versions_match`
  would be HIGH, so this phase does not edit that function.

### Phase 7C MCP Package Pin Correction Verification Notes

- `npm view @playwright/mcp@1.53.0 bin version --json` returned 404, proving
  the previous pin was not a valid stable package version.
- `npm view @playwright/mcp version versions --json` reported current stable
  version `0.0.75`.
- `npm view @playwright/mcp@0.0.75 bin version --json` reported version
  `0.0.75` and bin `playwright-mcp: cli.js`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 7 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 54 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/playwright_mcp.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7c-mcp-package-pin.md src-tauri/src/browser/runtime_pack.rs src-tauri/src/browser/runtime_pack_tests.rs src-tauri/src/browser/playwright_mcp.rs`
  passed.
- GitNexus staged detect reported 5 files, 21 symbols, 0 affected processes,
  `risk_level: low`; no HIGH or CRITICAL risk.

### Phase 7C MCP Package Pin Correction Next Action

- Closed. PR #470 merged as `5adc67a0`; continue with Phase 7D from
  `origin/main`.

## Phase 7D MCP Sidecar Runner Entry Criteria

Phase 7D can start because:

- PR #470 merged the valid `@playwright/mcp@0.0.75` package pin to `main` /
  `origin/main`;
- ADR Phase 7 requires Playwright MCP to run as a supervised sidecar with pinned
  package/browser versions, controlled output/profile dirs, and provider-level
  timeouts;
- official Playwright MCP docs show the server is launched as `@playwright/mcp`
  and supports isolated profile, storage state, browser, user data dir,
  capabilities, output dir, and timeout configuration;
- the earlier contract's npx-style package arg would be a production-boundary
  problem if used directly, so this slice corrects the runner path before
  protocol/client work;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7d-mcp-sidecar-runner`;
- the branch starts from `5adc67a0`, the current `origin/main`.

## Phase 7D MCP Sidecar Runner Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7d-mcp-sidecar-runner.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7d-mcp-sidecar-runner`
- Branch:
  `codex/browser-runtime-phase7d-mcp-sidecar-runner`
- Scope:
  add a supervised sidecar process runner that validates pack-local Node and
  MCP CLI paths, starts the sidecar with controlled args/env, reports a launch
  summary, kills on drop/terminate, and rejects global Node paths.
- Current PR:
  PR #471 `https://github.com/novolei/uclaw-new/pull/471`.
- Current commit:
  Phase 7D branch tip (`feat(browser): start playwright mcp sidecar`).
- Non-goal:
  no MCP protocol client, raw MCP tool exposure, provider routing/promotion,
  Settings UI/IPC, TaskEvents, DB migration, hosted provider, package
  install/download, or task dispatch.
- Rollback:
  revert this PR; MCP returns to contract/probe-only state while the feature
  flag remains off by default.

### Phase 7D MCP Sidecar Runner Impact Notes

- GitNexus index was refreshed for the Phase 7D worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `build_playwright_mcp_sidecar_spec`: 4 direct test callers, 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for
  `PlaywrightMcpSidecarSpec.args`: 1 direct test caller, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `playwright_mcp_provider_status`: 3 direct test callers, 0 affected
  processes.
- This slice explicitly checks the Phase 1-5A dry-run concern in the MCP lane:
  it does not avoid large hot-path files by staying contract-only; it adds the
  real local process boundary in focused browser modules while leaving
  `agentic_loop.rs` and `tauri_commands.rs` untouched because no task routing or
  IPC belongs in this PR.

### Phase 7D MCP Sidecar Runner Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 12 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 54 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs src-tauri/src/browser/playwright_mcp_sidecar.rs`
  passed. `src-tauri/src/browser/mod.rs` was excluded from this rustfmt check
  because rustfmt follows the module tree from `mod.rs` and reports unrelated
  historical formatting drift across `browser/`; `git diff --check` covers the
  narrow `mod.rs` export diff.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7d-mcp-sidecar-runner.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp.rs src-tauri/src/browser/playwright_mcp_sidecar.rs`
  passed.
- GitNexus staged detect reported 5 files, 17 symbols, 0 affected processes,
  `risk_level: low`; no HIGH or CRITICAL risk.
- Fresh reviewer Euler blocked PR #471 on a flaky `mcp-args.txt` timing
  fixture and stale plan rustfmt command. The follow-up removes the
  time-window file assertion and validates launched args through deterministic
  startup-exit stderr instead; the plan rustfmt command now matches the
  tracker.

### Phase 7D MCP Sidecar Runner Next Action

- Closed. PR #471 merged as `0d1ef4b1`; continue with Phase 7E from
  `origin/main`.

## Phase 7E MCP Stdio Action Boundary Entry Criteria

Phase 7E can start because:

- PR #471 merged the app-managed Playwright MCP sidecar runner to `main` /
  `origin/main`;
- ADR Phase 7 requires MCP to be used for exploratory automation,
  accessibility snapshots, locator discovery, trace capture, and ecosystem
  integrations without exposing raw tools to the model;
- Phase 7D can spawn and supervise the child process, but it does not yet speak
  MCP stdio or translate uClaw actions into fixed MCP calls;
- the user explicitly called out dry-run-lane risk, so this phase must add a
  real stdio request/response boundary against a managed child process rather
  than another contract-only shell;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7e-mcp-stdio-action-boundary`;
- the branch starts from `0d1ef4b1`, the current `origin/main`.

## Phase 7E MCP Stdio Action Boundary Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7e-mcp-stdio-action-boundary.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7e-mcp-stdio-action-boundary`
- Branch:
  `codex/browser-runtime-phase7e-mcp-stdio-action-boundary`
- Scope:
  add a supervised MCP stdio JSON-RPC action boundary that initializes the
  sidecar, sends the initialized notification, translates fixed uClaw actions
  into MCP `tools/call` requests, maps MCP JSON-RPC errors into structured Rust
  errors, and returns artifact-visible result metadata.
- Current PR:
  PR #472, merged.
- Current commit:
  `111eada1 feat(browser): add mcp stdio action boundary`; merge commit
  `d21b9fa2`.
- Non-goal:
  no provider promotion, task routing, UI, Tauri IPC, DB migration, TaskEvent
  emission, real network/site execution, generic raw MCP client, or
  `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; MCP returns to the Phase 7D supervised sidecar runner.

### Phase 7E MCP Stdio Action Boundary Impact Notes

- GitNexus index was refreshed for the Phase 7E worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `start_playwright_mcp_sidecar`: 4 direct test callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `PlaywrightMcpSidecarHandle`: 1 direct caller, 0 affected processes.
- GitNexus impact before edits reported LOW risk for `terminate`: 1 direct
  test caller, 0 affected processes.
- GitNexus impact before edits reported LOW risk for `PlaywrightMcpAction`: 0
  direct callers, 0 affected processes.
- Phase 7E keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because
  this slice is the provider-internal stdio boundary. That is not DMZ avoidance:
  no live task routing or IPC behavior belongs in this PR, and the real child
  protocol boundary is now exercised through focused browser-module tests.

### Phase 7E MCP Stdio Action Boundary Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 17 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 54 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp_sidecar.rs`
  passed. `src-tauri/src/browser/mod.rs` is intentionally excluded from
  rustfmt because rustfmt follows the module tree and rewrites unrelated
  historical browser-module formatting; `git diff --check` covers the narrow
  export diff.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7e-mcp-stdio-action-boundary.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp_sidecar.rs`
  passed.
- GitNexus staged detect reported 4 files, 32 changed symbols, 0 affected
  processes, `risk_level: low`; no HIGH or CRITICAL risk.
- Fresh reviewer Sartre blocked the first PR #472 revision on two findings:
  snapshot tool arguments should use the MCP schema's stable minimal shape, and
  JSON-RPC timeout must not leave the sidecar alive/reusable. Follow-up fixes
  switched snapshot/locator discovery calls from bounding-box output to
  `depth: 8`, added timeout poisoning that shuts down stdin, kills/waits the
  child, aborts stderr collection, and blocks handle reuse, and added focused
  tests for both behaviors. `@playwright/mcp@0.0.75` package README confirms
  `browser_click` / `browser_type` use a `target` field for exact snapshot refs
  or unique selectors, so the existing uClaw locator-to-target mapping remains.

### Phase 7E MCP Stdio Action Boundary Next Action

- Closed. PR #472 merged as `d21b9fa2`; continue with Phase 7F from
  `origin/main` to route MCP sidecar artifacts/errors through provider-level
  evidence without provider promotion.

## Phase 7F MCP Artifact/Error Routing Entry Criteria

Phase 7F can start because:

- PR #472 merged Phase 7E MCP stdio execution to `main` / `origin/main`;
- ADR Phase 7 requires MCP artifacts and errors to flow through the same
  supervisor, artifact, policy, TaskEvent, and projection model;
- Phase 7E returns sidecar-level result/error data, but not a provider-level
  result shape that later supervisor/task wiring can consume;
- Phase 7F is narrow enough to stay provider-internal and does not require
  `agentic_loop.rs`, `tauri_commands.rs`, UI, IPC, DB, or provider promotion;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7f-mcp-artifact-error-routing`;
- the branch starts from `d21b9fa2`, the current `origin/main`.

## Phase 7F MCP Artifact/Error Routing Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7f-mcp-artifact-error-routing.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7f-mcp-artifact-error-routing`
- Branch:
  `codex/browser-runtime-phase7f-mcp-artifact-error-routing`
- Scope:
  add provider-level MCP execution result/error/artifact DTOs and pure
  conversion helpers from `PlaywrightMcpSidecarActionResult` and
  `PlaywrightMcpSidecarRunnerError`.
- Merged PR:
  PR #473 (`https://github.com/novolei/uclaw-new/pull/473`), merged as
  `359b94e9`.
- Implementation commit:
  `1d2512bf feat(browser): route mcp artifact errors`.
- Non-goal:
  no provider promotion, task routing, UI, Tauri IPC, DB migration, TaskEvent
  emission, artifact file writes, global npm, raw MCP tool exposure, or
  `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; MCP returns to Phase 7E sidecar-level stdio execution.

### Phase 7F MCP Artifact/Error Routing Impact Notes

- GitNexus index was refreshed for the Phase 7F worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `PlaywrightMcpAction`: 0 direct callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `PlaywrightMcpSidecarActionResult`: 1 direct caller, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `PlaywrightMcpSidecarRunnerError`: 0 direct callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `playwright_mcp_provider_status`: 3 direct test callers, 0 affected
  processes.
- Phase 7F keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because
  the provider-level adapter is still below live task routing and IPC. This is
  not a dry-run lane: the MCP sidecar is already executable from Phase 7E, and
  this phase converts its real outputs into future supervisor evidence.

### Phase 7F MCP Artifact/Error Routing Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 22 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 54 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/playwright_mcp.rs`
  passed. `src-tauri/src/browser/mod.rs` is intentionally excluded from
  rustfmt because rustfmt follows the module tree and rewrites unrelated
  historical browser-module formatting; `git diff --check` covers the narrow
  export diff.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7f-mcp-artifact-error-routing.md src-tauri/src/browser/mod.rs src-tauri/src/browser/playwright_mcp.rs`
  passed.
- GitNexus staged detect reported 4 changed files, 26 changed symbols, 0
  affected processes, `risk_level: low`; no HIGH or CRITICAL risk.
- Fresh reviewer Locke returned `REVIEW ACCEPTED` for PR #473. Residual
  non-blocking notes: one transient sidecar stdio fixture timeout was not
  reproducible on isolated/serial/full reruns, provider DTO JSON round-trip
  coverage can be added later, and raw MCP output remains provider evidence
  only until future routing explicitly decides projection/persistence.

### Phase 7F MCP Artifact/Error Routing Next Action

- Closed. PR #473 merged as `359b94e9`; continue with Phase 7G from
  `origin/main` to encode the MCP-vs-CLI selection policy before any Phase 8
  provider promotion.

## Phase 7G MCP Selection Policy Entry Criteria

Phase 7G can start because:

- PR #473 merged Phase 7F MCP provider-level artifact/error routing to
  `main` / `origin/main`;
- ADR Phase 7 requires Playwright MCP to remain behind the Playwright CLI thin
  lane unless the task explicitly requires MCP-specific capability;
- provider capability cards already expose actions and observation modes, so a
  pure selection/ranking contract can encode this rule without task routing;
- Phase 7G is narrow enough to stay in contract/test code and does not require
  `agentic_loop.rs`, `tauri_commands.rs`, UI, IPC, DB, or provider promotion;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7g-mcp-selection-policy`;
- the branch starts from `359b94e9`, the current `origin/main`.

## Phase 7G MCP Selection Policy Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase7g-mcp-selection-policy.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase7g-mcp-selection-policy`
- Branch:
  `codex/browser-runtime-phase7g-mcp-selection-policy`
- Scope:
  add a pure `BrowserProviderSelectionRequest` /
  `BrowserProviderSelectionCandidate` contract and a
  `rank_browser_provider_candidates` helper over capability cards.
- Current PR:
  PR #474 (`https://github.com/novolei/uclaw-new/pull/474`), merged as
  `6d1704e0`.
- Implementation commit:
  `9388c666 feat(browser): add mcp selection policy`.
- Non-goal:
  no live provider selection, provider promotion, task routing, TaskEvent
  emission, UI, Tauri IPC, DB migration, runtime side effects, global npm, raw
  MCP tool exposure, or `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; Phase 7F provider result routing remains intact but the
  selection-rank contract disappears.

### Phase 7G MCP Selection Policy Impact Notes

- GitNexus index was refreshed for the Phase 7G worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderCapabilityCard`: 1 direct file touch, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `browser_provider_capability_cards`: 1 direct test caller, 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for
  `browser_provider_capability_card`: 1 direct test caller, 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for `BrowserProviderLane`: 0
  direct callers, 0 affected processes.
- Phase 7G keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because it
  is deliberately only encoding provider ranking metadata. This does not extend
  the dry-run lane: it is the guardrail needed before Phase 8 wires live
  selection/promotion.

### Phase 7G MCP Selection Policy Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  passed: 9 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 58 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 6 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp`
  passed: 22 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase7g-mcp-selection-policy.md src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
  passed.
- GitNexus staged detect reported 4 changed files, 33 changed symbols, 0
  affected processes, `risk_level: low`; no HIGH or CRITICAL risk.
- Fresh reviewer Dalton returned `REVIEW ACCEPTED` for PR #474 after checking
  scope boundaries, ranking behavior, tests, and tracker consistency.

### Phase 7G MCP Selection Policy Next Action

- Closed. PR #474 merged as `6d1704e0`; continue with Phase 8A from
  `origin/main` to add a provider route decision contract before live task
  routing.

## Phase 8A Provider Route Decision Entry Criteria

Phase 8A can start because:

- PR #474 merged Phase 7G MCP selection policy to `main` / `origin/main`;
- chromiumoxide, Playwright CLI, and Playwright MCP all have provider status or
  capability surfaces that can be compared as data;
- ADR Phase 8 requires provider choice to become a runtime policy decision
  backed by scorecards/events rather than a code fork;
- this slice can model provider selection, degradation, and rollback event
  intentions without emitting events or wiring live task routing;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8a-provider-route-decision`;
- the branch starts from `6d1704e0`, the current `origin/main`.

## Phase 8A Provider Route Decision Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8a-provider-route-decision.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8a-provider-route-decision`
- Branch:
  `codex/browser-runtime-phase8a-provider-route-decision`
- Scope:
  add pure provider route request, candidate, event-intention, and decision
  DTOs plus a `decide_browser_provider_route` helper over provider status
  snapshots.
- Current PR:
  PR #475 (`https://github.com/novolei/uclaw-new/pull/475`), merged as
  `f8a3a2cc`.
- Implementation commit:
  `5ca12d86 feat(browser): add provider route decision`.
- Non-goal:
  no live provider routing, provider promotion, agent-loop wiring, TaskEvent
  emission, UI, Tauri IPC, DB migration, runtime side effects, hosted provider
  implementation, or `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; Phase 7 ranking and provider status surfaces remain intact.

### Phase 8A Provider Route Decision Impact Notes

- GitNexus index was refreshed for the Phase 8A worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderStatus`: 0 direct callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderCapabilities`: 3 direct callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderReadiness`: 0 direct callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderSelectionRequest`: 4 direct test callers, 0 affected
  processes.
- Phase 8A keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because
  it is the route decision substrate. Phase 8B should wire a focused live route
  path once this decision is reviewable.

### Phase 8A Provider Route Decision Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 11 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  passed: 9 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 58 tests, 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 tests, 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8a-provider-route-decision.md src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
  passed.
- GitNexus staged detect reported 4 changed files, 19 changed symbols, 0
  affected processes, `risk_level: low`; no HIGH or CRITICAL risk.
- Fresh reviewer Anscombe returned `REVIEW ACCEPTED` for PR #475. Non-blocking
  follow-up: before Phase 8B live routing, tighten or document the exact
  `previous_provider_id` semantics so ordinary provider changes and rollback
  from an unavailable previous provider cannot be confused.

### Phase 8A Provider Route Decision Next Action

- Closed. PR #475 merged as `f8a3a2cc`; continue with Phase 8B from
  `origin/main` to add an in-memory provider router surface before live
  agent-loop/IPC wiring.

## Phase 8B Provider Router Surface Entry Criteria

Phase 8B can start because:

- PR #475 merged Phase 8A provider route decisions to `main` / `origin/main`;
- ADR Phase 8 requires provider choice to become data-driven and reversible;
- Phase 8A returns provider selected/degraded/rollback event intentions, but no
  owning surface keeps provider status snapshots, disabled ids, or previous
  provider state;
- this slice can add that surface without executing providers, emitting events,
  or touching `agentic_loop.rs` / `tauri_commands.rs`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8b-provider-router-surface`;
- the branch starts from `f8a3a2cc`, the current `origin/main`.

## Phase 8B Provider Router Surface Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8b-provider-router-surface.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8b-provider-router-surface`
- Branch:
  `codex/browser-runtime-phase8b-provider-router-surface`
- Scope:
  add an in-memory `BrowserProviderRouter` over provider status snapshots,
  disabled provider ids, last selected provider id, and explicit recovery
  provider id.
- Current PR:
  PR #476 (`https://github.com/novolei/uclaw-new/pull/476`), pending reviewer
  and GitHub merge-state refresh.
- Current commit:
  `feat(browser): add provider router surface`; final SHA will be recorded after
  PR publication/merge to avoid self-referential amend churn.
- Non-goal:
  no live provider action execution, provider promotion, agent-loop wiring,
  TaskEvent emission, UI, Tauri IPC, DB migration, runtime side effects, hosted
  provider implementation, or `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; Phase 8A pure route decisions remain intact.

### Phase 8B Provider Router Surface Impact Notes

- GitNexus index was refreshed for the Phase 8B worktree before impact checks.
- GitNexus impact before edits reported MEDIUM risk for
  `BrowserProviderRouteRequest`: 5 direct test callers, 0 affected processes.
- GitNexus impact before edits reported MEDIUM risk for
  `decide_browser_provider_route`: 5 direct test callers, 0 affected processes.
- GitNexus impact before edits reported LOW risk for `BrowserProviderStatus`:
  0 affected processes.
- Fresh reviewer Dirac blocked the first PR revision because a global
  `previous_provider_id` made ordinary provider changes look like rollback. The
  fix splits ordinary `last_selected_provider_id` tracking from one-shot
  `recovery_provider_id` input and adds a regression test for capability-driven
  local-to-MCP switching.
- Phase 8B keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because it
  is still building the route state surface. Phase 8C should own focused live
  routing integration and may touch those files if needed.

### Phase 8B Provider Router Surface Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed after reviewer fix: 16 passed; 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 58 passed; 0 failed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 passed; 0 failed.
- `rustfmt --edition 2021 --check src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
  passed.
- `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8b-provider-router-surface.md src-tauri/src/browser/provider.rs src-tauri/src/browser/provider_tests.rs`
  passed.
- GitNexus staged detect passed: LOW risk, 4 changed files, 17 changed symbols,
  0 affected processes.
- GitNexus staged detect after reviewer fix passed: LOW risk, 4 changed files,
  6 changed symbols, 0 affected processes.

### Phase 8B Provider Router Surface Next Action

- Closed. PR #476 merged as `814bfb40`; continue with Phase 8C from
  `origin/main` to add explicit provider harness score metadata before live
  routing/default promotion.

## Phase 8C Provider Scorecard Contract Entry Criteria

Phase 8C can start because:

- PR #476 merged Phase 8B provider router state to `main` / `origin/main`;
- ADR Phase 8 requires provider choice to be backed by scorecards, not a code
  fork or preference;
- provider capability cards already carry permissions, actions, observation
  modes, artifact policy, policy tags, harness subjects, and disable path, but
  not explicit harness score evidence;
- this slice can add scorecard metadata without live routing, event emission,
  provider promotion, UI, IPC, DB migration, or runtime side effects;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8c-provider-scorecard`;
- the branch starts from `814bfb40`, the current `origin/main`.

## Phase 8C Provider Scorecard Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8c-provider-scorecard.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8c-provider-scorecard`
- Branch:
  `codex/browser-runtime-phase8c-provider-scorecard`
- Scope:
  add static provider harness score metadata to capability cards and tests that
  every provider declares explicit scorecard evidence.
- Current PR:
  PR #477: <https://github.com/novolei/uclaw-new/pull/477>
- Current commit:
  PR branch HEAD commit `feat(browser): add provider scorecard metadata`.
- Non-goal:
  no live provider action execution, provider promotion, agent-loop wiring,
  TaskEvent emission, UI, Tauri IPC, DB migration, runtime side effects, hosted
  provider implementation, or `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; Phase 8A route decisions and Phase 8B router state remain
  intact.

### Phase 8C Provider Scorecard Contract Impact Notes

- GitNexus index was refreshed for the Phase 8C worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderCapabilityCard`: 1 direct file caller, 0 affected processes.
- GitNexus impact before edits reported MEDIUM risk for
  `browser_provider_capability_cards`: 6 direct callers, 0 affected processes.
- Phase 8C keeps `agentic_loop.rs` and `tauri_commands.rs` untouched because it
  is static provider metadata. Phase 8D should own focused live routing/event
  wiring and may touch those files if needed.

### Phase 8C Provider Scorecard Contract Verification Notes

- Runtime contract verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  returned `10 passed; 0 failed; 2708 filtered out`.
- Provider regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2702 filtered out`.
- Browser runtime regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2659 filtered out`.
- Runtime-pack regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2676 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
  and `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8c-provider-scorecard.md src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`,
  `changed_count: 24`, and `affected_processes: []`.

### Phase 8C Provider Scorecard Contract Next Action

- Closed. PR #477 merged as `42a764fb`; continue with Phase 8D from
  `origin/main` to make provider route decisions rollout-visible without
  changing provider defaults.

## Phase 8D Provider Route Events Entry Criteria

Phase 8D can start because:

- PR #477 merged Phase 8C scorecard metadata to `main` / `origin/main`;
- ADR Phase 8 requires provider selection, degradation, and rollback events;
- Phase 8A/8B already produce route event intents, but those intents are not
  yet materialized into canonical `TaskEvent`s;
- this slice can add an observable event bridge without switching live action
  execution, promoting providers, writing settings, emitting DB migrations, or
  changing UI;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8d-provider-route-events`;
- the branch starts from `42a764fb`, the current `origin/main`.

## Phase 8D Provider Route Events Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8d-provider-route-events.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8d-provider-route-events`
- Branch:
  `codex/browser-runtime-phase8d-provider-route-events`
- Scope:
  add a generic `TaskEvent::Signal` and a pure rollout bridge that turns
  provider route event intents into Browser-source signals with
  selected/degraded/rollback metadata.
- Merged PR:
  PR #478: <https://github.com/novolei/uclaw-new/pull/478>
- Merged commit:
  `f2983b77 feat(browser): emit provider route signals`; merge commit
  `23f57438`.
- Non-goal:
  no live provider execution routing, default promotion, UI, Tauri IPC,
  settings persistence, DB migration, hosted provider implementation, or
  `agentic_loop.rs` / `tauri_commands.rs` edits.
- Rollback:
  revert this PR; Phase 8A route decisions, Phase 8B router state, and Phase
  8C scorecard metadata remain intact.

### Phase 8D Provider Route Events Impact Notes

- GitNexus index was refreshed for the Phase 8D worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderRouteEventIntent`: 1 direct caller, 0 affected processes.
- GitNexus impact before edits reported MEDIUM risk for `browser_run_to_events`
  because it has 9 direct callers and 1 affected process; Phase 8D uses it only
  as nearby bridge context and does not change that function.
- GitNexus impact before edits reported LOW risk for the `TaskEvent` enum and
  impl: 0 affected processes.
- Phase 8D intentionally keeps Phase 8C fixture counts as evidence metadata
  only; live score computation/default promotion remains later Phase 8 work.

### Phase 8D Provider Route Events Verification Notes

- Runtime contracts verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml -p uclaw-runtime-contracts`
  returned `20 passed; 0 failed`.
- Provider route event bridge verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  returned `11 passed; 0 failed; 2710 filtered out`.
- Provider regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2705 filtered out`.
- Browser runtime regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2662 filtered out`.
- Runtime-pack regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2679 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check crates/uclaw-runtime-contracts/src/lib.rs crates/uclaw-runtime-contracts/src/contracts_tests.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs`
  and `git diff --check -- crates/uclaw-runtime-contracts/src/lib.rs crates/uclaw-runtime-contracts/src/contracts_tests.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8d-provider-route-events.md`
  returned no output.
- GitNexus staged detect reported `risk_level: medium`, `changed_files: 6`,
  `changed_count: 24`, and 3 affected `emit_browser_run_into_session_dir`
  processes due to nearby rollout bridge symbol mapping. The live
  `emit_browser_run_into_session_dir` function body is not changed.

### Phase 8D Provider Route Events Next Action

- Closed. PR #478 merged as `23f57438`; continue with Phase 8E from
  `origin/main` to wire route decisions into the live browser action path while
  keeping execution on the existing local Chromium provider.

## Phase 8E Live Provider Route Signals Entry Criteria

Phase 8E can start because:

- PR #478 merged Phase 8D provider route signal conversion to `main` /
  `origin/main`;
- ADR Phase 8 requires provider selection to become observable runtime
  behavior, not only static contract metadata;
- Phase 8A/8B/8C/8D already provide route decisions, router state, harness
  score metadata, and Signal conversion;
- the user explicitly asked to avoid keeping code in dry-run lanes because of
  old `agentic_loop.rs` / `tauri_commands.rs` fear;
- `BEHAVIOR.md` and `AGENTS.md` now treat `agentic_loop.rs` and
  `tauri_commands.rs` as normal hot-path files under senior engineering
  discipline, not special DMZ files;
- this slice can add live routing evidence before action execution without
  switching default providers or running CLI/MCP adapters;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8e-live-route-signals`;
- the branch starts from `23f57438`, the current `origin/main`.

## Phase 8E Live Provider Route Signals Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8e-live-route-signals.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8e-live-route-signals`
- Branch:
  `codex/browser-runtime-phase8e-live-route-signals`
- Scope:
  evaluate provider route decisions inside `BrowserAgentLoop` before each real
  browser action, emit route Signal events through the rollout writer when
  rollout is enabled, and stop safely if a non-local provider is selected before
  its execution adapter exists.
- Current PR:
  PR #479: <https://github.com/novolei/uclaw-new/pull/479>.
- Current commit:
  branch HEAD `feat(browser): emit live provider route signals`.
- Non-goal:
  no CLI/MCP execution switch, provider promotion, UI, IPC, settings
  persistence, DB migration, hosted provider implementation, global npm,
  user-installed Playwright, or raw provider tool exposure.
- Rollback:
  revert this PR; Phase 8A route decisions, Phase 8B router state, Phase 8C
  scorecard metadata, and Phase 8D signal conversion remain intact.

### Phase 8E Live Provider Route Signals Impact Notes

- GitNexus index was refreshed for the Phase 8E worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for `run` in
  `src-tauri/src/browser/agent_loop.rs`: 0 direct callers and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for `execute_with_identity` in
  `src-tauri/src/browser/action_registry.rs`: 0 affected processes. This was
  used as context; Phase 8E keeps execution in the existing local registry.
- GitNexus impact before edits reported LOW risk for
  `provider_route_decision_to_events` in
  `src-tauri/src/browser/rollout_bridge.rs`: 3 direct tests and 1 affected test
  process.
- `agent_loop.rs` is touched intentionally in this phase because the route
  decision must become live behavior. New provider-routing logic stays in small
  helpers so the loop remains orchestration rather than another large business
  logic block.
- The prior Phase 1-5A dry-run drift audit remains valid: PR #453 found no
  design drift caused by avoiding `agentic_loop.rs` / `tauri_commands.rs`.
  Phase 8E is the scheduled live-routing point, not a repair for earlier drift.

### Phase 8E Live Provider Route Signals Verification Notes

- Browser agent loop verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  returned `18 passed; 0 failed; 2708 filtered out`.
- Provider route rollout bridge verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  returned `12 passed; 0 failed; 2714 filtered out`.
- Provider router regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2710 filtered out`.
- Browser runtime regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2667 filtered out`.
- Runtime-pack regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2684 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs`
  and `git diff --check -- src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8e-live-route-signals.md`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 5`,
  `changed_count: 27`, and `affected_count: 0`; no HIGH/CRITICAL risk.

### Phase 8E Live Provider Route Signals Next Action

- Closed. PR #479 merged as `19b99593`; continue with Phase 8F from
  `origin/main` to extract the live provider execution boundary while
  preserving local Chromium behavior.

## Phase 8F Provider Execution Boundary Entry Criteria

Phase 8F can start because:

- PR #479 merged Phase 8E live provider route signals to `main` /
  `origin/main`;
- ADR Phase 8 requires browser actions to route through a `BrowserProvider`
  boundary, not remain a code fork in the task loop;
- Phase 8E proved the route decision is in the live action path, but the route
  and guard helpers still live inside `agent_loop.rs`;
- the user explicitly asked to keep `agentic_loop.rs` and `tauri_commands.rs`
  thin and not avoid hot-path work when it is the right design;
- this slice can extract a focused local provider execution boundary without
  changing behavior or enabling CLI/MCP execution;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8f-provider-execution-boundary`;
- the branch starts from `19b99593`, the current `origin/main`.

## Phase 8F Provider Execution Boundary Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8f-provider-execution-boundary.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8f-provider-execution-boundary`
- Branch:
  `codex/browser-runtime-phase8f-provider-execution-boundary`
- Scope:
  move live action-to-provider selection, local provider status, non-local
  fail-closed guard, and local Chromium action execution into a focused
  provider execution module used by `BrowserAgentLoop`.
- Current PR:
  PR #480: <https://github.com/novolei/uclaw-new/pull/480>.
- Current commit:
  branch HEAD `feat(browser): extract provider action execution boundary`.
- Non-goal:
  no CLI/MCP execution adapter wiring, provider promotion, UI, IPC, settings,
  DB migration, hosted provider, global npm, user-installed Playwright, or raw
  provider tool exposure.
- Rollback:
  revert this PR; Phase 8E's live route signal behavior can be restored by git
  history with no data migration.

### Phase 8F Provider Execution Boundary Impact Notes

- GitNexus index was refreshed for the Phase 8F worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for `run` in
  `src-tauri/src/browser/agent_loop.rs`: 0 direct callers and 0 affected
  processes.
- GitNexus impact before edits reported LOW risk for the `BrowserAgentLoop`
  struct: 0 direct callers and 0 affected processes.
- GitNexus impact before edits reported LOW risk for the Phase 8E helper
  symbols planned for extraction: `provider_selection_request_for_action`,
  `route_live_browser_action_provider`, `provider_route_blocks_local_action`,
  and `provider_route_blocked_step`.
- GitNexus did not resolve `browser/mod.rs` as a file target; Phase 8F plans
  only additive module exports there.
- This phase is intentionally a code-shape and boundary phase, not a dry-run
  stall: the boundary remains on the live action path and delegates to the
  existing local Chromium executor.
- `src-tauri/src/browser/mod.rs` is excluded from rustfmt checks because
  rustfmt follows the module tree from that file and rewrites unrelated legacy
  browser modules. Phase 8F keeps `mod.rs` to one additive module export and
  verifies it with `git diff --check`.

### Phase 8F Provider Execution Boundary Verification Notes

- Provider execution focused verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
  returned `4 passed; 0 failed; 2722 filtered out`.
- Browser agent loop verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  returned `14 passed; 0 failed; 2712 filtered out`.
- Provider route rollout bridge verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  returned `12 passed; 0 failed; 2714 filtered out`.
- Provider router regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2710 filtered out`.
- Browser runtime regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2667 filtered out`.
- Runtime-pack regression verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2684 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs`
  and `git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8f-provider-execution-boundary.md`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 6`,
  `changed_count: 22`, and `affected_count: 0`; no HIGH/CRITICAL risk.

### Phase 8F Provider Execution Boundary Next Action

- Closed. PR #480 merged as `49b71dd0`; continue with Phase 8G from
  `origin/main` to feed feature-flagged CLI/MCP candidate route inputs into
  the live provider executor while preserving local Chromium defaults.

## Phase 8G CLI/MCP Provider Candidate Route Inputs Entry Criteria

Phase 8G can start because:

- PR #480 merged Phase 8F provider execution boundary to `main` /
  `origin/main`;
- ADR Phase 8 requires chromiumoxide, Playwright CLI, and Playwright MCP to
  route through `BrowserProvider`;
- Phase 8F created the focused execution boundary, but its route inputs still
  include only local Chromium status;
- Phase 5 and Phase 7 already provide feature-flagged CLI/MCP status functions;
- this slice can make CLI/MCP visible to the same route boundary without
  switching live execution, promoting providers, or adding UI/IPC;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8g-cli-provider-candidate`;
- the branch starts from `49b71dd0`, the current `origin/main`.

## Phase 8G CLI/MCP Provider Candidate Route Inputs Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8g-cli-provider-candidate.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8g-cli-provider-candidate`
- Branch:
  `codex/browser-runtime-phase8g-cli-provider-candidate`
- Scope:
  add route options for feature flags, optional runtime-pack status, and
  disabled provider IDs, then use those options inside the live
  `BrowserProviderActionExecutor` route path.
- Implementation:
  `BrowserProviderActionRouteOptions` now feeds feature-flagged CLI/MCP status
  candidates into the same provider route boundary used by live browser actions.
  Safe defaults still omit CLI/MCP statuses; explicit route options can make a
  ready CLI candidate selectable for tests, and non-local selected routes still
  fail closed before the local Chromium action registry.
- Current PR:
  PR #481: `https://github.com/novolei/uclaw-new/pull/481`
- Current commit:
  branch HEAD `feat(browser): add provider route candidate inputs`.
- Non-goal:
  no CLI/MCP live execution from browser tasks, provider promotion, UI, IPC,
  Settings, DB migration, hosted provider, global npm, user-installed
  Playwright, or raw provider tool exposure.
- Rollback:
  revert this PR; Phase 8F local Chromium provider execution boundary remains
  intact.

### Phase 8G CLI/MCP Provider Candidate Route Inputs Impact Notes

- GitNexus index was refreshed for the Phase 8G worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderActionExecutor`, `BrowserProviderActionExecutor::new`,
  `BrowserProviderActionExecutor::route_action`, `route_live_browser_action_provider`,
  and `provider_route_blocks_local_action`.
- This phase deliberately keeps `agent_loop.rs` untouched: the live action path
  already delegates to `BrowserProviderActionExecutor`, and the new route inputs
  belong inside that focused boundary.
- Safe defaults remain local-only. CLI/MCP candidates are added only when their
  feature flags are enabled in route options and runtime readiness evidence is
  supplied where required.

### Phase 8G CLI/MCP Provider Candidate Route Inputs Verification Notes

- Focused tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
  returned `7 passed; 0 failed; 2722 filtered out`.
- Browser agent loop regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  returned `14 passed; 0 failed; 2715 filtered out`.
- Provider route signal regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  returned `12 passed; 0 failed; 2717 filtered out`.
- Provider tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2713 filtered out`.
- Runtime tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2670 filtered out`.
- Runtime-pack tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2687 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs`
  and `git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8g-cli-provider-candidate.md`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`,
  `changed_count: 28`, and `affected_count: 0`; no HIGH/CRITICAL risk.

### Phase 8G CLI/MCP Provider Candidate Route Inputs Next Action

- Closed. PR #481 merged as `e527ec45`; continue with Phase 8H from
  `origin/main` to execute explicitly selected Playwright CLI routes through
  the provider boundary while preserving local Chromium as the safe default.

## Phase 8H CLI Selected-Route Execution Entry Criteria

Phase 8H can start because:

- PR #481 merged Phase 8G route options to `main` / `origin/main`;
- ADR Phase 8 requires chromiumoxide, Playwright CLI, and Playwright MCP to
  route through `BrowserProvider`;
- Phase 5 already provides an app-managed Playwright CLI worker adapter with
  pinned runtime-pack paths, feature flags, timeout/kill behavior, and artifact
  refs;
- Phase 8G can explicitly select a ready CLI candidate, but selected non-local
  routes still stop before provider execution;
- this slice can wire selected CLI routes to that existing adapter without
  promoting CLI by default, adding UI/IPC/DB, or exposing raw scripts;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8h-cli-selected-execution`;
- the branch starts from `e527ec45`, the current `origin/main`.

## Phase 8H CLI Selected-Route Execution Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8h-cli-selected-execution.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8h-cli-selected-execution`
- Branch:
  `codex/browser-runtime-phase8h-cli-selected-execution`
- Scope:
  when a route decision explicitly selects Playwright CLI, convert supported
  browser actions into declarative CLI actions, execute through the existing
  app-managed worker adapter, and normalize the provider result back into
  `BrowserActionResult`.
- Implementation:
  selected Playwright CLI routes now execute through the Phase 5 managed worker
  adapter when route options explicitly select CLI with a ready runtime report.
  Browser actions are translated into declarative CLI actions for navigate,
  click, type, screenshot, and extract-compatible get-state requests; unsupported
  selected actions remain structurally blocked without falling through to the
  local Chromium registry. Provider results are normalized into
  `BrowserActionResult` with provider id, request id, status, artifact refs,
  output, and structured error details in `observation_json`.
- Current PR:
  PR #482: `https://github.com/novolei/uclaw-new/pull/482`
- Current commit:
  branch HEAD `feat(browser): execute selected cli provider routes`.
- Non-goal:
  no provider promotion, MCP execution, UI, IPC, Settings, DB migration, hosted
  provider, raw Playwright scripts/tools, global npm, or user-installed
  Playwright production path.
- Rollback:
  revert this PR; Phase 8G route inputs remain available and safe defaults
  continue to select local Chromium.

### Phase 8H CLI Selected-Route Execution Impact Notes

- GitNexus index was refreshed for the Phase 8H worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW risk for
  `BrowserProviderActionExecutor`,
  `BrowserProviderActionExecutor::execute_routed_with_identity`,
  `BrowserProviderActionRouteOptions`,
  `BrowserProviderActionExecutionOutcome`, and `provider_route_blocked_step`.
- `agent_loop.rs` may be touched only to make the provider-route blocked step
  wording accurate now that some selected non-local routes can execute; it must
  stay orchestration-only.

### Phase 8H CLI Selected-Route Execution Verification Notes

- Focused provider execution tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
  returned `8 passed; 0 failed; 2722 filtered out`.
- Playwright CLI provider tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli`
  returned `22 passed; 0 failed; 2708 filtered out`.
- Browser agent loop regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
  returned `14 passed; 0 failed; 2716 filtered out`.
- Provider route signal regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  returned `12 passed; 0 failed; 2718 filtered out`.
- Provider tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 2714 filtered out`.
- Runtime tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 2671 filtered out`.
- Runtime-pack tests passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 2688 filtered out`.
- Formatting and whitespace checks passed:
  `rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs`
  and `git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8h-cli-selected-execution.md`
  returned no output.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 5`,
  `changed_count: 29`, and `affected_count: 0`; no HIGH/CRITICAL risk.

### Phase 8H CLI Selected-Route Execution Next Action

- Closed. PR #482 merged as `49c274de`; continue with Phase 8I from
  `origin/main` to add provider parity matrix harness evidence before any
  default provider promotion.

## Phase 8I Provider Parity Matrix Harness Entry Criteria

Phase 8I can start because:

- PR #482 merged selected Playwright CLI provider execution to `main` /
  `origin/main`;
- ADR Phase 8 requires provider choice to be backed by scorecards and the same
  browser harness case to run across local Chromium, Playwright CLI,
  Playwright MCP where appropriate, and a mock hosted provider;
- current route decisions and capability cards exist, but there is no
  model-free parity matrix artifact tying shared cases, provider isolation,
  fallback, and artifact visibility together;
- this slice can add harness evidence without changing route ranking,
  promoting providers, adding UI/IPC/DB, or executing hosted/MCP providers;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8i-provider-parity-matrix`;
- the branch starts from `49c274de`, the current `origin/main`.

## Phase 8I Provider Parity Matrix Harness Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8i-provider-parity-matrix.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8i-provider-parity-matrix`
- Branch:
  `codex/browser-runtime-phase8i-provider-parity-matrix`
- Scope:
  add a model-free provider parity matrix harness module that forces shared
  navigate/click cases through each expected provider card and records fallback
  artifact visibility when local Chromium is disabled.
- Current PR:
  PR #483: `https://github.com/novolei/uclaw-new/pull/483`.
- Current commit:
  `feat(browser): add provider parity matrix harness`.
- Non-goal:
  no provider promotion, live default selection change, real hosted execution,
  MCP live execution, UI, IPC, Settings, DB migration, raw tools/scripts, or
  runtime-pack mutation.
- Rollback:
  revert this PR; Phase 8H provider execution remains unchanged.

### Phase 8I Provider Parity Matrix Harness Impact Notes

- GitNexus index was refreshed for the Phase 8I worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported MEDIUM for read-only route/card
  dependencies `BrowserProviderRouteRequest`, `decide_browser_provider_route`,
  and `browser_provider_capability_cards`; no HIGH/CRITICAL risk was observed.
- The implementation does not modify route ranking or provider cards. It adds
  a new harness adapter module and a module export only.

### Phase 8I Provider Parity Matrix Harness Verification Notes

- Provider parity harness verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser_provider`
  completed with `4 passed; 0 failed; 2730 filtered out`.
- Provider route regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with `16 passed; 0 failed; 2718 filtered out`.
- Runtime contracts regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  completed with `10 passed; 0 failed; 2724 filtered out`.
- Provider route rollout bridge regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  completed with `12 passed; 0 failed; 2722 filtered out`.
- Browser runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with `59 passed; 0 failed; 2675 filtered out`.
- Runtime-pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with `42 passed; 0 failed; 2692 filtered out`.
- Rust formatting check passed:
  `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/harness/adapters/browser_provider.rs src-tauri/src/harness/adapters/mod.rs`.
  The `skip_children=true` guard is intentional because rustfmt follows module
  children from `mod.rs` and would rewrite unrelated legacy adapter files.
- Diff hygiene passed:
  `git diff --check -- src-tauri/src/harness/adapters/browser_provider.rs src-tauri/src/harness/adapters/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8i-provider-parity-matrix.md`.
- GitNexus staged detect:
  `scope=staged` completed with LOW risk: 4 changed files, 15 changed symbols,
  0 affected processes.

### Phase 8I Provider Parity Matrix Harness Next Action

- Closed. PR #483 merged as `5a664789`; continue with Phase 8J from
  `origin/main` to add a reversible default-provider policy gate before any
  actual provider promotion or Phase 9 work.

## Phase 8J Provider Default Policy Gate Entry Criteria

Phase 8J can start because:

- PR #483 merged the provider parity matrix harness to `main` / `origin/main`;
- ADR Phase 8 still requires provider default selection to become data-driven
  and reversible;
- current route ranking and parity evidence exist, but there is no explicit
  default-provider policy decision that records promotion/fallback reasons and
  rollback provider id;
- this slice can add the default policy as a pure contract without mutating
  Settings, changing route ranking, promoting providers, adding UI/IPC/DB, or
  executing any provider;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8j-provider-default-policy`;
- the branch starts from `5a664789`, the current `origin/main`.

## Phase 8J Provider Default Policy Gate Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase8j-provider-default-policy.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase8j-provider-default-policy`
- Branch:
  `codex/browser-runtime-phase8j-provider-default-policy`
- Scope:
  add a pure provider default policy gate that retains current local Chromium,
  promotes only when a candidate beats current evidence and records rollback,
  blocks hosted defaults unless explicitly allowed, and selects an
  artifact-visible fallback when the current default is disabled.
- Current PR:
  PR #484: `https://github.com/novolei/uclaw-new/pull/484`.
- Current commit:
  current branch `HEAD`, subject `feat(browser): add provider default policy gate`.
- Non-goal:
  no live default mutation, provider promotion, route ranking change,
  provider execution, MCP execution, hosted execution, UI, IPC, Settings, DB
  migration, runtime-pack mutation, or task-loop behavior.
- Rollback:
  revert this PR; Phase 8I parity harness and Phase 8H provider execution
  remain unchanged.

### Phase 8J Provider Default Policy Gate Impact Notes

- GitNexus index was refreshed for the Phase 8J worktree before impact checks;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus impact before edits reported LOW for `BrowserProviderCapabilityCard`
  and MEDIUM for read-only dependencies `browser_provider_capability_cards`
  and `rank_browser_provider_candidates`; no HIGH/CRITICAL risk was observed.
- GitNexus did not resolve `browser/mod.rs` as a symbol target and returned
  UNKNOWN. The edit is intentionally kept to one additive module export.
- The implementation does not modify route ranking, live provider execution,
  provider capability cards, `agent_loop.rs`, or `tauri_commands.rs`.

### Phase 8J Provider Default Policy Gate Verification Notes

- Provider default policy focused verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults`
  completed with `6 passed; 0 failed; 2734 filtered out` after PR #484
  reviewer fixes that force disabled/unavailable current defaults through
  fallback-or-blocked semantics before any promotion decision.
- Browser provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with `16 passed; 0 failed; 2723 filtered out`.
- Runtime contract regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  completed with `10 passed; 0 failed; 2729 filtered out`.
- Default browser-runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with `59 passed; 0 failed; 2680 filtered out`.
- Runtime-pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with `42 passed; 0 failed; 2697 filtered out`.
- Formatting passed for the new provider-default policy module:
  `rustfmt --edition 2021 --check src-tauri/src/browser/provider_defaults.rs`.
- Formatting note: `rustfmt --edition 2021 --check --config skip_children=true
  src-tauri/src/browser/mod.rs` reports pre-existing formatting drift across the
  legacy module root. The Phase 8J diff in `browser/mod.rs` is one additive
  `pub mod provider_defaults;` line, so this PR intentionally avoids a broad
  unrelated reformat of that file.
- Diff hygiene passed:
  `git diff --check -- src-tauri/src/browser/provider_defaults.rs
  src-tauri/src/browser/mod.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase8j-provider-default-policy.md`.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`,
  `changed_count: 16`, `affected_count: 0`, and `affected_processes: []`.

### Phase 8J Provider Default Policy Gate Next Action

- Closed. PR #484 merged as `cab8f161`; continue with Phase 9A from
  `origin/main` to add a pure recipe candidate contract before any replay,
  persistence, UI, IPC, or production promotion.

## Phase 9A Recipe Candidate Contract Entry Criteria

Phase 9A can start because:

- PR #484 merged Phase 8J provider default policy to `main` / `origin/main`;
- ADR Phase 9 requires recipes, locator cache, and domain-skill candidates only
  after provider behavior is observable;
- provider route evidence, parity harnesses, and reversible default policy now
  exist, but there is no recipe candidate contract for redaction, fingerprints,
  provider-version invalidation, promotion state, or rollback;
- this slice can add a pure contract without replaying recipes, writing domain
  skills, persisting locator caches, adding UI/IPC/DB, or mutating production
  behavior;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9a-recipe-contract`;
- the branch originally started from `cab8f161` after PR #484 and was cleanly
  rebased onto `e5a98220`, the current `origin/main`, after unrelated PR #485
  landed while Phase 9A was in flight.

## Phase 9A Recipe Candidate Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase9a-recipe-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9a-recipe-contract`
- Branch:
  `codex/browser-runtime-phase9a-recipe-contract`
- Scope:
  add pure recipe/domain-skill candidate DTOs, candidate validation, redaction
  rejection, promotion-readiness metadata, rollback metadata, replay decision
  gates for fingerprint/provider-version mismatch, and tests.
- Current PR:
  PR #486 (`https://github.com/novolei/uclaw-new/pull/486`).
- Current commit:
  current branch `HEAD` with subject `feat(browser): add recipe candidate
  contract`.
- Non-goal:
  no recipe replay execution, locator cache persistence, production promotion,
  domain-skill file writes, UI, IPC, Settings, DB migration, provider route
  change, hosted provider integration, `agentic_loop.rs`, or `tauri_commands.rs`
  changes.
- Rollback:
  revert this PR; Phase 8 provider routing/default policy remains unchanged.

### Phase 9A Recipe Candidate Contract Impact Notes

- GitNexus index was refreshed for the Phase 9A worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus did not resolve `browser/mod.rs` as a symbol target and returned
  UNKNOWN. The edit is intentionally kept to one additive module export.
- New recipe contract code is isolated in `src-tauri/src/browser/recipes.rs`
  and is not consumed by live task routing, provider selection, UI, IPC, DB, or
  persistence.
- Fresh reviewer Averroes blocked the first PR revision because replay could
  allow a promoted-but-invalid candidate, ignored request/candidate recipe-id
  mismatch, and allowed promotion readiness with replay failures. The contract
  now fails closed for invalid replay candidates, adds a recipe-id mismatch
  status, and requires zero replay failures for promotion evidence.
- Fresh reviewer Lagrange blocked the second PR revision because empty rollback
  metadata and blank semantic locators could still pass the replay contract.
  The contract now trims semantic locator fields and rollback ids before
  treating them as stable/present.

### Phase 9A Recipe Candidate Contract Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed: 6 passed, 0 failed, 2740 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults`
  passed: 6 passed, 0 failed, 2740 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 16 passed, 0 failed, 2730 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 59 passed, 0 failed, 2687 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 passed, 0 failed, 2704 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs`
  passed.
- `rustfmt --edition 2021 --check --config skip_children=true
  src-tauri/src/browser/mod.rs` still reports pre-existing legacy formatting
  drift in the module root; this phase intentionally does not reformat the
  legacy file beyond the one additive `pub mod recipes;` export.
- `git diff --check -- src-tauri/src/browser/recipes.rs
  src-tauri/src/browser/mod.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase9a-recipe-contract.md`
  passed.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 4`,
  `changed_count: 15`, `affected_count: 0`, and `affected_processes: []`.
- After clean rebase onto `e5a98220`, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`:
  6 passed, 0 failed, 2753 filtered out.
- After clean rebase onto `e5a98220`, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`:
  59 passed, 0 failed, 2700 filtered out.
- After reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`:
  9 passed, 0 failed, 2753 filtered out.
- After reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`:
  16 passed, 0 failed, 2746 filtered out.
- After reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`:
  42 passed, 0 failed, 2720 filtered out.
- After second reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`:
  11 passed, 0 failed, 2753 filtered out.
- After second reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`:
  16 passed, 0 failed, 2748 filtered out.
- After second reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`:
  59 passed, 0 failed, 2705 filtered out.
- After second reviewer fixes, reran
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`:
  42 passed, 0 failed, 2722 filtered out.

### Phase 9A Recipe Candidate Contract Next Action

- Closed. PR #486 merged as `5228d0ab`; continue with Phase 9B from
  `origin/main` to add a pure recipe normalization intake boundary before any
  replay execution, locator cache persistence, domain-skill writes, UI, IPC,
  DB migration, or provider behavior change.

## Phase 9B Recipe Normalization Intake Entry Criteria

Phase 9B can start because:

- PR #486 merged Phase 9A's pure recipe candidate/replay contract into `main`
  and `origin/main`;
- ADR Phase 9 requires recipes and domain-skill candidates, but production
  replay/promotion still needs a deterministic candidate intake boundary first;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9b-recipe-normalization`;
- the branch starts from `5228d0ab`, the current `origin/main`;
- this slice can add a pure normalization helper without replaying recipes,
  persisting locator caches, writing domain skills, adding UI/IPC/DB, or
  mutating provider behavior.

## Phase 9B Recipe Normalization Intake Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase9b-recipe-normalization.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9b-recipe-normalization`
- Branch:
  `codex/browser-runtime-phase9b-recipe-normalization`
- Scope:
  add pure recipe action-observation DTOs, normalization input/output DTOs, a
  deterministic builder from successful action observations to
  `BrowserRecipeCandidate`, failed-action rejection metadata, artifact/harness
  evidence normalization, and focused tests.
- Current PR:
  PR #487 (`https://github.com/novolei/uclaw-new/pull/487`).
- Current commit:
  current branch `HEAD` with subject `feat(browser): normalize recipe
  candidates`.
- Non-goal:
  no recipe replay execution, locator cache persistence, production promotion,
  domain-skill file writes, UI, IPC, Settings, DB migration, provider route
  change, hosted provider integration, `agentic_loop.rs`, or
  `tauri_commands.rs` changes.
- Rollback:
  revert this PR; Phase 9A candidate/replay validation remains unchanged.

### Phase 9B Recipe Normalization Intake Impact Notes

- GitNexus index was refreshed for the Phase 9B worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus file-level impact for `src-tauri/src/browser/recipes.rs` reported
  LOW risk with 0 impacted symbols and 0 affected processes before editing the
  recipe contract module.
- New normalization code is isolated in `src-tauri/src/browser/recipes.rs` and
  is not consumed by live task routing, provider selection, UI, IPC, DB, or
  persistence.
- The intake builder deliberately marks failed action observations as rejected
  and increments replay-failure evidence so candidates cannot silently look
  promotion-ready.

### Phase 9B Recipe Normalization Intake Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed: 16 passed, 0 failed, 2753 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 passed, 0 failed, 2727 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 59 passed, 0 failed, 2710 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 16 passed, 0 failed, 2753 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs` passed.
- `git diff --check -- src-tauri/src/browser/recipes.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase9b-recipe-normalization.md`
  passed.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 3`,
  `changed_count: 45`, `affected_count: 0`, and `affected_processes: []`.

### Phase 9B Recipe Normalization Intake Next Action

- Closed. PR #487 merged as `930530cb`; continue with Phase 9C from
  `origin/main` to add a pure locator-cache contract before any replay
  execution, locator persistence, domain-skill writes, UI, IPC, DB migration,
  or provider behavior change.

## Phase 9C Locator Cache Contract Entry Criteria

Phase 9C can start because:

- PR #487 merged Phase 9B's pure recipe normalization intake into `main` and
  `origin/main`;
- ADR Phase 9 requires locator/action caching and deterministic reuse only
  after recipe candidates are normalized and provider behavior is observable;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9c-locator-cache-contract`;
- the branch starts from `930530cb`, the current `origin/main`;
- this slice can add a pure locator-cache validation and reuse decision
  boundary without replaying actions, persisting caches, writing domain skills,
  adding UI/IPC/DB, or mutating provider behavior.

## Phase 9C Locator Cache Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase9c-locator-cache-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9c-locator-cache-contract`
- Branch:
  `codex/browser-runtime-phase9c-locator-cache-contract`
- Scope:
  add pure locator-cache key/entry/validation/reuse decision DTOs, a builder
  from recipe candidate action templates to cache entries, fail-closed
  fingerprint/provider/version/promotion/policy checks, and focused tests.
- Current PR:
  PR #488 (`https://github.com/novolei/uclaw-new/pull/488`).
- Current commit:
  current branch `HEAD` with subject `feat(browser): add recipe locator cache
  contract`.
- Non-goal:
  no recipe replay execution, locator cache persistence, production promotion,
  domain-skill file writes, UI, IPC, Settings, DB migration, provider route
  change, hosted provider integration, `agentic_loop.rs`, or
  `tauri_commands.rs` changes.
- Rollback:
  revert this PR; Phase 9A candidate/replay validation and Phase 9B
  normalization remain unchanged.

### Phase 9C Locator Cache Contract Impact Notes

- GitNexus index was refreshed for the Phase 9C worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus file-level impact for `src-tauri/src/browser/recipes.rs` reported
  LOW risk with 0 impacted symbols and 0 affected processes before editing the
  recipe contract module.
- New locator-cache code is isolated in `src-tauri/src/browser/recipes.rs` and
  is not consumed by live task routing, provider selection, UI, IPC, DB, or
  persistence.
- The reuse decision deliberately requires promoted state, clean validation,
  matching recipe/action id, matching DOM/a11y fingerprint, matching provider
  id/version, and an explicit production replay policy flag before returning a
  reusable locator.

### Phase 9C Locator Cache Contract Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed: 22 passed, 0 failed, 2753 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: 42 passed, 0 failed, 2733 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: 59 passed, 0 failed, 2716 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: 16 passed, 0 failed, 2759 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs` passed.
- `git diff --check -- src-tauri/src/browser/recipes.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase9c-locator-cache-contract.md`
  passed.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 3`,
  `changed_count: 41`, `affected_count: 0`, and `affected_processes: []`.

### Phase 9C Locator Cache Contract Next Action

- Closed. PR #488 merged as `d96f432d`; continue with Phase 9D from
  `origin/main` to add a pure domain-skill candidate gate before any
  domain-skill file writes, replay execution, locator persistence, UI, IPC, DB
  migration, or provider behavior change.

## Phase 9D Domain-Skill Candidate Gate Entry Criteria

Phase 9D can start because:

- PR #488 merged Phase 9C's pure locator cache contract into `main` and
  `origin/main`;
- ADR Phase 9 requires domain-skill candidates to be redacted, harness-gated,
  promotion-gated, and rollback-backed before any production use;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9d-domain-skill-candidate-gate`;
- the branch starts from `d96f432d`, the current `origin/main`;
- this slice can add a pure domain-skill candidate gate without writing
  domain-skill files, replaying actions, persisting locators, adding UI/IPC/DB,
  or mutating provider behavior.

## Phase 9D Domain-Skill Candidate Gate Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase9d-domain-skill-candidate-gate.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9d-domain-skill-candidate-gate`
- Branch:
  `codex/browser-runtime-phase9d-domain-skill-candidate-gate`
- Scope:
  add pure domain-skill candidate gate status/report DTOs, a deterministic
  validator from `BrowserRecipeCandidate`, redaction/evidence/harness/rollback
  checks, promotion eligibility, and focused tests.
- Current PR:
  PR #489 (`https://github.com/novolei/uclaw-new/pull/489`).
- Current commit:
  current branch `HEAD` with subject `feat(browser): gate domain skill
  candidates`.
- Non-goal:
  no domain-skill file writes, recipe replay execution, locator cache
  persistence, production promotion, UI, IPC, Settings, DB migration, provider
  route change, hosted provider integration, `agentic_loop.rs`, or
  `tauri_commands.rs` changes.
- Rollback:
  revert this PR; Phase 9A/9B/9C recipe contracts remain unchanged.

### Phase 9D Domain-Skill Candidate Gate Impact Notes

- GitNexus index was refreshed for the Phase 9D worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus file-level impact for `src-tauri/src/browser/recipes.rs` reported
  LOW risk with 0 impacted symbols and 0 affected processes before editing the
  recipe contract module.
- New domain-skill candidate gate code is isolated in
  `src-tauri/src/browser/recipes.rs` and is not consumed by live task routing,
  provider selection, UI, IPC, DB, or persistence.
- The gate deliberately distinguishes clean-but-not-promoted candidates from
  rejected candidates so future file generation can remain explicit and
  rollbackable.
- Fresh reviewer Bohr blocked PR #489 on whitespace-only domain/evidence
  fields being treated as present. The fix trims all domain-skill and harness
  evidence presence checks before promotion eligibility, keeping reports
  normalized through `unique_non_empty`.

### Phase 9D Domain-Skill Candidate Gate Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed before reviewer fix: 27 passed, 0 failed, 2753 filtered out.
- After the reviewer whitespace-only gate fix,
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed: 28 passed, 0 failed, 2753 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed after the reviewer fix: 42 passed, 0 failed, 2739 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed after the reviewer fix: 59 passed, 0 failed, 2722 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed after the reviewer fix: 16 passed, 0 failed, 2765 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs` passed
  after the reviewer fix.
- `git diff --check -- src-tauri/src/browser/recipes.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase9d-domain-skill-candidate-gate.md`
  passed after the reviewer fix.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 3`,
  `changed_count: 30`, `affected_count: 0`, and `affected_processes: []`.

### Phase 9D Domain-Skill Candidate Gate Next Action

- Closed. PR #489 merged as `769e0d1e`; continue with Phase 9E from
  `origin/main` to add a pure recipe/domain-skill harness matrix before any
  recipe replay execution, locator persistence, domain-skill file writes, UI,
  IPC, DB migration, or provider behavior change.

## Phase 9E Recipe/Domain-Skill Harness Matrix Entry Criteria

Phase 9E can start because:

- PR #489 merged Phase 9D's pure domain-skill candidate gate into `main` and
  `origin/main`;
- ADR Phase 9's final gate requires recipe/domain-skill harness coverage for
  replay success, fingerprint mismatch, redaction, promotion, rejection,
  rollback, and provider-version invalidation;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9e-harness-matrix`;
- the branch starts from `769e0d1e`, the current `origin/main`;
- this slice can add a pure matrix/report boundary without replaying actions,
  persisting locator caches, writing domain-skill files, adding UI/IPC/DB, or
  mutating provider behavior.

## Phase 9E Recipe/Domain-Skill Harness Matrix Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase9e-harness-matrix.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase9e-harness-matrix`
- Branch:
  `codex/browser-runtime-phase9e-harness-matrix`
- Scope:
  add pure matrix case/report DTOs and a deterministic evaluator that composes
  candidate validation, replay decisions, locator reuse, and domain-skill gate
  decisions into ADR Phase 9 harness evidence.
- Current PR:
  PR #490 (`https://github.com/novolei/uclaw-new/pull/490`).
- Current commit:
  current branch `HEAD` with subject `feat(browser): add recipe harness
  matrix`.
- Non-goal:
  no recipe replay execution, locator cache persistence, production promotion,
  domain-skill file writes, UI, IPC, Settings, DB migration, provider route
  change, hosted provider integration, `agentic_loop.rs`, or
  `tauri_commands.rs` changes.
- Rollback:
  revert this PR; Phase 9A/9B/9C/9D recipe contracts remain unchanged.

### Phase 9E Recipe/Domain-Skill Harness Matrix Impact Notes

- GitNexus index was refreshed for the Phase 9E worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus file-level impact for `src-tauri/src/browser/recipes.rs` reported
  LOW risk with 0 impacted symbols and 0 affected processes before editing the
  recipe contract module.
- New matrix code is isolated in `src-tauri/src/browser/recipes.rs` and is not
  consumed by live task routing, provider selection, UI, IPC, DB, persistence,
  or runtime-pack execution.
- The evaluator is intentionally scenario-based: it derives safe in-memory
  replay, locator reuse, fingerprint mismatch, provider-version invalidation,
  redaction, promotion, rejection, and rollback checks from one candidate.
- Fresh reviewer Arendt blocked PR #490 on locator reuse case reports dropping
  locator/action-specific artifact refs. The fix threads built entry and reuse
  decision artifact refs into the locator matrix case and top-level matrix
  report.

### Phase 9E Recipe/Domain-Skill Harness Matrix Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed before reviewer fix: 32 passed, 0 failed, 2753 filtered out.
- After the reviewer artifact-preservation fix,
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::recipes`
  passed: 33 passed, 0 failed, 2753 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed after the reviewer fix: 42 passed, 0 failed, 2744 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed after the reviewer fix: 59 passed, 0 failed, 2727 filtered out.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed after the reviewer fix: 16 passed, 0 failed, 2770 filtered out.
- `rustfmt --edition 2021 --check src-tauri/src/browser/recipes.rs` passed.
- `git diff --check -- src-tauri/src/browser/recipes.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase9e-harness-matrix.md`
  passed.
- GitNexus staged detect reported `risk_level: low`, `changed_files: 3`,
  `changed_count: 38`, `affected_count: 0`, and `affected_processes: []`.
- GitNexus staged detect after the reviewer artifact-preservation fix reported
  `risk_level: low`, `changed_files: 2`, `changed_count: 18`,
  `affected_count: 0`, and `affected_processes: []`.

### Phase 9E Recipe/Domain-Skill Harness Matrix Next Action

- Closed. PR #490 merged as `c16a6720`; continue with Phase 10A from
  `origin/main` to add a pure hosted-provider capability/policy contract before
  any real hosted SDK, network path, credentials, provider promotion, UI, IPC,
  DB migration, or live execution.

## Phase 10A Hosted-Provider Capability Contract Entry Criteria

Phase 10A can start because:

- PR #490 merged Phase 9E's recipe/domain-skill harness matrix into `main` and
  `origin/main`;
- ADR Phase 10 requires hosted browser systems only as opt-in provider adapters
  with explicit data-boundary policy, profile/storage policy, artifact
  handling, cost visibility, disable path, and local fallback;
- the existing `browser.hosted` capability card exists but only has coarse tags
  and no pure gate that a harness can evaluate before real hosted execution;
- this slice can add a hosted-provider contract without vendor SDKs, network
  calls, credentials, UI, IPC, DB migration, provider promotion, or live
  execution;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase10a-hosted-provider-contract`;
- the branch starts from `c16a6720`, the current `origin/main`.

## Phase 10A Hosted-Provider Capability Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase10a-hosted-provider-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase10a-hosted-provider-contract`
- Branch:
  `codex/browser-runtime-phase10a-hosted-provider-contract`
- Scope:
  add pure hosted-provider policy/capability DTOs, a deterministic gate report,
  hosted provider status conversion, capability-card cost/profile/data-boundary
  declarations, and focused fallback/policy tests.
- Current PR:
  PR #491 merged as `568a0af0`.
- Current commit:
  `55b361d6 feat(browser): add hosted provider policy contract`
- Non-goal:
  no real hosted provider SDK, network call, credential storage, live hosted
  execution, provider default mutation, route promotion, UI, IPC, Settings, DB
  migration, TaskEvent emission, `agentic_loop.rs`, or `tauri_commands.rs`
  changes.
- Rollback:
  revert this PR; Phase 8 provider routing/default policy and Phase 9
  recipe/domain-skill harnesses remain unchanged.

### Phase 10A Hosted-Provider Capability Contract Impact Notes

- GitNexus index was refreshed for the Phase 10A worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus pre-change impact reported LOW for
  `BrowserProviderCapabilityCard`, MEDIUM for
  `browser_provider_capability_cards`, and MEDIUM for
  `rank_browser_provider_candidates`; no HIGH/CRITICAL risk was observed.
- GitNexus did not resolve `src-tauri/src/browser/mod.rs` as a target and
  returned UNKNOWN. The module-root edit is intentionally limited to one
  additive hosted-provider module export.
- This slice explicitly rechecks the prior dry-run concern: it does not avoid
  `agentic_loop.rs` or `tauri_commands.rs` out of fear; those files are simply
  not needed for a pure hosted-provider contract and should stay thin until a
  later live routing/IPC phase requires them.

### Phase 10A Hosted-Provider Capability Contract Verification Notes

- Hosted-provider focused verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::hosted_provider`
  completed with `6 passed; 0 failed; 2786 filtered out` after reviewer
  Ramanujan caught that `provider_disabled` blockers were dropped during
  `BrowserProviderStatus` conversion. The fix adds an aggregate
  `hosted_policy_gate` setup check so any report blocker keeps status
  non-ready.
- Provider route regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with `16 passed; 0 failed; 2775 filtered out` after the final diff
  stopped changing `provider_tests.rs`.
- Runtime contract regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  completed with `10 passed; 0 failed; 2782 filtered out`.
- Provider-default policy regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_defaults`
  completed with `6 passed; 0 failed; 2786 filtered out`.
- Runtime-pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with `42 passed; 0 failed; 2750 filtered out`.
- Default browser-runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with `59 passed; 0 failed; 2733 filtered out`.
- Rust formatting passed for changed Rust files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/hosted_provider.rs
  src-tauri/src/browser/runtime_contracts.rs
  src-tauri/src/browser/runtime_contracts_tests.rs
  src-tauri/src/browser/provider_defaults.rs`.
- Formatting note: `rustfmt --edition 2021 --check --config
  skip_children=true src-tauri/src/browser/mod.rs` still reports pre-existing
  legacy module-root formatting drift beyond the one additive
  `pub mod hosted_provider;` export, so this phase avoids a broad unrelated
  reformat.
- Diff hygiene passed:
  `git diff --check -- src-tauri/src/browser/hosted_provider.rs
  src-tauri/src/browser/runtime_contracts.rs
  src-tauri/src/browser/runtime_contracts_tests.rs
  src-tauri/src/browser/provider_defaults.rs src-tauri/src/browser/mod.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase10a-hosted-provider-contract.md`.
- GitNexus all-change detect initially reported HIGH after a provider-test
  insertion was mapped onto existing router test execution flows. Removing that
  unnecessary `provider_tests.rs` edit reduced detect to LOW:
  `changed_files: 5`, `changed_count: 26`, `affected_count: 0`,
  `affected_processes: []`.
- GitNexus staged detect reported LOW: `changed_files: 7`,
  `changed_count: 28`, `affected_count: 0`, `affected_processes: []`.
- Fresh reviewer Ramanujan blocked PR #491 on a real fail-closed bug: accepted
  hosted policy plus `disabled_provider_ids=["browser.hosted"]` could still
  convert to ready status. The status conversion now fails closed for all
  report blockers, and the disabled-provider regression is covered.

### Phase 10A Hosted-Provider Capability Contract Next Action

- Closed. PR #491 merged as `568a0af0`; continue with Phase 10B from
  `origin/main` to cover the ADR Phase 10 hosted-provider gate with a pure
  harness matrix before considering the Browser Runtime Supervisor ADR
  complete.

## Phase 10B Hosted-Provider Harness Matrix Entry Criteria

Phase 10B can start because:

- PR #491 merged Phase 10A's hosted-provider capability/policy contract into
  `main` and `origin/main`;
- ADR Phase 10 still requires harness coverage for hosted-provider disabled
  fallback, data-boundary prompt, artifact capture, cost visibility, and local
  provider fallback;
- a pure harness adapter can compose Phase 10A's gate/status contract without
  adding a hosted vendor SDK, network call, credential store, UI, IPC, DB
  migration, provider promotion, or live execution;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase10b-hosted-provider-harness`;
- the branch starts from `568a0af0`, the current `origin/main`.

## Phase 10B Hosted-Provider Harness Matrix Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase10b-hosted-provider-harness.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase10b-hosted-provider-harness`
- Branch:
  `codex/browser-runtime-phase10b-hosted-provider-harness`
- Scope:
  add an attachable hosted-provider harness matrix report that evaluates
  disabled fallback, data-boundary prompt required, artifact capture required,
  cost visibility required, local fallback required, and opt-in mock-hosted
  ready paths through the Phase 10A pure gate/status contract.
- Current PR:
  PR #492: `https://github.com/novolei/uclaw-new/pull/492`
- Current commit:
  Current PR head: `feat(browser): add hosted provider harness matrix`
- Non-goal:
  no hosted provider SDK, network call, credential storage, live hosted browser
  execution, provider default mutation, route promotion, UI, IPC, Settings, DB
  migration, TaskEvent emission, `agentic_loop.rs`, or `tauri_commands.rs`
  changes.
- Rollback:
  revert this PR; Phase 10A hosted-provider policy, Phase 9 recipe/domain-skill
  harnesses, and Phase 8 provider routing/default policy remain unchanged.

### Phase 10B Hosted-Provider Harness Matrix Impact Notes

- GitNexus index was refreshed for the Phase 10B worktree before edits;
  generated AGENTS/CLAUDE statistics changes were restored as noise.
- GitNexus pre-change impact for `src-tauri/src/harness/adapters/mod.rs`
  returned UNKNOWN because file-level module exports are not resolved as a
  graph symbol. The edit is intentionally limited to one additive
  `pub mod hosted_provider;` export.
- New `src-tauri/src/harness/adapters/hosted_provider.rs` defines additive
  harness DTOs, a default matrix builder, artifact attachment, and focused
  tests without modifying existing provider, runtime, UI, IPC, DB, or task-loop
  symbols.
- This slice explicitly rechecks the prior dry-run concern: it does not avoid
  `agentic_loop.rs` or `tauri_commands.rs` out of fear; those files are not
  needed to prove the ADR Phase 10 harness gate and should stay thin.

### Phase 10B Hosted-Provider Harness Matrix Verification Notes

- Hosted-provider harness focused verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::hosted_provider`
  completed with `4 passed; 0 failed; 2792 filtered out`.
- Hosted-provider contract regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::hosted_provider`
  completed with `6 passed; 0 failed; 2790 filtered out`.
- Runtime-pack regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  completed with `42 passed; 0 failed; 2754 filtered out`.
- Default browser-runtime regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  completed with `59 passed; 0 failed; 2737 filtered out`.
- Provider route regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  completed with `16 passed; 0 failed; 2780 filtered out`.
- Rust formatting passed for changed Rust files:
  `rustfmt --edition 2021 --check
  src-tauri/src/harness/adapters/hosted_provider.rs` and
  `rustfmt --edition 2021 --check --config skip_children=true
  src-tauri/src/harness/adapters/mod.rs`.
- Diff hygiene passed:
  `git diff --check -- src-tauri/src/harness/adapters/hosted_provider.rs
  src-tauri/src/harness/adapters/mod.rs
  docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
  docs/superpowers/plans/2026-05-24-browser-runtime-phase10b-hosted-provider-harness.md`.
- GitNexus staged detect reported LOW: `changed_files: 4`,
  `changed_count: 16`, `affected_count: 0`, `affected_processes: []`.

### Phase 10B Hosted-Provider Harness Matrix Next Action

- Closed. PR #492 merged as `58e2d58b`; Phase 10B final commit was
  `01d96e7d feat(browser): add hosted provider harness matrix`.

## Phase 6H Identity Authorization Contract Entry Criteria

Phase 6H can start because:

- PR #492 merged Phase 10B and closed the ADR Phase 10 hosted-provider harness
  gate;
- ADR Phase 6 still requires user-consented identity authorization/profile UX;
- Phase 6A-6F covered revocation, active-task drain, visible status, and resume
  decisions, but the generic identity authorization completion contract is not
  exposed through browser identity IPC/bridge;
- current code already captures login state for automation specs through
  `browser_ui_complete_login` / `browser_webview_complete_login`, proving this
  is not a speculative dry-run lane;
- this slice can make that capture reusable while keeping `tauri_commands.rs`
  thin and preserving the existing automation command names.

## Phase 6H Identity Authorization Contract Progress

- Plan:
  `docs/superpowers/plans/2026-05-24-browser-runtime-phase6h-identity-authorization-contract.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6h-identity-authorization-contract`
- Branch:
  `codex/browser-runtime-phase6h-identity-authorization-contract`
- Scope:
  add a generic browser identity authorization helper/IPC contract that can
  import captured browser/WebView storage state into a global browser identity
  profile, expose frontend bridge types/functions, and leave the existing
  automation login commands as compatibility shims.
- Current PR:
  PR #493
  (`https://github.com/novolei/uclaw-new/pull/493`).
- Current commit:
  current PR head commit: `feat(browser): add identity authorization contract`.
- Non-goal:
  no Settings connect UI, payment confirmation UI, TaskEvent emission, provider
  promotion, DB migration, hosted provider integration, external Chrome attach,
  raw secret display, global npm/manual Playwright path, or broad
  `agentic_loop.rs` / `tauri_commands.rs` rewrite.
- Rollback:
  revert this PR; Phase 6 list/revoke/resume contracts, Phase 8 provider
  routing, Phase 10 hosted-provider harnesses, and existing automation login
  command names remain recoverable.

### Phase 6H Identity Authorization Contract Impact Notes

- GitNexus index refreshed for the Phase 6H worktree; generated
  `AGENTS.md` / `CLAUDE.md` statistics noise was restored.
- GitNexus impact for `BrowserIdentityProfileSummary` returned LOW: one direct
  caller, no affected processes.
- GitNexus impact for frontend `listBrowserIdentities` returned LOW: no direct
  callers/processes in the indexed graph.
- GitNexus impact for frontend `browserUICompleteLogin` and
  `browserWebviewCompleteLogin` returned LOW; existing automation call sites
  remain compatibility shims.
- GitNexus impact for `BrowserIdentityStatusReport` returned CRITICAL because
  `tauri-bridge.ts` is a large aggregated bridge surface. This slice avoided
  editing that interface and added separate authorization DTOs/functions.
- GitNexus could not resolve the large-file `tauri_commands.rs` login helper
  symbols or `main.rs` `invoke_handler` macro target, returning UNKNOWN. Treat
  those as high-attention additive/narrow edits and require fresh review before
  merge.
- The worktree needed the ignored `ui/node_modules` link from the primary
  checkout before Vitest could run; this did not change tracked files.
- This slice explicitly rechecks the dry-run/special-DMZ concern: existing real
  login capture lives in `tauri_commands.rs`; the correction is to extract
  reusable behavior into browser identity modules and keep `tauri_commands.rs`
  as a shim, not to avoid the file.

### Phase 6H Identity Authorization Contract Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_authorization`
  passed: `5 passed; 0 failed; 2798 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
  passed: `6 passed; 0 failed; 2797 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: `42 passed; 0 failed; 2761 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: `59 passed; 0 failed; 2744 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: `16 passed; 0 failed; 2787 filtered out`.
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
  passed: `1 passed`; `4 tests passed`.
- `rustfmt --edition 2021 --check src-tauri/src/browser/identity_authorization.rs src-tauri/src/browser/identity_ipc.rs`
  passed.
- Broad rustfmt over `tauri_commands.rs`, `main.rs`, and `browser/mod.rs` was
  intentionally not used because those legacy large files have pre-existing
  formatting drift outside this phase; `git diff --check -- <changed files>`
  passed for the Phase 6H changed-file set.
- GitNexus staged detect reported MEDIUM risk: 36 changed symbols across 9
  files, 5 affected processes, and no HIGH/CRITICAL warning. Affected process
  coverage is limited to `main` command registration, browser identity Settings
  list flow, and documentation sections.

### Phase 6H Identity Authorization Contract Next Action

- Closed. PR #493 merged as `d248e4f5`; Phase 6H final commit was
  `7a0f9254 feat(browser): add identity authorization contract`.

## Phase 6I Payment Confirmation Harness Entry Criteria

Phase 6I can start because:

- PR #493 merged Phase 6H and closed the generic identity authorization
  completion contract gap;
- ADR Phase 6 still has a gate item for payment confirmation harness coverage;
- current code already detects `BrowserBoundaryKind::Payment` and routes human
  boundaries through the browser ask-user bridge;
- the missing piece is explicit harness/scorecard evidence that a payment
  boundary must include `ask_user_response` confirmation.

## Phase 6I Payment Confirmation Harness Progress

- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-phase6i-payment-confirmation-harness.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6i-payment-confirmation-harness`
- Branch:
  `codex/browser-runtime-phase6i-payment-confirmation-harness`
- Scope:
  add a Browser parity payment-confirmation case and scorecard check requiring
  both `needs_user_intervention` with kind `payment` and successful
  `ask_user_response` evidence.
- Current PR:
  PR #494
  (`https://github.com/novolei/uclaw-new/pull/494`).
- Current commit:
  current PR head commit: `test(browser): cover payment confirmation harness`.
- Non-goal:
  no payment UI, checkout execution, billing-data handling, provider promotion,
  DB migration, hosted provider, Settings change, or task-loop rewrite.
- Rollback:
  revert this PR; runtime payment boundary detection and existing ask-user
  bridge behavior remain unchanged.

### Phase 6I Payment Confirmation Harness Impact Notes

- GitNexus index refreshed for the Phase 6I worktree; generated
  `AGENTS.md` / `CLAUDE.md` statistics noise was restored.
- GitNexus impact for `BUILTIN_BROWSER_PARITY_CASES` returned LOW: no direct
  callers/processes reported.
- GitNexus impact for `score_browser_run` returned MEDIUM: 6 direct callers in
  harness adapter tests, no affected execution processes, one affected module
  (`Adapters`).
- Fresh reviewer Lovelace blocked PR #494 because the new case pointed at
  `/checkout` before the fixture server served that route. The branch now adds a
  deterministic checkout fixture containing payment text and a `cc-number`
  input, plus a focused server test proving the route exists.

### Phase 6I Payment Confirmation Harness Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser`
  passed before reviewer fix: `18 passed; 0 failed; 2787 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser`
  passed after reviewer fix: `19 passed; 0 failed; 2787 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::boundary`
  passed: `7 passed; 0 failed; 2798 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  passed: `42 passed; 0 failed; 2763 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  passed: `59 passed; 0 failed; 2746 filtered out`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  passed: `16 passed; 0 failed; 2789 filtered out`.
- `rustfmt --edition 2021 --check src-tauri/src/harness/adapters/browser.rs`
  passed.
- `git diff --check -- <changed files>` passed.
- GitNexus staged detect reported LOW risk: 21 changed symbols across 4 files,
  0 affected processes, and no HIGH/CRITICAL warning.

### Phase 6I Payment Confirmation Harness Next Action

- Closed. PR #494 merged as `f6447a71`; final commit was `aadd581b
  test(browser): cover payment confirmation harness`. Continue with the
  docs-only completion audit from `origin/main` to close the tracker.

---

## Completion Audit Entry Criteria

The final completion audit can start because:

- PR #494 merged Phase 6I into `main` / `origin/main`, closing the payment
  confirmation harness gap that the Phase 10 closeout review found in Phase 6;
- `origin/main` now includes merged Browser Runtime phase work through Phase 10B
  plus the Phase 6H/6I backfills;
- the remaining discrepancy is tracker state, not runtime behavior: Quick View
  and Branch Hygiene still needed to reflect PR #494 and the ADR closeout.

## Completion Audit Progress

- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-completion-audit.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-completion-audit`
- Branch:
  `codex/browser-runtime-completion-audit`
- Scope:
  reconcile the tracker with `origin/main`, PR #492/#493/#494 merge evidence,
  and ADR Phase 0-10 completion state.
- Current PR:
  PR #495
  (`https://github.com/novolei/uclaw-new/pull/495`).
- Current commit:
  latest pushed head of PR #495; GitHub's PR commit list is canonical because
  reviewer-finding amendments can change the head hash before merge.
- Non-goal:
  no Rust, TypeScript, UI, IPC, DB migration, provider promotion, hosted SDK,
  browser worker, identity runtime, payment UI, or task-loop behavior change.
- Rollback:
  revert this docs-only PR; all Browser Runtime implementation PRs remain
  untouched.

### Completion Audit Impact Notes

- Documentation only.
- No existing function/class/method/symbol is edited, so pre-edit GitNexus
  symbol impact is not required.
- GitNexus `detect_changes` is still required before commit.
- The primary worktree remains separate with unrelated user changes; this audit
  edits only the isolated completion-audit worktree.

### Completion Audit Verification Notes

- Passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-completion-audit.md`.
- Passed:
  `rg -n "Current phase|Phase 6 \\||Phase 10 \\||Completion Audit|PR #494" docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`.
- Staged GitNexus `detect_changes` reported LOW risk: 19 changed documentation
  sections across 2 files, 0 affected processes, and no HIGH/CRITICAL warning.

### Completion Audit Next Action

- Closed. PR #495 merged as `7e94b5ed`; final commit was `020a8ffd
  docs(browser): close runtime supervisor completion audit`. Continue with this
  final tracker sync from current `origin/main`.

---

## Final Tracker Sync Entry Criteria

Final tracker sync can start because:

- PR #495 merged the completion-audit tracker/plan into `main` / `origin/main`;
- fresh reviewer Banach accepted PR #495 after the stale PR/commit tracker
  findings were fixed;
- unrelated PR #496 advanced `origin/main`, so this sync starts from current
  merged-main truth rather than the older #494 or #495 base;
- the remaining discrepancy is only tracker text that still described the
  completion audit as an in-flight PR.

## Final Tracker Sync Progress

- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-final-tracker-sync.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-final-tracker-sync`
- Branch:
  `codex/browser-runtime-final-tracker-sync`
- Scope:
  update tracker merge evidence for PR #495, current-base evidence for PR #496,
  and the final no-more-runtime-phase next action.
- Current PR:
  final tracker-sync PR for `codex/browser-runtime-final-tracker-sync`; GitHub
  is canonical for the PR URL/state once opened.
- Current commit:
  latest pushed head of the final tracker-sync PR; GitHub's PR commit list is
  canonical because reviewer-finding amendments can change the head hash before
  merge.
- Non-goal:
  no Rust, TypeScript, UI, IPC, DB migration, provider promotion, hosted SDK,
  browser worker, identity runtime, payment UI, task-loop behavior, or unrelated
  #496 content change.
- Rollback:
  revert this docs-only PR; Browser Runtime implementation remains merged.

### Final Tracker Sync Impact Notes

- Documentation only.
- No function/class/method/symbol is edited, so pre-edit GitNexus symbol impact
  is not required.
- GitNexus `detect_changes` is required before commit.
- No further Browser Runtime phase is planned after this sync; the next step is
  requirement-by-requirement final verification from `origin/main`.

### Final Tracker Sync Verification Notes

- Passed:
  `git diff --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-final-tracker-sync.md`.
- Passed:
  `rg -n "Current phase|Phase 6 \\||Phase 10 \\||Final Tracker Sync|PR #495|PR #496|No further Browser Runtime phase" docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`.
- Staged GitNexus `detect_changes` reported LOW risk: 18 changed
  documentation sections across 2 files, 0 affected processes, and no
  HIGH/CRITICAL warning.

### Final Tracker Sync Next Action

- Closed. PR #497 merged as `1db7d988`; final commit was `8728adbc
  docs(browser): sync final runtime tracker state`. Continue with this
  docs-only verified closeout so the tracker records the final post-merge test
  evidence and no longer points at a future completion-verification step.

---

## Verified Complete Closeout Entry Criteria

Verified complete closeout can start because:

- PR #497 merged the final tracker sync into `main` / `origin/main`;
- the verified-complete branch was fast-forwarded to the current
  `origin/main` base `52ba4833` after unrelated PR #498 merged;
- all ADR Phase 0-10 implementation/backfill rows in Quick View are closed;
- final focused Browser Runtime verification passed from the post-#497 main
  state;
- the only remaining discrepancy is tracker language that still described final
  verification as a future action.

## Verified Complete Closeout Progress

- Plan:
  `docs/superpowers/plans/2026-05-25-browser-runtime-verified-complete.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-verified-complete`
- Branch:
  `codex/browser-runtime-verified-complete`
- Scope:
  docs-only tracker closeout that records PR #497 merge evidence, final focused
  verification results, and the completed/no-next-phase state.
- Current PR:
  verified-complete PR for `codex/browser-runtime-verified-complete`; GitHub is
  canonical for the PR URL/state once opened.
- Current commit:
  latest pushed head of the verified-complete PR; GitHub's PR commit list is
  canonical because reviewer-finding amendments can change the head hash before
  merge.
- Non-goal:
  no Rust, TypeScript, UI, IPC, DB migration, provider promotion, hosted SDK,
  browser worker, identity runtime, payment UI, task-loop behavior, or worktree
  cleanup.
- Rollback:
  revert this docs-only PR; all Browser Runtime implementation PRs remain
  merged.

### Verified Complete Closeout Impact Notes

- Documentation only.
- No function/class/method/symbol is edited, so pre-edit GitNexus symbol impact
  is not required.
- GitNexus `detect_changes` is required before commit.
- This closeout does not change runtime behavior. It only turns already-run
  `origin/main` evidence into tracker truth.

### Verified Complete Closeout Verification Notes

- Current-main focused runtime-pack verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  returned `42 passed; 0 failed; 0 ignored; 0 measured; 2780 filtered out`.
- Current-main focused runtime/supervisor verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  returned `59 passed; 0 failed; 0 ignored; 0 measured; 2763 filtered out`.
- Current-main focused provider verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
  returned `16 passed; 0 failed; 0 ignored; 0 measured; 2806 filtered out`.
- Current-main focused browser harness adapter verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib harness::adapters::browser`
  returned `19 passed; 0 failed; 0 ignored; 0 measured; 2803 filtered out`.
- Current-main verification emitted only pre-existing warnings outside this
  docs-only closeout scope.
- Docs-only whitespace verification passed:
  `git diff --cached --check -- docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-verified-complete.md`
  returned no output.
- Tracker closeout grep passed:
  `rg -n "Current phase|Verified Complete|PR #497|PR #498|52ba4833|42 passed|59 passed|16 passed|19 passed|No further Browser Runtime phase" docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`.
- GitNexus `detect_changes` initially reported the worktree was not registered;
  `npx gitnexus analyze` registered it, GitNexus auto-edited only
  `AGENTS.md`/`CLAUDE.md` statistics, and those noise changes were restored.
- GitNexus staged `detect_changes` then passed with LOW risk: 2 files, 20
  documentation symbols, 0 affected processes, and no HIGH/CRITICAL warning.
- Fresh review and GitHub merge-state checks remain required before merge.

### Verified Complete Closeout Next Action

- Closed for implementation and verification. While PR #499 is in review,
  GitHub's PR state is canonical for the merge commit; after PR #499 merges,
  no further Browser Runtime phase or tracker follow-up is planned. Future
  browser-runtime work requires a new ADR/spec or a new tracker row outside
  this completed goal.

---

## Per-Phase Closed Loop

### 1. Start

- Read `BEHAVIOR.md`, `CONTEXT.md`, the North Star ADR, and the Browser Runtime ADR.
- Read this status file.
- Use Superpowers workflow skills.
- Create or verify an isolated worktree.
- Record the intended phase row in Quick View.

### 2. Explore

- Use GitNexus context/impact for unfamiliar or existing symbols.
- Use subagents for broad read-only mapping when useful.
- Keep implementation boundaries tied to the ADR phase.
- Do not edit existing functions, classes, or methods before required impact analysis.

### 3. Plan

Each phase plan under `docs/superpowers/plans/` must include:

- ADR Section 18 answers;
- allowed files;
- non-goals;
- impact targets;
- first tests;
- policy hooks;
- rollback path;
- expected verification output;
- this status file update.

### 4. Implement

- Keep the PR narrow and reversible.
- Prefer pure DTOs/adapters before runtime behavior.
- Preserve existing user changes.
- Treat high-attention files as planned, narrow edits; `agentic_loop.rs` and
  `tauri_commands.rs` use normal code discipline and should stay thin.
- Keep tests in sibling `*_tests.rs` files for Rust modules.

### 5. Verify

Minimum before marking a phase ready:

```bash
git diff --check -- <changed-files>
```

Then run the focused tests named in the phase plan.

Before commit:

```bash
npx gitnexus detect-changes --scope staged --repo <phase-worktree>
```

If GitNexus is stale, refresh the index before trusting the report.

### 6. Close

- Update Quick View status and next action.
- Add verification notes.
- Append a Decision Log row if scope or phase order changed.
- Leave a handoff note for the next phase.

---

## Phase 0 Entry Criteria

Phase 0 can start because:

- the Browser Runtime Supervisor ADR is committed locally at `4cb7538` on top
  of latest `main` `d7a9527`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts`;
- existing PR9 BrowserProvider readiness metadata already covers Local Chromium
  setup/probe status;
- Phase 0 is scoped to pure contracts and projection skeleton only.

Recommended Phase 0 first tests:

- safe default feature flags;
- allowed supervisor state transitions;
- provider capability card inventory and default gating;
- browser event name inventory;
- projection attention reason derivation.

## Phase 0 Progress

- Plan:
  `docs/superpowers/plans/2026-05-23-browser-runtime-phase0-contracts.md`
- Worktree:
  `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts`
- Branch:
  `codex/browser-runtime-phase0-contracts`
- Scope:
  pure browser runtime contracts, flags, provider cards, event names, and
  projection DTOs.
- Rust hygiene:
  new tests live in sibling `runtime_contracts_tests.rs`.
- DMZ files:
  none planned.
- Migration:
  none planned.
- Rollback:
  revert `runtime_contracts.rs`, `runtime_contracts_tests.rs`,
  `browser/mod.rs` exports, this status file, and the Phase 0 plan file.

### Phase 0 Impact Notes

- Existing browser execution symbols are intentionally avoided:
  `BrowserContextManager`, `BrowserContext`, `BrowserAgentLoop`, and
  `BrowserService` are not edited.
- Existing `browser/provider.rs` readiness DTOs are reused conceptually but not
  modified in Phase 0.
- `browser/mod.rs` receives additive module exports only.

### Phase 0 Verification Notes

- Baseline bring-up linked ignored local runtime resources from the primary
  worktree because isolated worktrees do not copy `pyembed`, `bunembed`,
  `gbrain-source`, or `ui/node_modules`.
- Initial UI focused baseline before linking `ui/node_modules` failed with
  `vitest: command not found`; this was a worktree dependency issue, not a
  source failure.
- Initial Rust focused baseline before linking `pyembed` failed because Tauri
  build script could not find `src-tauri/pyembed/python`; this was a worktree
  dependency issue, not a source failure.
- Focused contract verification passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_contracts`
  returned `5 passed; 0 failed; 2568 filtered out`.
- Existing provider regression passed:
  `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider`
  returned `6 passed; 0 failed; 2567 filtered out`.
- Formatting and whitespace checks passed for changed files:
  `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_contracts.rs src-tauri/src/browser/runtime_contracts_tests.rs`
  and `git diff --check -- <changed-files>`.

---

## Handoff Notes

- Phase 0 and Phase 1 shell slices are merged to `main` / `origin/main`.
- The next Phase 1 wiring slice should route one low-risk call path through
  `BrowserRuntimeSupervisor`; this can proceed independently from Phase 2 once
  the runtime-pack shell is stable.
- The next Phase 2 slice can add the side-effect boundary for install/repair
  planning, but should still keep actual network download and archive extraction
  behind explicit policy gates.
- Phase 3 startup splash should consume the projection/doctor vocabulary from
  Phase 0 instead of inventing a separate UI state model.
