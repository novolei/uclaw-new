# Bundle 17-B/C Wire-up Design (C1.1)

> **Context**: First sub-task of C1 (M2 closeout) per
> [`docs/superpowers/plans/2026-05-22-pr-integration-strategy.md`](../plans/2026-05-22-pr-integration-strategy.md) §7.
> Unblocks long-pending task #146.

## 1. Background

Bundle 17-A (PR #385, commit `693f956e`) shipped `agent/compact/fold_diff.rs`
— 640 lines of typed cross-fold delta machinery:

- `AxisItem` trait: `stable_key()` + `content_hash()` per axis
- `AxisDelta<T>` carrying added / removed / changed / unchanged_count
- `diff_axis<T>` function: O(n+m) HashMap-indexed diff
- `FoldDelta` struct: 8-axis combined delta
- `AxisItem` impls for all 8 fold component types

The module comment explicitly says:

> The dispatcher (Bundle 17-B) reads the delta and either:
> - applies it locally (`apply_to`) to produce the next fold without an LLM call, or
> - emits a `<context_changes_since_last_fold>` block on top of the stable
>   prior fold so the system-prompt cache breakpoint keeps hitting.

**Current state**: `fold_diff` is dead code — nothing imports or consumes it
in production code paths. The dispatcher still re-runs the full LLM fold on
every `/compact` trigger.

**Why it stalled** (per task #146 "pending telemetry"): without
`TokenBudgetSnapshot` (M2-J pilot #342) being consumed by SettingsTab, we
can't observe whether delta-fold saves tokens. The fix shipped without
metrics would be invisible — would land in M2 closeout retro as "no idea
if it worked."

## 2. Scope

Two PRs, in order:

### 2.1 Bundle 17-B: dispatcher reads the delta

**Files touched**:

- `src-tauri/src/agent/compact/mod.rs` — re-export `apply_to` if needed,
  document the wire-up
- `src-tauri/src/agent/dispatcher.rs` — at the `/compact` trigger site,
  before calling LLM fold:
  1. Read prior `StructuredFold` from session state (already cached after
     last /compact)
  2. Call new producer `summarize_to_fold_delta(prior, new_corpus)` —
     this is the cheap path
  3. If `FoldDelta` is "small" (per threshold: total changed_count +
     added_count < 5 across all 8 axes) → emit
     `<context_changes_since_last_fold>` block + reuse prior fold (no LLM)
  4. Otherwise → fall back to full re-fold (current behavior, LLM call)

**Threshold**: 5 across-axis-cumulative delta count. Tunable via
`MemubotConfig::context.fold_delta_threshold` (default 5). Below threshold
= local apply; above = full LLM re-fold.

**Tests**:

- Unit test in dispatcher: given a session with prior fold + small delta,
  call should NOT spawn LLM
- Unit test: given a session with prior fold + large delta, call SHOULD
  spawn LLM (current path)
- Integration smoke: 50-turn fixture session — assert N_LLM_folds <
  N_compact_triggers (delta path triggers at least once)

### 2.2 Bundle 17-C: telemetry piping to M2-J snapshot

**Files touched**:

- `src-tauri/src/agent/token_budget/snapshot.rs` — extend
  `TokenBudgetSnapshot` with `fold_delta_stats: FoldDeltaStats`:
  ```rust
  pub struct FoldDeltaStats {
      pub total_compactions: u32,
      pub delta_applied: u32,        // skipped LLM via delta
      pub full_refold: u32,          // LLM re-fold path
      pub tokens_saved_estimate: u64,
  }
  ```
- `src-tauri/src/agent/dispatcher.rs` — emit `FoldDeltaStats` increments
  on each /compact trigger via the existing infra service event bus
- `src-tauri/src/tauri_commands.rs` — `get_token_budget_snapshot` already
  exists from #342; reads new field automatically via serde

**Tests**:

- Round-trip serde on new `FoldDeltaStats`
- After a session with 10 small compactions + 2 big ones, snapshot reports
  `delta_applied=10, full_refold=2`

## 3. Out of scope (deferred to next milestone slice)

- **Frontend UI for FoldDeltaStats** — that's M2-J wire-up proper (C1.2),
  separate PR
- **Remote compaction** (the `remote.rs` half of plan M2-H L7) — not
  blocking 17-B/C
- **CompactionAnalytics 5-dim** (plan M2-H L7 row "Analytics") — separate
  follow-up

## 4. Concrete commit plan

### PR-1: Bundle 17-B (dispatcher wire-up)

```
Commit 1: chore(agent/compact): expose summarize_to_fold_delta producer
Commit 2: feat(agent/dispatcher): apply FoldDelta in /compact path when below threshold
Commit 3: test(agent/dispatcher): cover delta-applied vs full-refold paths
```

### PR-2: Bundle 17-C (telemetry)

```
Commit 1: feat(token_budget): FoldDeltaStats on TokenBudgetSnapshot
Commit 2: feat(dispatcher): emit FoldDeltaStats deltas on /compact trigger
Commit 3: test(token_budget): round-trip + integration stats accumulation
```

Both PRs ship via `prep/bundle-17b-dispatcher-wireup` and
`prep/bundle-17c-telemetry` branches, merged sequentially.

## 5. Verification

After both PRs land:

1. **Unit + integration tests green** (`cargo test --lib agent::compact agent::dispatcher`)
2. **Bench harness**: 50-turn fixture session, compare token cost
   before/after — expected savings: ~30-50% on /compact path
3. **Drift script update**: classify both PRs as `M-Wireup` not Tactical;
   confirm tactical % stays low

## 6. Open questions for review

- Threshold of 5 is a wild guess — pick after instrumenting one session?
  Or pick 5 now, tune via setting later?
- `<context_changes_since_last_fold>` block format — borrow from
  Bundle 16's `<memory_context_changes>` shape? (cross-turn delta pattern)
- Should delta-applied path also bump prompt-cache breakpoint count
  (M2-I)? Probably yes — but separate commit if needed.

## 7. Estimated effort

- Bundle 17-B: 0.5-1 day
- Bundle 17-C: 0.5 day
- Total: ~1.5 days

## 8. Closes / unblocks

- task #146: pending → completed
- Drives M2 progress from ~55% → ~60% (out of ~6 sub-tasks left in M2)
