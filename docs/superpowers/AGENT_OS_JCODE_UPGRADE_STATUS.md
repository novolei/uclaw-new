# Agent OS jcode Absorption Status — Single Source of Truth

> Live state for the project-level uClaw Agent OS upgrade inspired by
> `/Users/ryanliu/Documents/jcode`.
>
> This file follows the closed-loop pattern from
> `docs/superpowers/MILESTONE_STATUS.md`: every PR updates this status file,
> then later sessions can resume from the current row instead of reconstructing
> the entire thread.
>
> Last updated: 2026-05-23 by Codex
> Current phase: PR-4 soft interrupts and boundary yields open for review
> Current source package: `docs/jcode_comparison/` +
> `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`

---

## Quick View

| PR | Theme | Status | Owner Session | Next Action |
|---|---|---|---|---|
| PR-0 | Design baseline and close-loop governance | Merged | Codex | Baseline docs are on `main`; local skill/context follow-up also landed on `main`. |
| PR-1 | Pure type crates for messages/tools/protocol/runtime contracts | Merged | Codex | GitHub PR #399 merged at `efe0e72d`. |
| PR-2 | ToolContext adapter | Merged | Codex | GitHub PR #400 merged at `17ec931e`. |
| PR-3 | Provider readiness core | Merged | Codex | GitHub PR #401 merged at `9af769c1`. |
| PR-4 | Soft interrupts and boundary yields | Open | Codex | GitHub PR #402 is open; review, merge, then sync local `main`. |
| PR-5 | Session projection journal | Not started | Unassigned | Wait for PR-1 contracts and M4 alignment. |
| PR-6 | Performance scorecards | Not started | Unassigned | Wait for first replay fixtures. |
| PR-7 | Subagent/team runtime hardening | Not started | Unassigned | Wait for PR-1 contracts and PR-4 boundary semantics. |
| PR-8 | jcode-inspired tool family mesh | Not started | Unassigned | Wait for PR-2 and Capability Mesh status. |
| PR-9 | BrowserProvider status/setup/probe | Not started | Unassigned | Wait for PR-1 contracts and browser impact map. |
| PR-10 | Ambient-to-automation mapping | Not started | Unassigned | Wait for PR-4 and automation policy review. |
| PR-11 | Harness campaigns | Not started | Unassigned | Wait for PR-2, PR-6, PR-9 smoke subjects. |
| PR-12 | Frontend projection reducer | Not started | Unassigned | Wait for PR-5 projection journal. |
| PR-13 | Surface convergence | Not started | Unassigned | Wait for PR-12 plus per-surface migration plans. |

---

## Live Decision Log

Append one row when a design decision changes the roadmap.

| Date | Decision | Evidence | Effect |
|---|---|---|---|
| 2026-05-23 | uClaw absorbs jcode as runtime-component reference, not as a second control plane. | ADR Agent OS v2 + `docs/jcode_comparison/06_adr_gap_audit_and_reference_addenda.md`. | All PRs must strengthen `IntentSpec -> TaskSpec -> TaskEvent -> WorldProjection -> Harness`. |
| 2026-05-23 | uClaw Browser Agent v2 remains the primary browser stack; jcode browser contributes readiness/setup/probe ergonomics. | jcode/uClaw browser comparison in the ADR gap audit. | Browser work starts as BrowserProvider status/probe, not browser replacement. |
| 2026-05-23 | jcode ambient maps into automation/scheduled workers, not a second scheduler. | ADR gap audit ambient section. | PR-10 must preserve automation and heartbeat ownership. |
| 2026-05-23 | Every PR uses Superpowers workflow. | User direction on 2026-05-23. | Each PR starts with `superpowers:using-superpowers`; implementation PRs need a plan. |
| 2026-05-23 | Corrected PR-1 numbering drift: PR-1 is pure type crate extraction, not event spine validation. | `docs/jcode_comparison/README.md` listed PR-1 as type extraction. | Event spine validation moves behind the type-crate foundation. |
| 2026-05-23 | Adopted jcode-style Rust test/module hygiene for uClaw PR-1. | User reference screenshots show sibling `*_tests.rs` modules loaded via `#[path = "..."] mod tests;`. | PR-1 crates must use sibling test files and avoid god files through focused module boundaries. |
| 2026-05-23 | PR-1 implementation uses compatibility re-exports, not call-site churn. | Commits `c85f4c1c`, `5fdc1e4b`, `a4428a71`, `8b5602e9`, `160d6491`. | Later PRs can migrate imports gradually while existing backend modules keep compiling against the facade paths. |
| 2026-05-23 | PR-2 uses a compatibility ToolContext adapter, not a `Tool::execute` signature rewrite. | GitNexus marks `Tool` HIGH impact with 28 direct implementers. | PR-2 keeps behavior stable and introduces a context seam for later tool migration. |
| 2026-05-23 | PR-3 starts with provider readiness/core metadata, not runtime split-prompt execution. | GitNexus marks `get_active_llm_config` HIGH risk; subagents agreed `LlmProvider`/OpenAI/Anthropic runtime paths should not change in PR-3. | PR-3 adds typed readiness reports and leaves split prompt, runtime failover, and provider trait migration to later PRs. |
| 2026-05-23 | PR-4 starts as an adapter/foundation slice, not an agent-loop rewrite. | GitNexus marks `run_agentic_loop` HIGH risk; jcode soft interrupt design can be adopted without changing the loop signature first. | PR-4 adds soft interrupt queue primitives and normalizes resumable boundaries into existing `TaskEvent` variants. |

