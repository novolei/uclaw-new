# 阶段 3 Tier 1+2+3 Batch · Sequencing Plan

> **Scope**: Close Tier 1 + Tier 2 + Tier 3 from `docs/superpowers/specs/2026-05-29-stage3-closeout-gap-reaudit.md`. Per-PR task detail goes inline in implementer dispatches (audit IS the spec). Tier 3.10 (importance_decay) deferred to 阶段 4 per user decision.

## PR sequence (7 PRs off main)

| # | PR | Tier | Scope (one-liner) | Est. LoC | Key files |
|---|---|---|---|---:|---|
| 1 | cancel-install | 1.1 | Install `CancellationToken` on chat + agent entry paths; expose `cancel_conversation` Tauri cmd | ~150 | `app.rs`, `tauri_commands.rs:1961+11334`, new `agent/cancellation_registry.rs` |
| 2 | compact-resume-persist | 1.2 | Persist + restore `CompactionState.previous_fold` across session reload | ~200 | `agent/compaction.rs`, `agent/session.rs`, `db/migrations.rs` (new V), `tauri_commands.rs:1962` |
| 3 | snapshot-model-thread | 1.3 | Thread `snapshot: &TurnSnapshot` into cost/observability; kill `self.model` reads inside per-turn flow | ~150 | `dispatcher/observability.rs`, `dispatcher/turn_runner.rs`, possibly trait updates |
| 4 | system-prompt-full-seam | 2.4 | Move 4 post-appends (gene/plan-hint/rules/ladder-pad) INTO `SystemPromptContext`; extend snapshot tests | ~250 | `dispatcher/content_assembler.rs`, `dispatcher/turn_runner.rs:486-569` |
| 5 | compose-cleanup + first-act-reset | 2.5 + 2.7 | Delete 3 dead `compose_system_prompt` variants; reset `is_first_act_turn` on Plan→Auto toggle | ~80 | `agent/mode_prompts.rs`, `dispatcher/mod.rs:198-202` |
| 6 | context-pressure-wire | 2.6 | Wire `estimate_context_pressure_ratio` to live `TokenBudgetSnapshot` | ~60 | `dispatcher/content_assembler.rs:357`, telemetry |
| 7 | memory-skeleton-cleanup | 3.8 + 3.9 | Drop `memory_contract/` + `MemoryPolicyExecutor` impl + tests (KEEP `memory_policy::types` + `receipts`) | ~-300 (net deletion) | `src/memory_contract/`, `src/memory_policy/executor.rs`, `src/memory_policy/tests.rs`, `src/lib.rs` |

**Dependencies**: PR3 and PR4 both touch `dispatcher/turn_runner.rs`; do PR3 first, PR4 rebases on it via main merge. Otherwise PRs are independent.

**Workflow per PR**: off-main worktree → 1-3 implementer dispatches → cumulative review → push → open PR. Each PR commits bisectable + tests pass + ≤49 warnings.

**Baselines after each PR** (must hold):
- `cargo build`: 0 errors, ≤49 warnings (or net delta documented).
- `cargo test --lib agent::dispatcher`: ≥55 passed / 0 failed.
- `cargo test --lib agent::`: ≥801 passed / 2 pre-existing failed.

## Out of scope (for this batch)

- Tier 3.10 importance_decay regression — deferred to 阶段 4.
- Tier 4/5/6 (memory adapter, AgentApi runtime callers, coding reliability, browser nested loop) — separate stages.

## Execution log

Filled in as PRs ship.
