# UCLAW Agent Autonomy Rollout Tracker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `docs/superpowers/specs/2026-05-19-uclaw-agent-autonomy-harness-design.md` into an ordered PR program with measurable verification after every merge, so Browser autonomy, Agent Loop evaluation, Memory/gbrain learning, Skills, Automation, Permissions, Hooks, Tools, Plugins, Coordinator, and Prompts advance through one accountable harness track.

**Architecture:** Keep PR #238's `src-tauri/src/harness/*` runtime core as the canonical evaluation substrate. Keep PR #240 and PR #241's browser identity broker and task-start injection as the Browser identity baseline. Every follow-up PR must either add a subject adapter, a browser autonomy capability, a harness reporting surface, or a promotion gate, and must append its result to the rollout ledger in this document or a successor track file.

**Tech Stack:** Rust/Tauri v2, React, Playwright-compatible browser state, existing `src-tauri/src/harness/*`, existing `src-tauri/src/browser/*`, existing memory and `gbrain` subsystems.

---

## Baseline Already Landed

| PR | Status | Scope | Verification Evidence |
| --- | --- | --- | --- |
| #238 `feat(harness): add universal runtime core` | Merged | Generic harness case, episode, trace, artifact, grader, adapter, and runtime modules. | `src-tauri/src/harness/{case,episode,trace,artifacts,graders,adapters,runtime}.rs` exist and are exported from `src-tauri/src/harness/mod.rs`. |
| #240 `feat(browser): add auth profile broker` | Merged | Browser identity broker for auth profile metadata and storage-state based identity handles. | Browser identity module present under `src-tauri/src/browser/identity/*`. |
| #241 `feat(browser): apply auth profiles at task startup` | Merged | `browser_task` and resume startup can resolve and inject auth profile state into browser contexts. | Main is at `feat(browser): apply auth profiles at task startup (#241)`. |

These three PRs are treated as the starting line. Do not recreate them.

---

## Rollout Rules

- [ ] Each implementation PR updates a track row before merge.
- [ ] Each PR has at least one automated verification command unless the PR is documentation-only.
- [ ] Browser PRs must include a manual smoke recipe when behavior depends on real websites, real profiles, or user intervention.
- [ ] Memory/gbrain PRs must verify both ordinary Memory System behavior and gbrain entity/page behavior.
- [ ] Sensitive auth material, cookies, bearer tokens, CAPTCHA images, and passwords must never be committed, logged as plaintext, or rendered in ordinary chat traces.
- [ ] CAPTCHA automation remains allowlist-only. Default third-party CAPTCHA behavior is detection, boundary event, ask_user, checkpoint, and resume.
- [ ] If a PR changes agent autonomy, it must explain what failure looks like and where the episode/artifact/trace proves it.

---

## PR Queue

### PR-244: `feat(browser): add visual perception adapter`

**Goal:** Add the browser visual perception provider seam without binding the browser loop to one OCR/VLM implementation.

**Primary files:**

- `src-tauri/src/browser/perception/mod.rs`
- `src-tauri/src/browser/perception/provider.rs`
- `src-tauri/src/browser/perception/sidecar.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/observation.rs`

**Implementation tasks:**

- [ ] Define `VisualPerceptionProvider`, `OcrTextBox`, `VisualControlCandidate`, and `VisualObservation`.
- [ ] Add a mock provider for deterministic unit tests.
- [ ] Attach visual observation metadata to browser observation without making OCR mandatory.
- [ ] Make provider failures degrade to DOM-only observations.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::perception --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::agent_loop --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Run a browser task against a local fixture with text only visible in screenshot space; confirm the trace includes visual observation artifacts when the mock/provider is enabled and still succeeds with provider disabled.

### PR-245: `feat(browser): add challenge boundary broker v2`

**Goal:** Promote login, password, 2FA, CAPTCHA, payment, privacy, and stale-auth states into structured boundary events that bridge to ask_user/checkpoint/resume.

**Primary files:**

- `src-tauri/src/browser/boundary.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/intervention_bridge.rs`
- `src-tauri/src/browser/checkpoint.rs`
- `src-tauri/src/agent/tool_call.rs`
- `ui/src/components/agent/*Tool*`

**Implementation tasks:**