---

## Current Branch Hygiene

This section is intentionally manual. Update it before opening or reviewing a
PR.

| Check | Current Value |
|---|---|
| Primary worktree | `/Users/ryanliu/Documents/uclaw` |
| Known pre-existing tracked changes | None in PR-4 worktree after restoring GitNexus auto-updated `AGENTS.md` and `CLAUDE.md` stats. |
| Current jcode comparison docs | `docs/jcode_comparison/` is tracked on `main`. |
| Current PR-0 spec | `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md` |
| Nested repo caveat | `/Users/ryanliu/Documents/uclaw/ulooi` is a separate git root; do not mix status or commits. |

---

## Per-PR Closed Loop

Every PR in this upgrade program follows this loop.

### 1. Start

- Read `BEHAVIOR.md`, `CONTEXT.md`, and the ADR.
- Read this status file.
- Use `superpowers:using-superpowers`.
- For design/spec PRs, use `superpowers:brainstorming`.
- For implementation PRs, write a plan under `docs/superpowers/plans/`.
- Record the intended PR row in the Quick View table.

### 2. Explore

- Use GitNexus query/context for unfamiliar code.
- For broad reads, use subagents where available.
- Keep a short evidence list in the plan or PR notes.
- Do not edit symbols until impact analysis is complete.

### 3. Plan

The implementation plan must include:

- ADR Section 18 answers;
- allowed files;
- symbol impact targets;
- tests to write first;
- policy hooks;
- rollback path;
- expected verification output;
- this status file update.

### 4. Implement

- Keep the PR narrow.
- Prefer adapters over rewrites.
- Preserve existing user changes.
- Avoid DMZ files unless the plan says why.
- Commit in small independently verifiable slices.

### 5. Verify

Minimum verification before marking a PR ready:

```bash
git diff --check -- <changed-files>
```

Then add the relevant runtime tests, for example:

```bash
cargo test -p uclaw_core -- <filter>
cd ui && npm test -- --run
```

Before commit:

```bash
gitnexus detect-changes
```

If GitNexus is stale, refresh the index before trusting the report.

### 6. Close

After merge or after the PR is declared ready:

- update the PR row in Quick View;
- update Last updated;
- append a Decision Log row if the roadmap changed;
- add drift notes if the PR was tactical or blocked;
- link closeout report for milestone-closing PRs;
- leave a handoff note if another session should continue.

---

## Drift Rules

Use these alarms to keep the upgrade from drifting into scattered fixes.

| Signal | Green | Yellow | Red | Required Action |
|---|---|---|---|---|
| Consecutive tactical PRs | 0-2 | 3-5 | > 5 | Pause tactical work and land the next roadmap PR. |
| Planned PR idle time | < 7 days | 7-14 days | > 14 days | Reconfirm next action and update Quick View. |
| Pilot without wire-up | < 14 days | 14-30 days | > 30 days | Either schedule wire-up or archive the pilot. |
| HIGH/CRITICAL impact PRs without reviewer | 0 | 1 pending | any merged | Stop and run writer/reviewer review. |
| Status file not updated after merge | 0 PRs | 1 PR | > 1 PR | Update status before starting new work. |

