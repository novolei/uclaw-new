# M2-D Phase 2 — Real Diff-Based Context Re-Injection

**Status:** Spec draft.
**Date:** 2026-05-21
**Owner:** Bundle 16 (Track A), Bundle 17 (Track B).
**Builds on:**
- M2-D pilot — `agent/context_diff/{mod,diff}.rs` (`FragmentSnapshot`, `ContextDiff`, `diff_snapshots`)
- Slice 3-B / Bundle 10 — observability wire-up in `agent/dispatcher.rs::build_dynamic_context()`
- M2-G — `agent/compact/StructuredFold` (the 8-axis fold produced by `/compact`)
- M2-I — Anthropic 4-breakpoint cache placement (Bundle 11)
- Slice 3 verification doc (`2026-05-21-slice-3-verification.md`)

---

## Goal

Move M2-D from "log when memory_context drifts" to "inject only the
delta when it drifts". Two tracks, smallest viable wins first:

| Track | Wins | Cost | Bundle |
| --- | --- | --- | --- |
| **A. memory_context cross-turn delta** | -200 to -800 tokens/turn when recall is stable; cache_control hit-rate up | 1 PR; needs line-level snapshot upgrade | Bundle 16 |
| **B. StructuredFold cross-fold delta** | -30-60% on post-/compact follow-ups; -1 fold-prompt LLM call per re-fold | 2-3 PRs; needs `StructuredFold::diff()` + dispatcher anchor state | Bundle 17 |

Anti-goals for Phase 2:
- **Not** doing per-iteration delta within the same turn. Iter 1 →
  iter 2 of the same turn already hits Anthropic cache reliably
  thanks to M2-I; the un-cached providers (DeepSeek / Kimi) don't have
  prompt-cache to differentiate "saw it last iter" from "seeing it
  fresh", so per-iter delta has no benefit and risks corruption.
- **Not** redesigning M2-G fold shape. We add a `diff` method on the
  existing 8-axis fold; we do not introduce a new compact format.

---

## Why now

Slice 3 closed the foundational pieces (StructuredFold, cache
breakpoints, observability). The follow-up token wins are now
unlocked but un-realized — pre-existing prep work means each track
is 1-2 commits of business logic rather than a multi-week pilot.

Production hint: the per-iter `[M2-D] memory_context unchanged`
debug line already shows that the **across-iter** case is well
covered by Anthropic cache. The visible drift in `/tmp/uclaw-dev.log`
happens **between turns** when recall re-runs against a slightly
different query — exactly what Track A targets.

---

## Track A — memory_context cross-turn delta

### Problem

`recall.retrieve_with_context(query)` runs once per user turn. Turn N
might produce a memory_context like:

```
<memory_context>
- 用户偏好: Chinese conversation
- 当前项目: uClaw refactor (M2-D phase 2)
- 最近 5 个技能: search_files (12 uses), edit_block (9 uses), ...
</memory_context>
```

Turn N+1 with a slightly different query might surface 4 of the
same 5 facts plus 1 new one. Today the dispatcher re-sends the whole
block, busting the Anthropic system-prompt cache breakpoint.

### Track A spec

#### A1. Line-level FragmentSnapshot for memory_context

Bundle 10's snapshot is a single djb2 of the whole block — any
1-byte change marks the whole thing as "changed". For Track A we
need line-level granularity so the diff can name which lines added /
removed / changed.

**New helper:** `agent/context_diff/line_snapshot.rs`

```rust
/// Per-line snapshot inside a single named fragment. Lets a
/// fragment-level diff downgrade "whole fragment changed" into
/// "these 2 lines changed, 1 added".
pub struct LineFragmentSnapshot {
    pub fragment_ref: ArtifactRef,
    pub lines: Vec<LineEntry>,
    pub token_estimate: usize,
}

pub struct LineEntry {
    pub key: String,      // stable identity (e.g. "preferred_language" or sha1(line)[..8])
    pub hash: String,     // djb2 of full line content
    pub content: String,  // line text — kept so the differ can re-emit
}
```

**Stable key derivation:** for `memory_context`, recall emits
structured items via `MemoryItem::id`. The renderer joins them as
`format!("- {label}: {value}", ...)`. Track A's snapshot reads
`MemoryItem::id` as the line key directly (not a hash of the
rendered text). This sidesteps "minor formatting change re-keys all
lines" failure mode.

#### A2. Cross-turn anchor state

Add to `LoopDelegate`:

