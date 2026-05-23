# PR-13 World Projection Surface Selectors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a pure frontend WorldProjection selector layer so Agent, Chat, Browser, Automation, Symphony, and Team surfaces can share PR-12 runtime task truth.

**Architecture:** PR-13 does not rewrite visual components yet. It adds a small TypeScript module that maps `RuntimeProjection` into per-surface buckets and stable `WorldTaskProjection` records, creating the contract future UI wiring will consume.

**Tech Stack:** TypeScript, Vitest, existing PR-12 `ui/src/lib/agent-os/projection-reducer.ts` DTOs.

---

## Scope Boundaries

- Owns: `WorldProjection` frontend DTO, surface mapping helpers, deterministic selector tests, status ledger handoff.
- Does not own: Tauri commands, Jotai atoms, React components, browser runtime behavior, automation runtime behavior, Symphony execution, team worker runtime.
- Evidence: PR-12 reducer is now on `main`; GitNexus impact for `buildRuntimeProjection` is LOW with only reducer tests upstream.

## ADR 18 Answers

| Question | PR-13 Answer |
|---|---|
| 1. What user intent does this support? | Let users see one consistent task truth across Agent, Chat, Browser, Automation, Symphony, and Team views. |
| 2. What autonomy level can it run at? | Passive read-only frontend projection; it performs no autonomous action. |
| 3. What is the canonical truth source? | PR-12 `RuntimeProjection`, derived from PR-5 `TaskEvent` projection records. |
| 4. What TaskEvent entries does it emit? | None; it maps existing projected TaskEvent summaries. |
| 5. What context does it read, and how is it cited? | Reads task ids, source, status, sequence, timestamps, checkpoint refs, boundary reasons, and rollout file handles already carried by projection DTOs. |
| 6. What capability cards does it add or consume? | Adds no capability cards; later surface wiring consumes this view behind existing UI/runtime policy. |
| 7. What policy hooks can block it? | None in this pure selector; future data loading and actions remain policy-gated elsewhere. |
| 8. What world projection does the UI render? | A task map plus per-surface active, waiting, terminal, and attention task buckets. |
| 9. What harness cases prove it works? | Vitest fixtures for surface grouping, status bucket derivation, attention reasons, and explicit surface overrides. |
| 10. What is the rollback or disable path? | Remove `world-projection*` files and status/plan doc updates; no runtime wiring to disable. |
| 11. What does it deliberately not own? | UI component migration, event listeners, backend DTO changes, DB schema, and runtime execution semantics. |

## Files

- Create: `ui/src/lib/agent-os/world-projection.ts`
- Create: `ui/src/lib/agent-os/world-projection.test.ts`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Create: `docs/superpowers/plans/2026-05-23-pr13-world-projection-surface-selectors.md`

## Task 1: World Projection Tests

- [x] **Step 1: Write fixture tests**

Create tests that build a `RuntimeProjection` fixture and assert:

```ts
const world = buildWorldProjection(runtime)
expect(world.surfaces.agent.taskIds).toEqual(['agent-task', 'browser-task', 'automation-task', 'team-task'])
expect(world.surfaces.browser.taskIds).toEqual(['browser-task'])
expect(world.surfaces.automation.taskIds).toEqual(['automation-task'])
expect(world.surfaces.team.taskIds).toEqual(['team-task'])
expect(world.surfaces.chat.taskIds).toEqual(['agent-task'])
```

- [x] **Step 2: Run red test**

Run: `cd ui && npm test -- --run src/lib/agent-os/world-projection.test.ts`

Expected: initially fails because `world-projection.ts` does not exist.

## Task 2: Pure WorldProjection Selectors

- [x] **Step 1: Implement the selector module**

Create `world-projection.ts` with:

- `WorldSurface = 'agent' | 'chat' | 'browser' | 'automation' | 'symphony' | 'team'`
- `WorldTaskRole = 'primary' | 'tool' | 'browser' | 'automation' | 'memory' | 'coordination' | 'worker'`
- `WorldTaskAttention = 'none' | 'waiting' | 'warning' | 'failed' | 'budget_exhausted'`
- `buildWorldProjection(runtime, options?)`
- deterministic default mapping:
  - agent sees every task;
  - chat sees agent loop, prompts, tools, permissions, skills, plugins, hooks, memory, and gbrain tasks;
  - browser sees browser tasks;
  - automation sees automation tasks;
  - team sees tasks and coordinator tasks;
  - symphony sees automation, tasks, and coordinator tasks.

- [x] **Step 2: Run focused tests**

Run: `cd ui && npm test -- --run src/lib/agent-os/world-projection.test.ts`

Expected: all world projection selector tests pass.

## Task 3: Docs, Verification, and PR

- [x] **Step 1: Update status ledger**

Mark PR-12 merged and PR-13 in progress in `AGENT_OS_JCODE_UPGRADE_STATUS.md`; after PR creation mark PR-13 open.

- [x] **Step 2: Final verification**

Run:

```bash
cd ui && npm test -- --run src/lib/agent-os/world-projection.test.ts
git diff --cached --check -- docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-pr13-world-projection-surface-selectors.md ui/src/lib/agent-os/world-projection.ts ui/src/lib/agent-os/world-projection.test.ts
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr13-surface-convergence
```

Expected: Vitest passes, diff check passes, GitNexus reports low risk.

Execution note: PR13 worktree indexing succeeded, but the local GitNexus
registry did not expose the PR13 path to `detect-changes`; the attempted path
run failed with repository-not-found. The final gate therefore uses focused
Vitest, a strict two-file TypeScript check, and staged diff whitespace check,
with the GitNexus registry limitation recorded in the commit/PR notes.

- [ ] **Step 3: Commit and open PR**

Commit:

```bash
git commit -m "feat(ui): add Agent OS world projection selectors"
```

## Self-Review

- Spec coverage: maps PR-12 runtime projection into PR-13 surface convergence contract without premature component rewrites.
- Placeholder scan: no TODO/TBD placeholders.
- Type consistency: imports PR-12 frontend projection DTOs and does not duplicate backend-derived task status/source types.