---

## PR-0 Checklist

- [x] jcode comparison package exists under `docs/jcode_comparison/`.
- [x] ADR gap audit covers tools, browser, ambient, harness, subagents, teams.
- [x] PR-0 design spec exists under `docs/superpowers/specs/`.
- [x] Close-loop status file exists.
- [x] User reviewed and approved PR-0 design baseline.
- [x] PR-1 implementation plan is written after approval.

---

## PR-1 Entry Criteria

PR-1 can start only when:

- PR-0 spec is reviewed;
- this status file is accepted as the live coordination source;
- current branch/worktree strategy is chosen;
- allowed files are listed in the PR-1 plan;
- GitNexus impact targets are identified;
- test fixtures are defined before behavior changes.

Recommended PR-1 first tests:

- serde wire-shape test for `ChatMessage` in `crates/uclaw-message-types/src/message_tests.rs`;
- serde wire-shape test for `ToolCall` and `ToolDefinition` in `crates/uclaw-tool-types/src/tool_tests.rs`;
- serde round-trip tests for `IntentSpec`, `TaskSpec`, and `TaskEvent` in `crates/uclaw-runtime-contracts/src/contracts_tests.rs`;
- compile regression tests for existing provider, rollout, browser, automation, and harness modules.

## PR-1 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr1-plan`
- Branch: `codex/agent-os-jcode-pr1-plan`
- Scope: extract `uclaw-message-types`, `uclaw-tool-types`, `uclaw-runtime-contracts`, and `uclaw-protocol-types`.
- Rust hygiene: sibling `*_tests.rs` files only; no substantial inline test module blocks in production modules.
- DMZ files: root `Cargo.toml` touched; writer/reviewer required before merge.
- Migration: none planned.
- Rollback: revert crate additions, dependency additions, and compatibility re-export facades.

### PR-1 Implementation Commits

| Commit | Slice | Review Status | Verification |
|---|---|---|---|
| `c85f4c1c` | `uclaw-message-types` | Spec approved; quality approved after import-order fix. | `cargo test -p uclaw-message-types` passed 3 tests; `src-tauri` focused tests blocked by missing ignored runtime resources. |
| `5fdc1e4b` | `uclaw-tool-types` | Spec approved; quality approved. | `cargo test -p uclaw-tool-types` passed 2 tests; `cargo check -p uclaw` blocked by missing `pyembed/python`. |
| `a4428a71` | `uclaw-runtime-contracts` | Spec approved; quality approved. | `cargo test -p uclaw-runtime-contracts` passed 20 tests; `cargo check -p uclaw --lib` blocked by missing `gbrain-source`. |
| `8b5602e9` | `uclaw-protocol-types` | Spec approved; quality approved. | `cargo test -p uclaw-protocol-types` passed 2 tests. |
| `160d6491` | harness `TaskEventSource` bridge compatibility | GitNexus risk low; 0 affected processes. | `cargo check -p uclaw --lib` passed; `cargo test harness::case --lib` passed 3 tests. |

### PR-1 Verification Notes

- GitNexus marked `ChatMessage` as CRITICAL impact before Task 1; Ryan explicitly confirmed continuing with PR-1.
- GitNexus `detect-changes` can warn or omit `risk_level` in this sibling worktree because the indexed repo path is `/Users/ryanliu/Documents/uclaw`; commit hooks surfaced that caveat on each commit.
- Full `cargo check -p uclaw --lib` passed after linking ignored local runtime resources (`pyembed/python`, `bunembed/bun`, `gbrain-source`) from the primary worktree into this isolated worktree.
- `cargo test -p uclaw-message-types -p uclaw-tool-types -p uclaw-runtime-contracts -p uclaw-protocol-types` passed 27 unit tests plus doctests.
- `cd src-tauri && cargo test agent::types --lib` passed 17 tests.
- `cd src-tauri && cargo test channels::dispatcher --lib` passed 19 tests.

---

## PR-2 Entry Criteria

PR-2 can start because:

