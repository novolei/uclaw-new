# Browser Runtime Supervisor Upgrade Status - Single Source of Truth

> Live state for the Browser Runtime Supervisor and Playwright provider
> implementation program.
>
> This file follows the closed-loop pattern from
> `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: every phase PR updates
> this status file so later sessions can resume from the current row instead of
> reconstructing thread history.
>
> Last updated: 2026-05-24 by Codex
> Current phase: Phase 4E Browser Runtime task-time prompt UI in progress
> Source ADR:
> `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

---

## Quick View

| Phase | Theme | Status | Owner Session | Worktree / Branch | Next Action |
|---|---|---|---|---|---|
| Phase 0 | Contracts, flags, and projection skeleton | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts` / `codex/browser-runtime-phase0-contracts` | Closed; contract regressions stay in every later browser-runtime phase. |
| Phase 1 | Supervisor around current chromiumoxide runtime | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor` / `codex/browser-runtime-phase1-supervisor` | Closed for shell slice; later wiring slices must use this supervisor surface. |
| Phase 2 | App-managed Playwright runtime pack | Runtime-pack shell through Phase 2F executor boundary merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2f-executor-boundary` / `codex/browser-runtime-phase2f-executor-boundary` | Closed for no-side-effect runtime-pack boundary; real filesystem/network adapters remain future scoped work. |
| Phase 3 | Startup Splash, Startup Doctor, and shell UX | Phase 3A-3C and 3E-3H merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase3h-app-startup-route` / `codex/browser-runtime-phase3h-app-startup-route` | Closed for branded root startup route; later recovery/deep-link work must build on the merged Startup Splash route. |
| Phase 4 | Browser Runtime settings and task-time preparation UX | Phase 4A-4D merged; Phase 4E task-time prompt UI in progress | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4e-task-time-prompt-ui` / `codex/browser-runtime-phase4e-task-time-prompt-ui` | Finish additive task-time prompt UI before IPC, deep links, TaskEvents, or checkpoint writes. |
| Phase 5 | Playwright CLI thin lane behind a feature flag | Not started | Unassigned | TBD | Wait for Phase 2 runtime pack and Phase 1 supervisor. |
| Phase 6 | Browser identity authorization and profile UX | Not started | Unassigned | TBD | Wait for supervised isolated-profile baseline. |
| Phase 7 | Playwright MCP sidecar behind a feature flag | Not started | Unassigned | TBD | Wait for provider contract and runtime pack policy. |
| Phase 8 | Provider abstraction, parity harness, and default selection | Not started | Unassigned | TBD | Wait for chromiumoxide, CLI, and MCP lanes. |
| Phase 9 | Recipes, locator cache, and domain-skill candidates | Not started | Unassigned | TBD | Wait for observable provider behavior and harness scorecards. |
| Phase 10 | Optional hosted providers and hard-site escape hatches | Not started | Unassigned | TBD | Wait for local-first provider routing and policy prompts. |

---

## Live Decision Log

| Date | Decision | Evidence | Effect |
|---|---|---|---|
| 2026-05-23 | Implement Browser Runtime Supervisor as phased PR slices, not one broad rewrite. | Browser Runtime Supervisor ADR section 12 and user request for phase-pack execution. | Each phase gets a plan, status row, verification notes, and reversible commit boundary. |
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

---

## Current Branch Hygiene

| Check | Current Value |
|---|---|
| Primary worktree | `/Users/ryanliu/Documents/uclaw` |
| Current phase worktree | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase4e-task-time-prompt-ui` |
| Current phase branch | `codex/browser-runtime-phase4e-task-time-prompt-ui` |
| Current local base | `7d4f70e0 Merge pull request #430 from novolei/codex/browser-runtime-phase4d-task-time-prompt-model` |
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
| Phase 4E implementation commit | Current PR #431 on `codex/browser-runtime-phase4e-task-time-prompt-ui`; implementation is the branch tip commit. |
| Known pre-existing tracked changes | None in the Phase 4E worktree at start. Primary worktree has unrelated untracked Tauri IPC docs/reports that are preserved and not copied into this worktree. |
| Linked ignored runtime resources | Phase 4E may use ignored local `ui/node_modules` from the primary worktree for Vitest verification. Rust resource links may be added only if the default browser-runtime regressions need them. |
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
  [#431](https://github.com/novolei/uclaw-new/pull/431).
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
  not opened yet.
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
  not opened yet.
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
  not opened yet.
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
- Avoid DMZ files unless explicitly planned.
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