```rust
/// Bundle 16 — last successfully-injected memory_context snapshot.
/// Diffed against the current turn's snapshot in
/// build_dynamic_context to decide between full-block re-injection
/// (drift > threshold) and delta-block injection (drift <= threshold).
///
/// Survives across turns within a session. Cleared on /compact
/// (because the fold is the new baseline).
last_memory_context_snapshot:
    std::sync::Mutex<Option<LineFragmentSnapshot>>,
```

Replaces the current `memory_context_snapshot` field which was
per-iter. Per-iter observability moves into pure tracing — we lose
nothing since iter-2 cache hit was already covered by M2-I.

#### A3. Injection logic

```rust
fn render_memory_context_block(
    current: &LineFragmentSnapshot,
    prior: Option<&LineFragmentSnapshot>,
    raw: &str,
) -> String {
    let diff = match prior {
        None => return wrap_full(raw),  // turn 1 of session — full block
        Some(p) => line_diff(p, current),
    };

    // Drift > 40% → full block cheaper than delta (cache will miss either way)
    if diff.stats().is_significant_change(0.40) {
        return wrap_full(raw);
    }

    // No drift → still wrap the full block to keep system prompt stable
    // for cache_control: ephemeral. This is the EXPECTED path on most
    // turns, and it lets Anthropic prompt cache hit the breakpoint.
    if diff.is_empty() {
        return wrap_full(raw);
    }

    // Small drift → inject delta annotation alongside the full block.
    // We keep the full block so non-cached providers (DeepSeek/Kimi)
    // still see fresh state, and add a <memory_context_changes>
    // wrapper that gives the LLM a *signal* that something shifted.
    // Cached providers will still cache-miss on this turn, but the
    // delta annotation is information the LLM otherwise wouldn't have.
    let mut out = String::new();
    out.push_str(&wrap_full(raw));
    out.push_str("\n\n");
    out.push_str(&render_delta_annotation(&diff));
    out
}
```

`render_delta_annotation` produces:

```
<memory_context_changes vs_prior_turn="true">
+ added: <key=last_skill_search_query>
- removed: <key=task_in_progress>
~ changed: <key=preferred_language> (from "en" to "zh")
</memory_context_changes>
```

#### A4. Why we don't omit the full block

Three reasons:

1. **Un-cached providers** (DeepSeek, Kimi today; OpenAI without
   explicit cache_control) — they have no notion of "saw it last
   turn". Omitting the block means the LLM forgets the context.
2. **Cache misses on system prompt are expected** when state drifts
   (M2-I cache_control is `ephemeral` with ~5min TTL anyway).
3. **The win is signal, not bytes** — the delta annotation tells the
   LLM `preferred_language changed en → zh` which is information the
   raw block (a flat list with no "what changed" semantics) lacks.

