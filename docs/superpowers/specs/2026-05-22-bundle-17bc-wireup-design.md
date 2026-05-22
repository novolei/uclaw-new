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

## 6. Decisions (locked 2026-05-22)

> The three open questions from the draft spec are settled. Recording
> here rather than removing so the spec stays a self-contained record
> of *why* the implementation looks the way it does.

### 6.1 Threshold default = 5; tune via settings within 2 weeks

- **Decision**: ship the wild guess (5) as the default. Don't gate
  implementation on a-priori telemetry.
- **Why**: Bundle 17-C ships the stats in the same PR-pair, so we'll
  have data within days of merge. Tuning is `MemubotConfig::context.fold_delta_threshold`
  — settings-page editable via the same pattern as PR #396's
  StreamSkillThresholdsSection. **No fresh PR needed to retune**.
- **Plan**: After M2 closeout bench, look at the histogram of observed
  delta sizes. If most sessions hit delta = 1-3 (small turns), 5 is
  fine. If most hit 8-15 (chunky turns), bump to 10+. Recorded as
  follow-up in M2 closeout report.

### 6.2 Block format — reuse Bundle 16's `<memory_context_changes>` shape

- **Decision**: borrow the Bundle 16 (PR #384) cross-turn delta
  rendering directly. Tag becomes
  `<context_changes_since_last_fold>` but the inner shape is
  identical: added/removed/changed lists with stable-key identifiers.
- **Why**:
  1. LLM already trained on this shape via Bundle 16 production
     traffic — zero new prompt-design risk.
  2. **prompt-cache friendly** — same prefix convention means the
     M2-I cache breakpoint placement Just Works without case-splitting
     on which delta type fired.
  3. Code reuse — the renderer in `agent/context_diff/diff.rs` can be
     pulled into a shared helper rather than duplicated.
- **Trade-off accepted**: 8-axis FoldDelta is structurally richer than
  Bundle 16's flat memory_context. We collapse to flat list per axis
  with axis name prefix in the change description, e.g.
  `[decisions] "Use rusqlite" → "Use sqlx"`. Loses the per-axis
  structure visually, gains uniformity.
- **Future option**: if LLM grounding suffers, split per-axis blocks in
  a follow-up. Don't pre-optimize.

### 6.3 Trigger M2-I cache breakpoint count on delta-applied path

- **Decision**: yes, delta-applied path increments the prompt-cache
  breakpoint counter (M2-I).
- **Why**: the whole point of using a delta block is to keep prior
  fold + baseline + UCLAW.md as a stable prefix. The M2-I cache
  placement policy reads this counter to decide where to insert the
  `cache_control: { type: "ephemeral" }` marker. Without the count
  bump, M2-I won't see the delta-applied turns as cache-hit candidates
  and will re-place breakpoints suboptimally.
- **Implementation**: in the dispatcher delta-applied branch, after
  rendering the changes block, call the existing M2-I cache helper
  (signature TBD during implementation — likely
  `agent::cache_policy::record_stable_prefix_turn(&mut session_state)`).
- **Out of scope for 17-B**: if the cache helper doesn't exist with
  the right signature yet, add a TODO + use the most general bump
  available, then refactor in a follow-up touching M2-I directly.

## 7. Implementation notes (binding for the impl session)

- **Branch**: `prep/bundle-17b-dispatcher-wireup` (then
  `prep/bundle-17c-telemetry` after merge)
- **Default + config**: `MemubotConfig::context.fold_delta_threshold:
  u32 = 5` with `#[serde(default = "default_fold_delta_threshold")]`.
  Backend `set_fold_delta_threshold` Tauri command with clamp [1, 50].
  No frontend UI in this PR — defer to C1.2 (M2-J full UI).
- **Stat field shape** (Bundle 17-C):
  ```rust
  // agent/token_budget/snapshot.rs
  pub struct FoldDeltaStats {
      pub total_compactions: u32,
      pub delta_applied: u32,
      pub full_refold: u32,
      pub tokens_saved_estimate: u64,
  }
  ```
  `tokens_saved_estimate` ≈ `(prior_fold_tokens * delta_applied) -
  (delta_block_tokens * delta_applied)`. Computed at increment time
  using tiktoken-style estimator already in `agent/dispatcher.rs`.
- **Backward compat**: old MemubotConfig.json without
  `context.fold_delta_threshold` deserializes to 5 via serde default.
  Old `TokenBudgetSnapshot` JSON without `fold_delta_stats`
  deserializes to `FoldDeltaStats::default()` (all zeros).

## 7. Estimated effort

- Bundle 17-B: 0.5-1 day
- Bundle 17-C: 0.5 day
- Total: ~1.5 days

## 8. Closes / unblocks

- task #146: pending → completed
- Drives M2 progress from ~55% → ~60% (out of ~6 sub-tasks left in M2)

---

## 9. Implementation reality reconciliation (2026-05-22, addendum)

> The spec sections above were drafted from the design intent before
> surveying the actual call sites. The §2.1 wording is partially wrong
> on three points; this section is the binding correction. **Treat
> §9 as authoritative where it conflicts with §2.1 / §6.3.**

### 9.1 `/compact` lives in `tauri_commands.rs`, not `dispatcher.rs`

§2.1 says "src-tauri/src/agent/dispatcher.rs — at the /compact trigger
site". That trigger site doesn't exist in `dispatcher.rs`. The actual
`/compact` intercept is `tauri_commands.rs:10007` inside the agent
turn-execution path. The dispatcher only holds `last_memory_context_snapshot`
(Bundle 16's memory_context delta, an unrelated axis) and a comment
on line 55 saying the snapshot is *cleared* on `/compact`.

**Correction**: Bundle 17-B wire-up edits `tauri_commands.rs` at the
`/compact intercept (agent path)` block, between Phase 2 (LLM
`summarize_to_fold`) and Phase 3 (insert placeholder). No dispatcher
edits needed for PR-1 except potentially exposing the
`SafetyManager` / cache-breakpoint helper for the §9.3 counter bump.

### 9.2 Prior fold is **not** currently cached — V52 migration adds storage

§2.1 step 1 says "Read prior `StructuredFold` from session state
(already cached after last /compact)". This is false: today's Phase 3
in `tauri_commands.rs` calls `fold.to_markdown()` and inserts the
markdown as an agent_messages row, then **drops the typed `StructuredFold`**.
There's no in-memory cache and no DB column carrying the structured form.

**Correction**: introduce a new table

```sql
-- V52 (next free V-num per CONTEXT.md registry, confirmed 2026-05-22)
CREATE TABLE IF NOT EXISTS agent_fold_baselines (
    session_id     TEXT PRIMARY KEY,
    fold_json      TEXT NOT NULL,
    baseline_hash  TEXT NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_fold_baselines_session
  ON agent_fold_baselines(session_id);
```

Plus a helper module `agent/compact/baseline.rs` with:

```rust
pub fn load(conn: &Connection, session_id: &str) -> Option<StructuredFold>;
pub fn upsert(conn: &Connection, session_id: &str, fold: &StructuredFold) -> Result<()>;
```

Missing row = no prior fold = full re-fold path (graceful first-compact).
Backward compat: existing DBs without V52 applied just see no rows on
load, which is the correct first-compact behavior.

Rationale for picking a new table vs extending `compaction_markers`:
the `compaction_markers` table is row-per-event (one row per `/compact`
invocation), but the baseline we need is row-per-session (one current
fold per session). Coupling two cardinalities into one table needs an
`is_current` flag or a `MAX(created_at)` join on every read. A
dedicated table is the cleaner shape.

### 9.3 "no LLM" wording was imprecise — savings are on *placeholder size*, not LLM call

§2.1 step 3 says "If `FoldDelta` is 'small' ... → emit
`<context_changes_since_last_fold>` block + reuse prior fold (no LLM)".
The "no LLM" parenthetical is wrong as written — to compute a
FoldDelta you must produce a candidate new fold first, and the only
producer that exists is the LLM-based `summarize_to_fold`. Skipping
the LLM means skipping the producer means there's no delta to compare
against.

**Correction**: the actual savings come from **how the
placeholder is rendered**, not from skipping the LLM call:

1. Always run `summarize_to_fold(history)` → `new_fold` (current cost).
2. `baseline::load(conn, session_id)` → `Option<prior_fold>`.
3. If `Some(prior_fold)` and `prior_fold.diff(&new_fold).total_drift() < threshold`:
   - Placeholder text = `prior_fold.to_markdown()` (the **same** markdown
     as last turn — byte-stable prefix → M2-I prompt cache hits this
     prefix) + `\n\n` + `render_delta_block(&delta)` (the new
     `<context_changes_since_last_fold>` wrapper around added/removed/
     changed lists, per §6.2).
   - The block is much smaller than a fresh full fold markdown, so the
     next turn's system-prompt cache breakpoint sits on a stable
     prefix (prior_fold) + small mutable tail (delta block).
   - Bump M2-I cache-breakpoint counter (§6.3 still applies here).
4. Else (large delta or first compact): placeholder text =
   `new_fold.to_markdown()` (current behavior). Cache breakpoint moves
   to the new fold; future deltas re-baseline against it.
5. Always `baseline::upsert(conn, session_id, &new_fold)` — even on the
   delta-rendered path, so we baseline against the *fresh* fold next
   time, not the increasingly stale prior. (Without this, two
   consecutive small deltas would compute delta against a 2-compacts-stale
   fold and miss intermediate changes.)

This still hits the spec's "30-50% savings on /compact path" bench
target (§5) — the savings are on **next-turn input tokens via cache
hits**, not on summarize_to_fold's own output tokens.

### 9.4 Updated file list for PR-1

Concrete file edits in PR-1 (binding):

| File | Edit |
|---|---|
| `src-tauri/src/db/migrations.rs` | Add `V52_AGENT_FOLD_BASELINES` const + run() entry |
| `src-tauri/src/agent/compact/baseline.rs` | New file — load/upsert helpers + roundtrip tests |
| `src-tauri/src/agent/compact/mod.rs` | `pub mod baseline; pub use baseline::{load_baseline, upsert_baseline};` |
| `src-tauri/src/agent/compact/render.rs` | Add `render_fold_delta_block(&FoldDelta) -> String` (Bundle 16 style, §6.2 borrowed shape) |
| `src-tauri/src/tauri_commands.rs` | At /compact intercept Phase 2/3 boundary: load baseline, decide delta vs full, render placeholder accordingly, upsert baseline. Add `set_fold_delta_threshold` Tauri command. |
| `src-tauri/src/main.rs` | Register `set_fold_delta_threshold` in `invoke_handler!` (per CLAUDE.md adjacent-edits rule) |
| `src-tauri/src/memubot_config.rs` | Add `context.fold_delta_threshold: u32` (default 5, clamp 1..=50 via setter) |
| `CONTEXT.md` | Add V52 row to Active migration registry |

**Out of scope for PR-1**: M2-I cache-breakpoint helper signature — if
no `record_stable_prefix_turn` helper exists yet (§6.3 caveat), leave
a `TODO(M2-I)` comment + use the most general counter bump available.
Don't gate PR-1 on M2-I refactor.

### 9.5 Commit plan (revised, supersedes §4 PR-1)

1. **chore(spec)**: this §9 addendum itself (already in this commit)
2. **feat(db/v52)**: V52_AGENT_FOLD_BASELINES const + run() entry + CONTEXT.md row + `compact/baseline.rs` helpers + module export
3. **feat(/compact)**: tauri_commands.rs delta-rendered branch + render_fold_delta_block + MemubotConfig threshold + set_fold_delta_threshold command + invoke_handler register
4. **test**: baseline roundtrip + small-delta-vs-large-delta /compact placeholder text assertion

Effort revision: 0.5-1 day → **~1 day** for PR-1 (the V52 migration +
baseline helpers are an extra commit beyond §4's 3-commit plan, but
each commit is smaller and more bisectable than the original).
