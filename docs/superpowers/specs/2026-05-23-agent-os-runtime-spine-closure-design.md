# Agent OS Runtime Spine Closure Design

Status: proposed design after Post-PR14 audit
Date: 2026-05-23
Owner: Ryan Liu
Scope: recovery design for closing the gap between the Agent OS/jcode
absorption contracts and the actual uClaw product runtime.

## 1. Purpose

The PR-1 through PR-14 chain created the foundation for the Agent OS v2 spine,
but the Post-PR14 audit found that several pieces remain contract-only,
metadata-only, or UI-invisible.

This design defines the next implementation wave. Its purpose is to make the
spine real in the product, starting with the smallest user-visible loop.

Target spine:

```text
IntentSpec -> TaskSpec -> TaskEvent -> WorldProjection -> Harness
```

The next wave must close runtime wiring before adding more conceptual layers.

## 2. Brainstorming Summary

### Approach A: Continue feature-family PRs

Continue one PR per subsystem: browser, tools, ambient, teams, harness,
evolution.

Pros:

- Easy to assign work to specialists.
- Keeps file ownership narrow.

Cons:

- Repeats the current problem: more local scaffolds without proving the
  cross-system spine.
- UI may remain fragmented for many more PRs.

### Approach B: Big bang runtime unification

Rewrite Agent, Browser, Automation, Teams, ToolContext, Harness, and Evolution
onto the canonical spine in one large branch.

Pros:

- Conceptually clean.
- Could eliminate several compatibility layers at once.

Cons:

- High merge risk.
- Touches DMZ files and high-impact symbols.
- Hard to verify and hard to roll back.

### Approach C: Spine-first recovery wave

First land the smallest visible spine loop, then migrate subsystems in order.

Pros:

- Proves the product path before broad rewrites.
- Creates a common diagnostics surface for later work.
- Makes hidden failures visible early.
- Keeps each PR reversible.

Cons:

- Requires discipline to avoid opportunistic panel rewrites.
- Leaves some subsystem-specific gaps for later PRs.

Recommendation: Approach C.

## 3. Design Decision

The next implementation wave will be spine-first:

1. Add read-only projection ingress.
2. Render a visible WorldProjection diagnostics surface.
3. Make default Agent/Chat runs emit TaskEvent observability.
4. Remove fake-success fallbacks on Agent OS paths.
5. Wire Browser Runtime Supervisor into BrowserProvider readiness.
6. Gate self-evolution promotion through harness evidence.
7. Converge teams and ambient work onto worker/projection events.

This is not a broad UI redesign. It is a runtime truth closure program.

## 4. Non-Goals

- Do not replace the existing Browser Agent v2 stack.
- Do not introduce a second scheduler.
- Do not revive `memory_graph` writes.
- Do not rewrite `run_agentic_loop` in the first PR.
- Do not move all panels to WorldProjection in one PR.
- Do not add new schema migrations unless a later plan explicitly reserves a
  migration number.
- Do not touch `tauri_commands.rs` with large logic blocks; use thin command
  wrappers over focused modules.
- Do not promote new evolution genes directly to active status.

## 5. ADR Section 18 Answers

| Question | Answer |
|---|---|
| 1. What user intent does this support? | Users need long-running agent, browser, automation, and team work to be observable, recoverable, and explainable from one runtime truth path. |
| 2. What autonomy level can it run at? | L0-L2 by default while wiring projection and diagnostics; higher autonomy remains blocked until harness promotion gates are real. |
| 3. What is the canonical truth source? | `TaskEvent` is runtime truth; projection journal payloads are derived views; `WorldProjection` is UI truth; `gbrain` remains durable knowledge truth. |
| 4. What TaskEvent entries does it emit? | Initial PRs read existing rollout records and projection journal entries. Later PRs ensure normal Agent/Chat runs emit started, model turn, tool, boundary, checkpoint, failure, cancellation, and finished events by default. |
| 5. What context does it read, and how is it cited? | Projection ingress reads rollout JSONL/projection journal artifacts for a session. The UI receives file/session ids and malformed counts so diagnostics can cite source artifacts without pretending they are durable facts. |
| 6. What capability cards does it add or consume? | Consumes existing projection journal, BrowserProvider, Browser Runtime Supervisor, tool family mesh, provider readiness, and harness campaign cards. Adds no new capability family in PR-15. |
| 7. What policy hooks can block it? | Read-only projection diagnostics should not require approval. Later write/promote paths are blocked by SafetyManager, evolution promotion policy, automation autonomy policy, browser profile/login policy, and harness gates. |
| 8. What world projection does the UI render? | PR-15 renders a diagnostics projection: session, active/completed/blocked buckets, attention reasons, journal entry counts, malformed count, source artifact ids, and last event time. |
| 9. What harness cases prove it works? | Projection hydration tests, backend projection payload command tests, malformed journal replay tests, UI bridge tests, and a model-free replay fixture that proves visible buckets match backend records. |
| 10. What is the rollback or disable path? | Disable the new projection command and diagnostics panel. Existing Agent/Chat/Browser/Automation behavior continues because PR-15 is read-only. |
| 11. What does it deliberately not own? | It does not own full panel migration, Browser Runtime Playwright provider selection, evolution promotion implementation, team worker convergence, or automation scheduler rewiring. Those are later PRs. |

## 6. Target Architecture

### 6.1 Projection Ingress

Add a backend read model that produces a frontend-friendly payload:

```ts
type RuntimeProjectionHydrationPayload = {
  sessionId: string
  stub: SessionProjectionStub | null
  journalEntries: ProjectionJournalEntry[]
  source: {
    rolloutPath?: string
    stubPath?: string
    journalPath?: string
  }
  diagnostics: {
    malformedRecords: number
    skippedEntries: number
    loadedAt: string
  }
}
```

The payload is a derived diagnostic view. It is not a new store of record.

### 6.2 Frontend Hydration

The frontend uses the PR-14 helpers:

- `hydrateProjection`
- `applyProjectionHydration`
- `buildWorldProjection`

The first visible consumer should be a small diagnostics surface, not a full
replacement for Agent, Chat, Browser, Automation, or Team panels.

### 6.3 Error Handling

Projection reads must return typed errors:

- `session_not_found`
- `rollout_not_found`
- `projection_not_available`
- `malformed_journal`
- `io_error`

Frontend bridge code must not synthesize successful sessions or silently return
empty projection state for these errors. It should show a recoverable status.

### 6.4 Browser Runtime Supervisor Integration

The restored Browser Runtime Supervisor commits are now part of `main`. The
spine closure wave treats them as the future browser runtime readiness source.

Near-term rule:

- BrowserProvider readiness may consume supervisor snapshots.
- Browser tasks should not yet switch providers or action execution lanes.
- Browser Runtime Supervisor state should become projection attention before
  Playwright or MCP sidecar work proceeds.

### 6.5 Evolution Gate Safety

Self-evolution must not directly activate newly distilled genes.

Near-term rule:

- Distilled genes enter candidate/quarantine status.
- Promotion requires harness evidence and user-visible review.
- UI verdict labels must distinguish pass, hold, and reject.

## 7. PR Sequence

### PR-15: Projection ingress and visible diagnostics

Outcome:

- Users and developers can see whether a session has runtime projection data.

Allowed scope:

- Focused backend projection read module.
- Thin Tauri command wrapper.
- Typed frontend bridge.
- Jotai/hook wrapper.
- Small diagnostics panel.
- Tests for backend payload and frontend hydration.
- Status tracker update.

Forbidden scope:

- No panel-wide migration.
- No agent-loop behavior change.
- No browser provider switch.

### PR-16: Default TaskEvent observability

Outcome:

- Normal Agent/Chat runs emit read-only TaskEvent records by default.

Allowed scope:

- Flip rollout observability default.
- Preserve legacy execution behavior.
- Make fallback visible.
- Add focused tests for event emission and fallback diagnostics.

Forbidden scope:

- No complete `run_agentic_loop` rewrite.

### PR-17: Agent OS bridge no-fake-success hardening

Outcome:

- Frontend Agent OS paths stop reporting success when backend commands fail.

Allowed scope:

- Replace synthetic session fallback.
- Add typed bridge errors.
- Render recoverable projection attention.

Forbidden scope:

- No broad UI redesign.

### PR-18: Browser Runtime Supervisor readiness bridge

Outcome:

- BrowserProvider readiness uses supervisor state and becomes visible before
  browser tasks start.

Allowed scope:

- Supervisor status command.
- BrowserProvider adapter.
- Browser readiness line in diagnostics.

Forbidden scope:

- No Playwright provider default switch.

### PR-19: Evolution gate freeze and harness runner entry

Outcome:

- Gene promotion cannot bypass harness/user review.

Allowed scope:

- Candidate gene status path.
- Harness campaign runner command.
- UI verdict fix for reject/hold/pass.

Forbidden scope:

- No new autonomous mutation loop.

### PR-20: Worker/team projection convergence

Outcome:

- Team runtime state appears as worker projection events.

Allowed scope:

- Adapter from team worker lifecycle to TaskEvent.
- Minimal UI state that can start and display a team.

Forbidden scope:

- No new team planner.

## 8. Close-Loop Tracker Rules

Every PR in this wave must update:

- `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- the PR plan under `docs/superpowers/plans/`
- audit follow-up rows when a finding is resolved
- verification notes with commands and expected output

Browser-related PRs must also update:

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`

## 9. Testing Strategy

PR-15 minimum verification:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::projection_journal
cd ui && npm test -- --run projection
git diff --check -- <changed-files>
```

PRs that touch existing symbols must run GitNexus impact before edits and
GitNexus detect-changes before commit.

Rust tests must follow the jcode-style sibling test file convention:

```rust
#[cfg(test)]
#[path = "module_tests.rs"]
mod tests;
```

## 10. Rollback

Each PR must be independently reversible:

- PR-15 rollback removes diagnostics ingress only.
- PR-16 rollback restores env-gated rollout emission.
- PR-17 rollback restores bridge fallback behavior only if a user-visible
  warning remains.
- PR-18 rollback disables supervisor-backed readiness and leaves current
  Browser Agent v2 execution untouched.
- PR-19 rollback pauses promotion rather than re-enabling direct activation.
- PR-20 rollback removes team projection adapter while keeping legacy teams.

## 11. Self-Review

Placeholder scan:

- No placeholder markers remain.

Consistency check:

- The design keeps PR-15 read-only and reserves behavior changes for later
  PRs.
- The Browser Runtime Supervisor restoration is treated as a dependency, not
  as an immediate provider switch.

Scope check:

- The implementation wave is split into six PRs so no single PR owns the full
  runtime rewrite.

Ambiguity check:

- "Projection ingress" means a read-only diagnostics path, not a new canonical
  store.