Token savings (Track A): negligible bytes — we still ship the full
block. The real win is LLM steerability ("notice that the user
switched languages") and a foundation for Track B (the StructuredFold
delta does omit the full fold because it's much larger).

If we later add real omit-on-cache-hit, that's a Track A2 follow-up
gated on per-provider cache capability detection.

#### A5. Tests

In `agent/context_diff/line_snapshot.rs::tests`:
- `line_diff_detects_added_line`
- `line_diff_detects_removed_line`
- `line_diff_detects_value_change`
- `line_diff_stable_keys_no_diff_on_reorder`
- `line_diff_significant_change_threshold`
- `render_delta_annotation_emits_added_removed_changed_sections`
- `render_delta_annotation_empty_for_no_drift`

In `agent/dispatcher.rs` (extended `effective_system_prompt` tests):
- `dispatcher_first_turn_emits_full_memory_context_block`
- `dispatcher_unchanged_turn_emits_full_block_no_delta_annotation`
- `dispatcher_small_drift_emits_full_block_plus_delta_annotation`
- `dispatcher_significant_drift_emits_full_block_no_annotation`
- `dispatcher_compact_clears_anchor`

#### A6. Telemetry

Replace the 3 current `[M2-D]` log lines with a single line per turn:

```
[M2-D] turn=N memory_context_lines=12 prior=11 added=2 removed=1
       changed=0 unchanged=10 emitted=full+delta cache_state=miss
```

Adds a counter on the existing `TokenBudgetSnapshot` (M2-J) for
`memory_context_delta_emitted`.

---

## Track B — StructuredFold cross-fold delta

### Problem

After `/compact` runs, the conversation history is replaced with a
StructuredFold (8 axes: goals, decisions, blockers, open_questions,
key_files, key_commits, key_facts, next_steps). Each subsequent
compaction trigger re-runs the fold prompt against ALL history,
including the previously folded content. This is:

- Wasteful: the fold prompt LLM call costs ~2-5K tokens
- Cache-busting: each new fold has a different content_hash so the
  system prompt cache breakpoint misses every fold

The original M2-D pilot intent (see `agent/context_diff/mod.rs` line
9-13) was to compute a **fold-vs-fold delta** so the next fold can be
expressed as `prior_fold + delta` rather than re-derived from
scratch.

### Track B spec

#### B1. StructuredFold::diff

In `agent/compact/fold.rs`, add:

```rust
impl StructuredFold {
    /// Compute the per-axis delta from `self` (prior) to `other`
    /// (current). Result is a `FoldDelta` — for each axis,
    /// added / removed / changed entries.
    pub fn diff(&self, other: &StructuredFold) -> FoldDelta { ... }
}

pub struct FoldDelta {
    pub goals: AxisDelta<Goal>,
    pub decisions: AxisDelta<Decision>,
    // ... one AxisDelta per axis
    pub generated_at_ms: i64,
    pub baseline_hash: String,    // hash of the prior fold
}

pub struct AxisDelta<T> {
    pub added: Vec<T>,
    pub removed: Vec<T>,
    pub changed: Vec<(T, T)>,  // (prior, new)
}
```

`AxisDelta` reuses the existing `diff_snapshots` algorithm at the
item level — each axis item must already have a stable `id()` method
(check + add if missing for any axis).

#### B2. Re-fold decision policy

When the dispatcher detects compaction-trigger conditions and a
**prior fold exists**, it has two paths:

```rust
if prior_fold_exists() && delta_is_small(&recent_history) {
    // Path A: incremental delta
    let delta = compute_delta_from_recent_history(&prior_fold);
    let new_fold = prior_fold.apply_delta(&delta);   // local merge, no LLM call
    inject_as("<context_changes_since_last_fold>", delta);
} else {
    // Path B: full re-fold (status quo)
    let new_fold = run_fold_prompt(&full_history).await?;
    inject_as("<conversation_summary>", new_fold);
}
```

`delta_is_small` heuristic:
- Recent history (since last fold) is < N=20 messages, OR
- Fold prompt cost estimate > delta computation cost

The local merge (`apply_delta`) needs to handle:
- New goals/decisions added since last fold → trivial union
- Decision REVERSALS ("we decided X, then reversed to Y") → represented
  as a `changed` AxisDelta entry
- Stale blockers / questions → represented as `removed`

#### B3. Cache-friendly injection layout

Wire layout for an LLM call after compaction:

```
[system_prompt] (cached breakpoint)
[<conversation_summary> base_fold_hash=abc123]
  ... prior_fold rendered as before ...
[/conversation_summary]

[<context_changes_since_last_fold> base=abc123]
  + decision: ...
  ~ blocker: ... → resolved
  - open_question: ...
[/context_changes_since_last_fold]

[user/assistant messages since last fold]
```

The `<conversation_summary>` block is **stable across folds** until
a full re-fold runs — so its cache_control breakpoint hits on every
follow-up turn. Only the `<context_changes_since_last_fold>` block
varies, and it's small (the whole point).

#### B4. Tests

`agent/compact/fold.rs`:
- `fold_diff_detects_added_decision`
- `fold_diff_detects_resolved_blocker_as_removed`
- `fold_diff_detects_decision_reversal_as_changed`
- `fold_diff_apply_delta_roundtrip` (apply(diff(A, B)) == B)
- `fold_diff_empty_when_identical`
- `axis_delta_stats_count_correct`

`agent/dispatcher.rs`:
- `dispatcher_first_compact_runs_full_fold_prompt`
- `dispatcher_second_compact_with_small_drift_skips_fold_prompt`
- `dispatcher_second_compact_with_large_drift_falls_back_to_full_fold`
- `dispatcher_after_full_refold_anchor_resets`

#### B5. Telemetry

```
[M2-D-fold] compact_trigger=N base_hash=abc123 recent_msgs=18
  delta_added=3 delta_changed=1 delta_removed=2
  emitted=delta_only fold_prompt_skipped=true
```

Counter on TokenBudgetSnapshot: `fold_prompt_calls_avoided`,
`fold_delta_token_savings`.

---

## Bundle breakdown

| Bundle | Track | Files | Loose estimate |
| --- | --- | --- | --- |
| Bundle 16-A | A1 + A5 partial | `agent/context_diff/line_snapshot.rs` (+ tests) | 1 commit |
| Bundle 16-B | A2 + A3 + A4 + A6 | `agent/dispatcher.rs` + new `last_memory_context_snapshot` field | 1 commit |
| Bundle 16-C | A5 dispatcher tests | `agent/dispatcher.rs` tests | 1 commit |
| Bundle 17-A | B1 (StructuredFold::diff) | `agent/compact/fold.rs` (+ tests) | 1 commit |
| Bundle 17-B | B2 + B3 (re-fold policy + injection) | `agent/dispatcher.rs` + compact module | 1 commit |
| Bundle 17-C | B4 dispatcher tests | `agent/dispatcher.rs` tests | 1 commit |

Each Bundle = its own PR, matching the `prep/bundleN-<topic>` shape
already used through Bundle 15.

---

## Acceptance criteria

Track A done when:
- `last_memory_context_snapshot` field replaces the per-iter field on
  `LoopDelegate`.
- A turn whose memory_context differs from the prior turn by ≤ 40%
  ships both the full block AND a `<memory_context_changes>` block.
- `[M2-D]` log line emits per-turn (not per-iter) and includes
  `added/removed/changed/unchanged/emitted/cache_state`.
- All Track A tests pass on `cargo test --lib`.
- 50-turn dev session (per `2026-05-21-slice-3-verification.md`)
  shows `<memory_context_changes>` blocks appear on turns where
  recall surfaces new items, and they're absent on no-drift turns.

Track B done when:
- `StructuredFold::diff(&self, &other) -> FoldDelta` exists with
  passing axis-level roundtrip tests.
- The second `/compact` in a session computes a delta rather than
  re-running the fold prompt when recent-message count is below
  threshold.
- `fold_prompt_calls_avoided` increments at least once in the
  50-turn verification session.

---

## Risks

1. **Stable line keys for memory_context** — the renderer must use
   `MemoryItem::id` directly, not a hash of the rendered string. If
   recall ever changes its rendering (e.g. localizes the label), the
   keys flip and every line shows as "changed". Mitigation: gate the
   snapshot construction behind a renderer that explicitly takes
   `MemoryItem` (not the post-rendered string).

2. **Fold delta drift** — `apply(diff(A, B))` must equal `B` exactly,
   or the LLM sees an incrementally-rebuilt fold that doesn't match
   what a fresh fold-prompt run would produce. Mitigation: the
   roundtrip test `fold_diff_apply_delta_roundtrip` exercises this
   for every axis with synthetic data. Production failures (rare
   edge case in axis merging) fall back to full re-fold.

3. **Decision reversals are subtle** — "we decided X" → "we decided
   not-X" might serialize as `removed(X) + added(not-X)` instead of
   `changed(X, not-X)`. The LLM might miss the cause-and-effect.
   Mitigation: when both an `added` and `removed` entry share the
   same `topic_id`, promote them to `changed`. Same heuristic
   `diff_snapshots` already uses at the fragment level (lines 149-163
   of `diff.rs`).

4. **Anthropic cache TTL is 5 min** — even with delta injection, if
   the user goes idle > 5 min between turns, the cache cold-starts
   anyway. Track A's delta annotation is still useful (signals
   change to the LLM), but the cache-miss reduction is bounded by
   user activity.

