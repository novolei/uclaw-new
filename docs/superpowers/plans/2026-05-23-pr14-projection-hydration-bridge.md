# PR-14 Projection Hydration Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a pure frontend hydration bridge that turns backend projection DTO payloads into the PR-12 `RuntimeProjection` and PR-13 `WorldProjection` outputs future UI surfaces can consume.

**Architecture:** PR-14 stays on the frontend contract layer and avoids Tauri command, Jotai atom, event listener, and React component wiring. It defines the loader result shape and deterministic hydrate/apply helpers so PR-15 can wire a read-only backend command without changing reducer semantics.

**Tech Stack:** TypeScript, Vitest, existing `projection-reducer.ts`, existing `world-projection.ts`.

---

## Scope Boundaries

- Owns: pure hydration helpers, loader result type, focused tests, status ledger update.
- Does not own: backend Tauri commands, `tauri_commands.rs`, `main.rs invoke_handler!`, Jotai atoms, React component wiring, runtime projection journal changes, SQLite migrations.
- GitNexus impact before editing: `buildRuntimeProjection` LOW, `buildWorldProjection` LOW; each currently has only its own test as an upstream caller.

## ADR 18 Answers

| Question | PR-14 Answer |
|---|---|
| 1. What user intent does this support? | Let future UI surfaces hydrate one consistent Agent OS projection from backend projection payloads. |
| 2. What autonomy level can it run at? | Passive read-only frontend mapping; no autonomous execution. |
| 3. What is the canonical truth source? | Backend `SessionProjectionStub` and `ProjectionJournalEntry` DTOs derived from `TaskEvent` rollout JSONL. |
| 4. What TaskEvent entries does it emit? | None; it consumes projection DTOs only. |
| 5. What context does it read, and how is it cited? | Reads schema version, rollout file, task ids, sequences, timestamps, checkpoint refs, boundary reasons, malformed counts, and journal entries already carrying source handles. |
| 6. What capability cards does it add or consume? | Adds none; future command wiring must remain capability/policy-aware. |
| 7. What policy hooks can block it? | None in this pure bridge; backend command and action wiring remain policy-gated later. |
| 8. What world projection does the UI render? | Hydrated `RuntimeProjection` plus derived `WorldProjection` from the same payload. |
| 9. What harness cases prove it works? | Vitest fixtures for stub-only hydration, journal replay, empty fallback, and world surface derivation. |
| 10. What is the rollback or disable path? | Remove `projection-hydration*` files plus status/plan updates; no runtime wiring to disable. |
| 11. What does it deliberately not own? | Backend projection IO, Tauri commands, component rendering, atoms, runtime execution, and database schema. |

## Files

- Create: `ui/src/lib/agent-os/projection-hydration.ts`
- Create: `ui/src/lib/agent-os/projection-hydration.test.ts`
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Create: `docs/superpowers/plans/2026-05-23-pr14-projection-hydration-bridge.md`

## Task 1: Hydration Tests

- [x] **Step 1: Write fixture tests**

Create `projection-hydration.test.ts` with fixtures that assert:

- `hydrateProjection(payload)` returns both `runtime` and `world`;
- journal entries are replayed after the stub and can add newer task state;
- an empty or missing stub creates an empty runtime projection for a session id;
- the returned `world` surface buckets match the replayed runtime state.

- [x] **Step 2: Run red test**

Run: `cd ui && npm test -- --run src/lib/agent-os/projection-hydration.test.ts`

Expected: initially fails because `projection-hydration.ts` does not exist.

## Task 2: Pure Hydration Bridge

- [x] **Step 1: Implement `projection-hydration.ts`**

Create exports:

- `ProjectionHydrationPayload`
- `ProjectionHydrationResult`
- `hydrateProjection(payload, options?)`
- `applyProjectionHydration(result, payload, options?)`

Semantics:

- `hydrateProjection` starts from a provided `SessionProjectionStub` when present, otherwise `emptyRuntimeProjection(sessionId)`.
- It applies `journalEntries` after stub hydration.
- It derives `world` from the final runtime projection.
- It preserves the input payload by copying through existing reducers/selectors.
- `applyProjectionHydration` accepts a prior result and replays only the incoming journal entries through `applyProjectionJournalEntries`, then rebuilds `world`.

- [x] **Step 2: Run focused tests**

Run: `cd ui && npm test -- --run src/lib/agent-os/projection-hydration.test.ts`

Expected: all projection hydration tests pass.

## Task 3: Verification and PR

- [x] **Step 1: Final verification**

Run:

```bash
cd ui && npm test -- --run src/lib/agent-os/projection-hydration.test.ts src/lib/agent-os/projection-reducer.test.ts src/lib/agent-os/world-projection.test.ts
cd ui && npx tsc --noEmit --strict --target ES2021 --lib ES2023,DOM,DOM.Iterable --module ESNext --moduleResolution bundler --skipLibCheck --jsx react-jsx --baseUrl . src/lib/agent-os/projection-hydration.ts src/lib/agent-os/projection-hydration.test.ts src/lib/agent-os/projection-reducer.ts src/lib/agent-os/world-projection.ts
git diff --cached --check -- docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-23-pr14-projection-hydration-bridge.md ui/src/lib/agent-os/projection-hydration.ts ui/src/lib/agent-os/projection-hydration.test.ts
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr14-projection-hydration-bridge
```

Expected: focused tests pass, strict file-level TypeScript check passes, staged diff check passes, GitNexus reports low risk or records any local registry limitation.

- [x] **Step 2: Commit and open PR**

Commit:

```bash
git commit -m "feat(ui): add Agent OS projection hydration bridge"
```

## Self-Review

- Spec coverage: bridges PR5/PR12/PR13 at the frontend contract layer without premature backend command or UI wiring.
- Placeholder scan: no TODO/TBD placeholders.
- Type consistency: reuses existing projection DTOs and does not duplicate task status/source definitions.