- PR-1 type crates exist on branch `codex/agent-os-jcode-pr1-plan`;
- GitHub PR #399 is open and mergeable;
- the new worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr2-tool-context`;
- jcode ToolContext and uClaw tool execution chain were explored by subagents;
- GitNexus impact for `Tool` was run and reported HIGH risk.

Recommended PR-2 first tests:

- sibling-file tests for `ToolExecutionContext::for_subcall`;
- sibling-file tests for `ToolExecutionContext::resolve_candidate_path`;
- pass-through test for `execute_tool_with_context`;
- focused dispatcher/headless compile regression tests.

## PR-2 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr2-tool-context-adapter.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr2-tool-context`
- Branch: `codex/agent-os-jcode-pr2-tool-context`
- Scope: introduce `ToolExecutionContext` and a compatibility execution helper
  without changing `Tool::execute(params)`.
- Rust hygiene: move touched `tool.rs` tests to sibling `tool_tests.rs`; no new
  inline production-file test modules.
- DMZ files: none planned.
- Migration: none planned.
- Rollback: revert context/helper additions and switch dispatcher/headless calls
  back to direct `tool.execute(params)`.

### PR-2 Impact Notes

- `Tool`: HIGH impact; 28 direct implementers.
- `LoopDelegate::execute_tool_calls`: MEDIUM impact; avoid changing signature.
- `ChatDelegate::execute_tool_calls`: main runtime hot path; adapter must be
  behavior-preserving.
- New PR-2 worktree is not yet a GitNexus indexed repo path, so impact was
  checked against the indexed PR-1 worktree baseline.

### PR-2 Verification Notes

- `cargo test -p uclaw --lib agent::tools::tool` passed 7 tests.
- `cd src-tauri && cargo test agent::dispatcher --lib` passed 43 tests.
- `cd src-tauri && cargo test agent::headless --lib` passed compilation with
  0 matching tests.
- `cargo check -p uclaw --lib` passed with existing warnings only.
- `git diff --check` passed.
- `npx gitnexus analyze` indexed the PR-2 worktree; it auto-touched
  `AGENTS.md` and `CLAUDE.md`, which were restored.
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr2-tool-context`
  reported low risk, 0 affected processes.

---

## PR-3 Entry Criteria

PR-3 can start because:

- PR-1 and PR-2 are merged into `main`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr3-provider-core`;
- jcode provider-core and uClaw provider/LLM architecture were explored by
  subagents;
- GitNexus impact for `get_active_llm_config` was run and reported HIGH risk;
- PR-3 is scoped to metadata/readiness and avoids runtime provider trait
  migration.

Recommended PR-3 first tests:

- serde round-trip tests for `ProviderReadinessReport`;
- readiness precedence tests for credentials, base URL, model selection, probe
  failures, and unknown providers;
- provider adapter tests for API-family and credential-status mapping;
- `ProviderService::provider_readiness` test proving reports do not expose API
  keys.

## PR-3 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr3-provider-readiness-core.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr3-provider-core`
- Branch: `codex/agent-os-jcode-pr3-provider-core`
- Scope: add `uclaw-provider-core` plus a provider readiness adapter over
  existing provider configs; no runtime LLM behavior changes.
- Rust hygiene: new tests live in sibling `provider_tests.rs` and
  `readiness_tests.rs`; no substantial inline test additions.
- DMZ files: root `Cargo.toml` is touched only to add the new workspace member.
- Migration: none planned.
- Rollback: revert the new crate, Cargo wiring, readiness adapter, helper
  methods, and docs.

### PR-3 Impact Notes

- `get_active_llm_config`: HIGH impact; do not change signature or behavior.
- `create_provider`: practical high risk due many runtime call sites; avoid in
  PR-3.
- `LlmProvider`: practical high risk despite low/ambiguous index signal; avoid
  trait changes.
- `ProviderService`: acceptable for additive helper methods only.
- `ProviderConfig`/`ProviderConfigs`: do not change persisted JSON shape.

### PR-3 Verification Notes

- `cargo test -p uclaw-provider-core` passed 9 tests.
- `cd src-tauri && cargo test providers::readiness --lib` passed 9 tests after
  linking ignored runtime resources into the worktree.
