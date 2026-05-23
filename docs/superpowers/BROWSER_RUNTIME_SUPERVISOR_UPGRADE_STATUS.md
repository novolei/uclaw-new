# Browser Runtime Supervisor Upgrade Status - Single Source of Truth

> Live state for the Browser Runtime Supervisor and Playwright provider
> implementation program.
>
> This file follows the closed-loop pattern from
> `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: every phase PR updates
> this status file so later sessions can resume from the current row instead of
> reconstructing thread history.
>
> Last updated: 2026-05-23 by Codex
> Current phase: Phase 2 runtime-pack manager shell committed
> Source ADR:
> `docs/adr/2026-05-23-browser-runtime-supervisor-playwright-provider.md`

---

## Quick View

| Phase | Theme | Status | Owner Session | Worktree / Branch | Next Action |
|---|---|---|---|---|---|
| Phase 0 | Contracts, flags, and projection skeleton | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase0-contracts` / `codex/browser-runtime-phase0-contracts` | Closed; contract regressions stay in every later browser-runtime phase. |
| Phase 1 | Supervisor around current chromiumoxide runtime | Merged to `main` / `origin/main` | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase1-supervisor` / `codex/browser-runtime-phase1-supervisor` | Closed for shell slice; later wiring slices must use this supervisor surface. |
| Phase 2 | App-managed Playwright runtime pack | Runtime-pack shell committed | Codex | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack` / `codex/browser-runtime-phase2-runtime-pack` | Push/open PR or continue the next Phase 2 installer-planning slice from this committed shell baseline. |
| Phase 3 | Startup Splash, Startup Doctor, and shell UX | Not started | Unassigned | TBD | Wait for Phase 0 projection skeleton and Phase 2 runtime-pack status. |
| Phase 4 | Browser Runtime settings and task-time preparation UX | Not started | Unassigned | TBD | Wait for Phase 2 runtime manager and Phase 3 shell route. |
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

---

## Current Branch Hygiene

| Check | Current Value |
|---|---|
| Primary worktree | `/Users/ryanliu/Documents/uclaw` |
| Current phase worktree | `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase2-runtime-pack` |
| Current phase branch | `codex/browser-runtime-phase2-runtime-pack` |
| Current local base | `81d9b9dc Merge remote-tracking branch 'origin/codex/browser-runtime-phase1-supervisor'` |
| Browser ADR commit on phase branch | Included in merged `origin/main` history. |
| Phase 0 implementation commit | Merged through `origin/main` history as `a24cbc08 feat(browser): add runtime supervisor phase0 contracts`. |
| Phase 1 implementation commit | Merged through `origin/main` history as `bcf823f8 feat(browser): add runtime supervisor phase1 shell`. |
| Phase 2 implementation commit | In progress. |
| Known pre-existing tracked changes | None in the Phase 2 worktree at start. |
| Linked ignored runtime resources | Not linked yet for Phase 2; link `src-tauri/pyembed`, `src-tauri/bunembed`, `src-tauri/gbrain-source`, and `ui/node_modules` only if verification needs them. |
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
- GitNexus staged detect after refreshing the Phase 2 worktree index reported
  `risk_level: low`, `changed_files: 5`, and `affected_processes: []`.
- Phase 2 runtime-pack shell committed on
  `codex/browser-runtime-phase2-runtime-pack` as current `HEAD`:
  `feat(browser): add runtime pack manager shell`.

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