- [ ] Add `BrowserBoundaryKind` and `BrowserBoundaryEvent`.
- [ ] Classify password fields, login-required pages, CAPTCHA indicators, payment forms, and stale auth profile probes.
- [ ] Route user-facing boundaries through the existing ask_user tool-call UI, not a separate one-off banner path.
- [ ] Persist checkpoint references so tasks can resume from the same browser state after user action.
- [ ] Add allowlist-only CAPTCHA automation policy hooks, with default behavior set to ask_user.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::boundary --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::intervention_bridge --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::agent_loop --lib
```

**Manual smoke:** Run a task against `https://the-internet.herokuapp.com/login`; verify a password boundary produces an ask_user tool-call record in chat, the browser task pauses, user response is recorded, and resume continues from the same page state.

### PR-246 / GitHub #247: `feat(browser): add memory and gbrain adapter`

**Goal:** Persist browser task checkpoint, boundary, auth profile selection, and visual observation events into the long-term agent memory path through both the Memory System and gbrain.

**Primary files:**

- `src-tauri/src/browser/memory_adapter.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/tools.rs`
- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/memory.rs`
- `src-tauri/src/mcp.rs`

**Implementation tasks:**

- [x] Add browser long-term memory adapter that writes structured events into `MemoryStore`.
- [x] Add gbrain writer that uses the existing connected MCP `put_page` path without holding the MCP manager lock across the tool call.
- [x] Record auth profile application, visual observation summaries, boundary events, checkpoints, and final task state.
- [x] Strip raw screenshot base64 before memory/gbrain writes.
- [x] Wire the adapter into `browser_task`, `browser_task_resume`, and `retry_with_browser_agent`.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::memory_adapter --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::agent_loop --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Run one `browser_task` with an auth profile and one task that reaches a login/CAPTCHA boundary; verify `memory_search namespace=browser_task` finds the run events, and `mcp__gbrain__recall` can retrieve the generated `browser-tasks/*` pages.

### PR-248: `feat(harness): add browser parity suite`

**Goal:** Make browser-use parity measurable through harness cases instead of subjective inspection.

**Primary files:**

- `src-tauri/src/harness/adapters/browser.rs`
- `src-tauri/src/harness/cases/browser/*.json`
- `src-tauri/src/harness/graders.rs`
- `src-tauri/src/browser/tools.rs`
- `docs/superpowers/reports/browser-parity-scorecard.md`

**Implementation tasks:**

- [ ] Add browser harness adapter that can run deterministic cases with browser task tools.
- [ ] Add cases for navigation, multi-tab planning, file upload, auth profile restore, boundary detection, checkpoint resume, and long task recovery.
- [ ] Add graders for task success, action count, active tab correctness, boundary precision, and checkpoint resume success.
- [ ] Emit a scorecard artifact after each suite run.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::browser --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Run the browser parity suite and attach the generated scorecard path to the PR body.

### PR-249: `feat(harness): add memory and gbrain eval adapters`

**Goal:** Evaluate both Memory System and gbrain through one memory harness target model.

**Primary files:**

- `src-tauri/src/harness/adapters/memory.rs`
- `src-tauri/src/harness/adapters/gbrain.rs`
- `src-tauri/src/harness/trace.rs`
- `src-tauri/src/memory.rs`
- `src-tauri/src/memory_graph/*`
- `src-tauri/src/mcp.rs`

**Implementation tasks:**

- [ ] Add `MemoryEvalProbe` and `MemoryEvalResult` types.
- [ ] Add Memory System write/recall probes.
- [ ] Add gbrain entity/page probes using the existing gbrain MCP path.
- [ ] Score recall precision, coverage, hallucinated facts, stale facts, entity consistency, page grounding, and correction adoption.
- [ ] Ensure every memory write/recall event is represented in harness traces without duplicating truth stores.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::gbrain --lib
cargo test --manifest-path src-tauri/Cargo.toml memory --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Write a user preference, recall it through the chat agent, verify the harness records both Memory System and gbrain results, then apply a correction and verify stale facts score as failures.

### PR-250: `feat(harness): add agent loop tools permissions hooks adapters`

**Goal:** Cover the main agent loop and control-plane surfaces with harness adapters.

**Primary files:**

- `src-tauri/src/harness/adapters/agent_loop.rs`
- `src-tauri/src/harness/adapters/tools.rs`
- `src-tauri/src/harness/adapters/permissions.rs`
- `src-tauri/src/harness/adapters/hooks.rs`
- `src-tauri/src/agent/*`
- `src-tauri/src/mcp.rs`

**Implementation tasks:**

- [ ] Convert model turns, tool calls, tool results, permission requests, hook executions, and tool crashes into harness events.
- [ ] Add stuck-loop, recovery-after-tool-error, permission-correctness, and final-answer-groundedness graders.
- [ ] Add deterministic tool-failure fixture proving frontend running state does not desynchronize from backend work.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::agent_loop --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::tools --lib
cargo test --manifest-path src-tauri/Cargo.toml agent:: --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Trigger a controlled failing tool call; verify the UI still shows the session as running until the backend run actually finishes, and verify the harness episode contains both the failure and recovery.

### PR-251: `feat(harness): add dashboard and report commands`

**Goal:** Make harness episodes, scorecards, traces, and artifacts visible and runnable from the app.

**Primary files:**

- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/harness/runtime.rs`
- `ui/src/components/harness/HarnessDashboard.tsx`
- `ui/src/components/harness/HarnessEpisodeView.tsx`
- `ui/src/components/harness/HarnessScorecard.tsx`
- `ui/src/lib/tauri-bridge.ts`

**Implementation tasks:**

- [ ] Add Tauri commands to list cases, run cases, list episodes, open artifacts, and export scorecards.
- [ ] Add a Harness dashboard surface with subject filters and latest verdicts.
- [ ] Add trace detail view with model/tool/permission/boundary/memory/checkpoint event grouping.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness:: --lib
cargo check --manifest-path src-tauri/Cargo.toml
pnpm --dir ui test -- --run
pnpm --dir ui typecheck
```

**Manual smoke:** Run one browser case and one memory case from the dashboard; verify both episodes show trace and artifact links.

### PR-252: `feat(autonomy): add skill prompt automation promotion gates`

**Goal:** Let failed episodes become learning candidates without silently mutating production memory, prompts, hooks, or skills.

**Primary files:**

- `src-tauri/src/harness/adapters/skills.rs`
- `src-tauri/src/harness/adapters/prompts.rs`
- `src-tauri/src/harness/adapters/tasks.rs`
- `src-tauri/src/harness/adapters/coordinator.rs`
- `src-tauri/src/skills.rs`
- `src-tauri/src/automation/*`

**Implementation tasks:**

- [ ] Add learning candidate artifacts generated from failed or partial episodes.
- [ ] Add promotion gates for skill extraction, prompt revisions, automation memory promotion, and hook changes.
- [ ] Require passing regression cases before promoting a candidate.
- [ ] Add rollback metadata for every promoted candidate.

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::skills --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::prompts --lib
cargo test --manifest-path src-tauri/Cargo.toml automation:: --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

**Manual smoke:** Force a browser failure that suggests a skill candidate; verify the candidate is generated, blocked before promotion, promoted only after passing its regression case, and rollback metadata is visible.

---

## Track Ledger

Update this table after every PR is implemented, verified, merged, and synced.

Numbering caveat: rows named `#248` through `#252` are planned autonomy harness slice labels. The corresponding implementation landed in GitHub PR #282, not GitHub PRs #248-#252. GitHub PR #285 then connected the remaining harnesses to the System Diagnostics frontend.

| PR | Branch | Merge Commit | Subject | Verification Commands | Manual Smoke | Verdict | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| #238 | merged | `e2d8e85` | harness core | Existing harness tests from PR. | N/A | Pass | Baseline runtime core exists. |
| #240 | merged | `0c5829f` | browser identity | Existing browser identity tests from PR. | N/A | Pass | Auth profile broker baseline exists. |
| #241 | merged | `5c3eedf` | browser identity startup | `browser::identity`, `browser::agent_loop`, `browser::context_manager`, `cargo check` from PR run. | Auth state can be selected for browser task startup. | Pass | Local `main` synced after merge. |
| #242 | merged | `7e2a56a` | rollout tracker | `rg -n "PR-244\\|Memory System and gbrain\\|Track Ledger\\|Immediate Next Step\\|CAPTCHA automation remains allowlist-only" docs/superpowers/plans/2026-05-19-uclaw-agent-autonomy-rollout-tracker.md` | N/A | Pass | Local `main` synced after merge. |
| #244 | merged | `9ea01b6` | browser perception | `browser::perception`, `browser::observation`, `browser::agent_loop`, `cargo check` | Not run; production OCR sidecar is deferred, provider seam is covered by mock/no-op tests. | Pass | Local `main` synced after merge. |
| #245 | merged | `21a3f19` | browser boundary | `browser::boundary`, `browser::intervention_bridge`, `browser::agent_loop`, `cargo check` | Not run; covered by deterministic CAPTCHA/login/visual CAPTCHA/stale-auth unit tests. | Pass | Local `main` synced after merge. Expands boundary detection into structured events with evidence, recommended action, and resume metadata. |
| #247 | merged | `6e51961` | browser memory/gbrain adapter | `browser::memory_adapter`, `browser::agent_loop`, `cargo check` | Pending. | Pass | GitHub #246 was already consumed by a merged dock PR; this implements planned PR-246 as GitHub #247. Wires browser task auth profile, visual observation, boundary, checkpoint, and final-state events into MemoryStore and gbrain MCP `put_page`; raw screenshot base64 is stripped before long-term writes. |
| PR-248 | merged via GitHub #282 | `da0bb48` | browser parity harness | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::browser --lib`; `cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Deterministic fixture suite; live browser smoke deferred. | Pass | Adds browser-use-aligned scorecards over navigation, multi-tab, file upload, auth profile restore, human boundary, checkpoint resume, and recovery. Backend can run through `BrowserAgentLoop`; frontend Browser button added later in GitHub #285 uses deterministic `BrowserFixtureParityExecutor`. |
| PR-249 | merged via GitHub #282 | `da0bb48` | memory/gbrain eval harness | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory --lib`; `cargo test --manifest-path src-tauri/Cargo.toml memory_gbrain_eval_harness_command_tests --lib`; `cargo test --manifest-path src-tauri/Cargo.toml harness::memory_inventory --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Explicit eval command writes harness-namespaced probe facts only when invoked. | Pass | Converts inventory smoke plus live write/recall probe into app-native scorecards; distinguishes unavailable, reachable-empty, and hallucinated recall. |
| PR-250 | merged via GitHub #282 | `da0bb48` | agent loop control-plane harness | `cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::agent_loop --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `git diff --check` | Deterministic trace fixtures. | Pass | Normalizes model turns, tool call/result pairing, permission boundaries, checkpoints, and non-running final status into scorecards. |
| PR-251 | merged via GitHub #282; expanded via GitHub #285 | `da0bb48`, `c1a57cf` | harness UI/reporting | `npm test -- --run src/components/settings/SystemTab.test.tsx`; `npm run build`; `git diff --check` | System Diagnostics exposes `All`, `Browser`, `Memory`, `Agent`, and `Self`. | Pass | Initial UI exposed Memory and Agent. GitHub #285 added Browser parity and Self controls plus sequential All runner. Browser UI path is deterministic parity fixture validation, not live arbitrary browsing. |
| PR-252 | merged via GitHub #282; frontend exposed via GitHub #285 | `da0bb48`, `c1a57cf` | self-improvement gates | `cargo test --manifest-path src-tauri/Cargo.toml harness::self_improvement --lib`; `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`; `npm test -- --run src/components/settings/SystemTab.test.tsx`; `git diff --check` | Self gate candidates render in System Diagnostics scorecards. | Pass | Adds deterministic promotion/hold/reject checks for memory, gbrain, skills, prompts, and hooks; reject is treated as a valid decided safety outcome in the UI. |

---

## Per-PR Completion Template

Each PR body should include this block:

```markdown
## Track Result

- Plan row: PR-___ in `docs/superpowers/plans/2026-05-19-uclaw-agent-autonomy-rollout-tracker.md`
- Subject:
- What changed:
- Automated verification:
  - [ ] command:
  - [ ] result:
- Manual smoke:
  - [ ] scenario:
  - [ ] result:
- Harness artifact / scorecard:
- Known residual risk:
```

---

## Immediate Next Step

After GitHub PR #285, the next documentation-aligned implementation target is persisted harness history plus a separate live Browser smoke harness. This keeps the current deterministic scorecard surface honest while adding a bounded real browser-task regression path.

Reason: the harness substrate, browser identity, perception seam, boundary broker, memory/gbrain adapter, Browser parity suite, memory/gbrain eval suite, agent control-plane suite, UI reporting, and self-improvement gates are now implemented. The remaining gap is observability over time and one bounded live-browser regression path, so future failures can be distinguished between scorecard contract drift and real browser-task execution drift.
