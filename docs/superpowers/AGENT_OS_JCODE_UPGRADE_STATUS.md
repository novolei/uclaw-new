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
> Current phase: PR-0 design baseline
> Current source package: `docs/jcode_comparison/` +
> `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`

---

## Quick View

| PR | Theme | Status | Owner Session | Next Action |
|---|---|---|---|---|
| PR-0 | Design baseline and close-loop governance | Committed | Codex | Baseline commit `c44a3267`; PR-1 numbering correction is tracked in this worktree. |
| PR-1 | Pure type crates for messages/tools/protocol/runtime contracts | Plan ready | Codex | Review and execute `docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md` in the isolated worktree. |
| PR-2 | ToolContext adapter | Not started | Unassigned | Wait for PR-1 pure type crates. |
| PR-3 | Provider readiness core | Not started | Unassigned | Wait for PR-1 and current provider impact analysis. |
| PR-4 | Soft interrupts and boundary yields | Not started | Unassigned | Wait for PR-1 contracts and policy review. |
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

---

## Current Branch Hygiene

This section is intentionally manual. Update it before opening or reviewing a
PR.

| Check | Current Value |
|---|---|
| Primary worktree | `/Users/ryanliu/Documents/uclaw` |
| Known pre-existing tracked changes | `AGENTS.md`, `CLAUDE.md` |
| Current jcode comparison docs | `docs/jcode_comparison/` is untracked at the time this status file was created. |
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

- serde wire-shape test for `ChatMessage`;
- serde wire-shape test for `ToolCall` and `ToolDefinition`;
- serde round-trip tests for `IntentSpec`, `TaskSpec`, and `TaskEvent`;
- compile regression tests for existing provider, rollout, browser, automation, and harness modules.

## PR-1 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr1-plan`
- Branch: `codex/agent-os-jcode-pr1-plan`
- Scope: extract `uclaw-message-types`, `uclaw-tool-types`, `uclaw-runtime-contracts`, and `uclaw-protocol-types`.
- DMZ files: root `Cargo.toml` touched; writer/reviewer required before merge.
- Migration: none planned.
- Rollback: revert crate additions, dependency additions, and compatibility re-export facades.

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