---

## What's NOT in this phase

- **Per-iter delta injection** — already covered by M2-I; risks
  un-cached providers.
- **Real per-provider cache capability detection** — Anthropic =
  cache_control ephemeral; OpenAI = automatic prompt cache (different
  semantics); DeepSeek + Kimi = no cache. Phase 2 assumes Anthropic
  behavior for the cache_state telemetry; per-provider awareness
  lands in a later Phase 3 (or alongside provider-specific routing).
- **UI for cache hit/miss rates** — TokenBudgetSnapshot already
  surfaces input/cache_read tokens. A dedicated cache-hit gauge
  could be a Phase 2.5 UI polish bundle.
- **Adaptive thresholds** — the 40% drift cutoff in Track A and the
  N=20 message cutoff in Track B are hardcoded constants. Tuning
  data lives in the 50-turn verification logs; we leave hyper-param
  search to a later observability bundle.

---

## Sign-off

This spec replaces the verbal "M2-D phase 2 — make the diff actually
re-inject" backlog item with two concrete bundles. Track A is the
quick win (1-3 commits, 1 PR). Track B is the bigger structural win
(1-3 commits, 1 PR). Both reuse the existing M2-D pilot types
without new abstractions.

Next action: open Bundle 16-A — `LineFragmentSnapshot` +
`line_diff` + tests in `agent/context_diff/line_snapshot.rs`. No
dispatcher changes yet, so the commit is fully bisectable as
"add new module".