- `cd src-tauri && cargo test providers::service --lib` passed 3 tests.
- `cd src-tauri && cargo test providers::types --lib` passed 5 tests.
- `cd src-tauri && cargo test providers::store --lib` passed 3 tests.
- `cargo check -p uclaw --lib` passed with existing warnings only.
- `pyembed`, `bunembed`, and `gbrain-source` were linked from the primary
  worktree because those ignored runtime resources are not copied into
  isolated worktrees.
- Subagent review findings were addressed before commit:
  - unconfigured providers now report `NeedsConfiguration`;
  - Codex OAuth readiness does not require a base URL;
  - OAuth/API-family/latency JSON names are pinned with tests.
- `git diff --check` passed.
- GitNexus staged detect passed after indexing the PR-3 worktree.
- GitHub PR #401 merged at `9af769c1`; local `main` was synced before PR-4.

---

## PR-4 Entry Criteria

PR-4 can start because:

- PR-1, PR-2, and PR-3 are merged into `main`;
- the worktree is isolated at
  `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr4-soft-interrupts`;
- jcode soft-interrupt/browser/harness references were explored by subagents;
- uClaw runtime contracts already contain `BoundaryYield`, `Checkpoint`,
  `PermissionRequested`, and `PermissionDecided`;
- GitNexus impact for `run_agentic_loop` was run and reported HIGH risk, so
  this PR avoids loop signature or control-plane rewrites.

Recommended PR-4 first tests:

- sibling-file tests for `SoftInterruptQueue` FIFO drain, urgent counting,
  non-destructive snapshot, clear, and serde shape;
- sibling-file `RegularTask` test proving `LoopOutcome::NeedApproval` emits
  `BoundaryYield` and no terminal `TaskFinished`;
- browser rollout bridge tests proving `NeedsUserIntervention` and
  `PausedCheckpointed` are resumable yields, not completed runs;
- existing contract tests to ensure no `TaskEvent` wire-shape drift.

## PR-4 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr4-soft-interrupts-boundary-yields.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr4-soft-interrupts`
- Branch: `codex/agent-os-jcode-pr4-soft-interrupts`
- Pull request: GitHub PR #402.
- Scope: clean-room soft-interrupt queue primitives plus event adapter mapping
  for approval, browser intervention, and browser checkpoint boundaries.
- Rust hygiene: new tests live in sibling `interrupts_tests.rs`,
  `regular_task_pr4_tests.rs`, and `browser/rollout_bridge_tests.rs`; PR-4
  avoids adding new inline test bodies.
- DMZ files: none planned.
- Migration: none planned.
- Rollback: revert the new queue module, module export, boundary mapping
  changes, tests, and docs.

### PR-4 Impact Notes

- `run_agentic_loop`: HIGH impact; do not edit in PR-4.
- `ReasoningContext::is_cancelled`: HIGH impact; do not edit in PR-4.
- `LoopDelegate`: MEDIUM by GitNexus but semantically high; do not change
  signatures in PR-4.
- `outcome_to_verdict`: MEDIUM impact; keep mapping unchanged for compatibility
  and add boundary-yield behavior in `RegularTask::run`.
- `browser_run_to_events`: MEDIUM impact; direct caller is
  `emit_browser_run_into_session_dir` plus local tests.
- `TaskEvent`: serialized contract; do not add or rename variants in PR-4.

### PR-4 Verification Notes

- `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::interrupts`
  passed 4 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::regular_task`
  passed 11 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
  passed 7 tests.
- `cargo test -p uclaw-runtime-contracts` passed 20 tests plus doctests.
- `gbrain-source`, `pyembed`, and `bunembed` were linked from the primary
  worktree because ignored runtime resources are not copied into isolated
  worktrees.
- `git diff --cached --check` passed.
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr4-soft-interrupts`
  reported low risk, 0 affected processes.

---

## Handoff Template

Use this when a session stops mid-PR.

```text
Current PR:
Branch/worktree:
Files changed:
Existing user changes preserved:
GitNexus impact run:
Tests run:
Known failing tests:
Next exact command:
Decision needed from Ryan:
Rollback path:
```

---

## Closeout Template

Use this when a PR closes a milestone or major slice.

```text
Closed slice:
Merged PR:
ADR Section 18 answers changed:
Runtime behavior changed:
Harness evidence:
Performance evidence:
Regression risk:
Follow-up PR:
Status file updated:
```
