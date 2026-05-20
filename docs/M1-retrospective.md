# M1 — Runtime Contracts: Retrospective

**Status**: ✅ Complete — 2026-05-20
**Duration**: Phase 0.5 + M1, single conversation in Cowork mode
**Total PRs**: 24 (Phase 0.5: 8, M1: 16 including this one)
**Net code**: ~5,400 LOC added · 200+ unit tests · 0 regressions

This document records how the M1 milestone (`Runtime Contracts`) actually
played out vs. the original plan, what we learned, and what's deferred to
follow-ups.

---

## 1. ADR exit criterion

Per `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md §"Cross-domain rollout"`:

> *M1 is complete when one chat task + one browser task + one automation
> run produce comparable traces in the same rollout JSONL.*

**Status: closed.** Trace emission wired in all three domains, gated by
`UCLAW_ROLLOUT_ENABLED`, writing to `~/.uclaw/sessions/rollout-*.jsonl` +
the `task_events_rollout` SQLite mirror (V48).

| Domain | Conversion layer | Wire-up | PR |
|---|---|---|---|
| Chat | implicit (RegularTask + outcome_to_verdict) | `run_with_rollout` | M1-T4b (#311) |
| Browser | `browser_run_to_events` | `emit_browser_run_into_session_dir` | M1-T4c (#316) + M1-T4d (#317) |
| Automation | `activity_to_events` | `emit_activity_into_session_dir` | M1-T4e (#319) + M1-T4f (#320) |

---

## 2. PR ledger

### Phase 0.5 — Foundation

| PR | Task | Summary |
|---|---|---|
| #297 | T2 | Cargo workspace conversion |
| #298 | T7 | `memory_graph` runtime freeze guard |
| #299 | T3 | First 3 utility crates (async-utils, terminal-detection, file-watcher) |
| #300 | T4 | Second 9 utility crates from `codex-rs/utils/` |
| #301 | T5a | path-utils + image + uclaw home (modified) |
| #302 | T6 | `uclaw_utils_home` full-repo sweep |

### M1 — Runtime Contracts

| PR | Task | Summary |
|---|---|---|
| #304 | T1 | `IntentSpec` / `TaskSpec` / `TaskEvent` (13 variants) + `AutonomyLevel` (L0–L6) + `RiskClass` |
| #305 | T2a | `SessionTask` trait + `TaskScheduler` (100ms graceful timeout) |
| #306 | T2b | `agentic_loop` state audit spec (`docs/superpowers/specs/`) |
| #307 | T2c | `RegularTask : SessionTask` + **R-1 HIGH fix** (`force_text` reset) |
| #308 | T3 | `HarnessSubject ↔ TaskEventSource` 1:1 bridge |
| #309 | T4a | `RegularTask` emits `ModelTurn` + `Warning` events |
| #310 | T5 | Rollout JSONL writer + `task_events_rollout` (V48) |
| #311 | T4b | Chat dispatcher rollout integration |
| #313 | T6 | `TokenUsage` 6-D + V49 `cost_records` columns |
| #314 | T2d | Inter-stage `CancellationToken` + **R-6 HIGH fix** |
| #315 | T7 | LLM HTTP/2 prewarm at Stage 3 |
| #316 | T4c | `BrowserTaskRun → TaskEvent` conversion |
| #317 | T4d | Browser rollout wire-up |
| #319 | T4e | `AutomationActivity → TaskEvent` conversion |
| #320 | T4f | Automation rollout wire-up — **closes M1** |

### Skipped / deferred

- **M1-T2e** (LoopDelegate trait gets `&CancellationToken` for mid-LLM-stream cancel) — deferred because audit spec's HIGH gaps (R-1, R-6) are both closed by M1-T2c/d; mid-stream cancel is nice-to-have, the trait signature change touches `ChatDelegate` + `HeadlessDelegate` + multiple mocks. Risk/benefit ratio favors deferral.

---

## 3. Two HIGH risks closed

Per `docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md`:

### R-1 HIGH — `reason_ctx.force_text` sticky flag (fixed by #307)

**Symptom**: once truncation triggered `force_text = true` in any turn, every subsequent turn in the same session inherited the constraint forever (no reset path existed).

**Fix**: `RegularTask::run` resets `ctx.force_text = false` before delegating to `run_agentic_loop`. Reset-at-start (the audit spec's design Q3 choice) covers 100% of the bug at the cost of one extra LLM turn that might have benefited from the constraint.

### R-6 HIGH — Cancellation observation gap (fixed by #314)

**Symptom**: cancellation could only be observed at the iteration-top `check_signals` poll. A `LoopSignal::Cancel` arriving mid-LLM-call had to wait for the response to fully return AND then for the next iteration to begin before being acted on (could be 30+ seconds for streaming responses).

**Fix**: Added two checkpoints inside `run_agentic_loop`:
1. Between `check_signals` poll and stage-2 (context compression)
2. Immediately after `call_llm().await` returns

`ReasoningContext` gained an `Option<CancellationToken>` field that `RegularTask::run` populates. The agent loop checks `ctx.is_cancelled()` at the two checkpoints and exits with `LoopOutcome::Cancelled { partial_code }` if fired.

Mid-LLM-stream cancellation (interrupting in the middle of a 30s response) still requires extending `LoopDelegate` trait signatures with `&CancellationToken` — deferred to M1-T2e.

---

## 4. What worked

### Cowork ↔ user-IDE isolation

Throughout this milestone, the user's primary `~/Documents/uclaw` worktree continued landing PRs (#312 halo-compatible digital humans, gitnexus docs, automation stop fixes) **completely in parallel** with Cowork's M1 work. Zero merge conflicts — git worktree isolation worked exactly as designed (`BEHAVIOR.md §5`).

### Stacked PR pattern

Phase 0.5 used a 6-deep stack (#290 → #295). After observing the GitHub auto-retarget bug (`--delete-branch` orphaned downstream PRs), M1 switched to **direct-to-main** PRs with manual stacking only when truly dependent. Result: zero orphaned PRs, smoother merges.

### One audit before the DMZ edit

`agentic_loop.rs` (882 lines, DMZ) got edited twice: M1-T2c (R-1 fix) and M1-T2d (R-6 fix). Both PRs were preceded by the audit spec (M1-T2b, #306). The Writer/Reviewer pattern (`BEHAVIOR.md §8`) reduced reviewer cognitive load — they could read 178 lines of spec before looking at 9 lines of code change.

### Conversion-then-wire-up split

For browser + automation, the per-domain rollout integration was split into 2 PRs each:
- M1-T4c / M1-T4e: conversion (`*Run → Vec<TaskEvent>`)
- M1-T4d / M1-T4f: wire-up (call the conversion from production callsites)

This let reviewers focus on **mapping correctness** in PR 1, then **callsite placement** in PR 2. Both reviews are cheap; together they're harder.

### Fire-and-forget pattern

`tokio::spawn(async move { emit_*_into_session_dir(...).await })` shows up in 3 production callsites (chat, browser, automation) — kept the hot paths zero-blocking while the rollout I/O happens in the background.

---

## 5. What was harder than expected

### V-number coordination

The plan referenced V41/V44/V55 as M1 migration numbers, but V44–V47 were already claimed by Memory OS L3 PRs running in parallel. We landed on **V48** (task_events_rollout) and **V49** (cost_records 6-D). The `CONTEXT.md` migration registry is the source of truth here; the plan's V-numbers were aspirational.

### Trait signature changes

M1-T2e (LoopDelegate gains `&CancellationToken`) would have touched the trait, ChatDelegate impl (1196 lines in dispatcher.rs), HeadlessDelegate impl, and multiple test mocks. **Single-PR cost projection**: ~600 LOC across ~6 files. Deferred until the inter-stage fix (M1-T2d) ships first to validate the cancellation model.

### `Option<&CancellationToken>` ergonomics

The `run_with_rollout` helper accepts `Option<&RolloutHandle>` so callers can opt out cheaply. `RegularTask` takes `CancellationToken` directly via `SessionTask::run` (M1-T2a). Two slightly different patterns — should they unify? **Decision: not yet.** Forcing unification before we have ≥3 caller patterns risks premature abstraction.

---

## 6. Test coverage delta

| Module | Tests added in M1 | Status |
|---|---|---|
| `runtime::contracts` | 11 | all pass |
| `runtime::task` | 6 | all pass |
| `runtime::rollout` | 3 | all pass |
| `agent::regular_task` | 10 | all pass |
| `agent::rollout_integration` | 3 | all pass |
| `agent::types` (TokenUsage 6-D fixtures) | — (extended existing) | unchanged |
| `harness::case` (M1-T3 bridge) | 2 | all pass |
| `llm::prewarm` | 6 | all pass |
| `browser::rollout_bridge` | 6 | all pass |
| `automation::rollout_bridge` | 9 | all pass |
| `db::migrations` (V49) | 2 | all pass |
| **Total new in M1** | **58** | **58 pass, 0 fail** |

Plus ~170 tests from Phase 0.5 utility-crate ports.

---

## 7. Remaining backlog

| Priority | Item | Notes |
|---|---|---|
| Medium | M1-T2e — `LoopDelegate &CancellationToken` | Mid-LLM-stream cancellation; large diff, deferred |
| Low | Failed/Cancelled automation paths | M1-T4f only wires Completed; other terminal statuses need their UPDATE callsites identified + wired |
| Low | SQLite mirror for chat callsite | M1-T4b passes `None` for db_path; AppState threading needed |
| Low | Real provider+model in `ModelTurn` | "agent_loop"/"aggregated" placeholders today; requires `LoopOutcome::Response` extension (~10 callsites) |
| Low | `cost_records.cost_usd` recompute with new 6-D pricing | M1-T6 added columns but didn't backfill |

None of these block M2 entry.

---

## 8. M2 readiness

Per the ADR, M2 introduces:
- **Context Fabric** — typed context container that survives task boundaries
- **Hook Bus** — 13-event policy hook system
- **StructuredFold** (M2-G) — context compression evolution
- **Token budget surfacing** (M2-J) — UI dashboard

M1's `TaskEvent` schema is the input for M2's Hook Bus. M1's `IntentSpec` / `TaskSpec` are the units M2's Context Fabric attaches to. **M2 can start whenever.**

---

## Sign-off

This retrospective is referenced by the M2 entry PR. Per `BEHAVIOR.md §10`, the DRI (Ryan Liu, [@novolei](https://github.com/novolei)) approves M1 closure.

Generated by Cowork on 2026-05-20 during the M1-closing pack.
