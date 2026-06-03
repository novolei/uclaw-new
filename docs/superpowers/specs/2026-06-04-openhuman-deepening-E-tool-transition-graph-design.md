# openhuman Deepening · Slice E — Tool Transition Graph Design

**Date:** 2026-06-04
**Status:** Design (recon done; pending spec review → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice E — replace the **boolean undirected `co_used` edges facade** with a **directed, weighted tool-transition graph**, revive the dead `suggest_tool_chain`, and feed its suggestions into the agent context (replacing the hardcoded keyword heuristics).

## Problem

Three disconnected pieces today:

1. **`suggest_tool_chain` is dead** (`proactive/tool_memory.rs:259`). It reads `memory_graph` Procedure nodes whose metadata carries `total_uses`/`success_count` (`list_all_tool_nodes`, `kind='procedure' AND metadata_json LIKE '%"total_uses"%'`), but modern tool usage is recorded into the `tool_stats` adapter facade (`record_tool_usage`), never into Procedure-node metadata. So the read always returns nothing — confirmed by `test_suggest_tool_chain_returns_empty_until_ported`. Only callers are tests; fully orphaned in prod.

2. **The co-occurrence model is boolean + undirected.** `record_co_usage` writes `co_used` edges via `memory_adapter::edges::relate`, whose `edge_key` sorts endpoints (symmetric dedup) — so there's no direction and no count/weight. `edges::neighbors` returns a flat unweighted neighbor set. There's no notion of "tool A tends to be *followed by* tool B," nor how often, nor how recently, nor whether it worked.

3. **Agent tool suggestions are hardcoded keyword heuristics** (`proactive/proactive_recall.rs` ~369: `if query.contains("search") { push("grep_code") }` …), injected into the system prompt's `## 推荐工具:` line via `format_background_for_prompt`. Not learned from actual usage.

Meanwhile the real signal already exists: `agent_turns(session_id, turn_index, role, tool_name, is_error, created_at)` records every tool call in order, so directed sequential transitions (A immediately followed by B in the same session) are derivable.

## Decision (approved 2026-06-04)

Four forks resolved with the user (all the ambitious option):

1. **Materialized `tool_transitions` table (new migration) + a periodic aggregation job** — not an on-the-fly `agent_turns` self-join. A persisted directed weighted graph reads cheaply, supports recency-decay, and matches the C/D `%N` proactive-job pattern.
2. **Directed sequential transitions A→B** (turn_index+1 within a session) — the "tool chain" / Markov "what comes next" semantic — not symmetric same-session co-occurrence.
3. **Replace the hardcoded keyword heuristics** in the proactive-recall context injection with `suggest_tool_chain` output (finish-line: one path, not a parallel one).
4. **Weighted score = `count × recency_decay(last_seen) × success_rate`** (reuse Slice B's `exp(-age_days/half_life)` shape) — not raw count.

## Design

### §1 Migration V58 — `tool_transitions` table

(V56 = automation_approval_requests, V57 = archived_at are taken; **plan reconfirms the next-free V-number against `db/migrations.rs` — spec assumes V58.**)

```sql
CREATE TABLE IF NOT EXISTS tool_transitions (
    space_id      TEXT NOT NULL,
    from_tool     TEXT NOT NULL,
    to_tool       TEXT NOT NULL,
    count         INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    last_seen_ms  INTEGER NOT NULL,
    PRIMARY KEY (space_id, from_tool, to_tool)
);
CREATE INDEX IF NOT EXISTS idx_tool_transitions_from
    ON tool_transitions(space_id, from_tool);
```

Registered in `db/migrations.rs` `run()` after the latest migration, idempotent (`CREATE … IF NOT EXISTS`), mirroring how V57 was added.

Store helpers (home: a new `src-tauri/src/memory_graph/tool_transitions.rs`, or extend `proactive/tool_memory.rs` — plan pins; a focused module keeps it cohesive and freeze-clean):
- `upsert_transition(conn, space_id, from, to, success: bool, now_ms)` — `INSERT … ON CONFLICT(space_id,from_tool,to_tool) DO UPDATE SET count=count+1, success_count=success_count+?success, last_seen_ms=?now`.
- `top_transitions_from(conn, space_id, from_tool, limit) -> Vec<TransitionRow>` — `SELECT to_tool, count, success_count, last_seen_ms WHERE space_id=? AND from_tool=? ORDER BY count DESC LIMIT ?` (scoring applied in Rust at read; raw rows returned).

### §2 Aggregation job (proactive `%N` tick, incremental watermark)

A blocking job (mirror Slice C/D's `spawn_blocking` shape) that turns `agent_turns` into transition counts incrementally:

1. Read the watermark — the `created_at` (or rowid) of the last `agent_turns` row already folded into `tool_transitions`. Persisted in a small marker (plan pins storage: a `settings`-style kv row, e.g. key `tool_transitions_watermark_ms`, or a dedicated single-row table). Default 0 on first run.
2. Select new tool turns since the watermark, ordered by `(session_id, turn_index)`, bounded by `tool_transition_batch_size`:
   ```sql
   SELECT session_id, turn_index, tool_name, is_error, created_at
   FROM agent_turns
   WHERE created_at > ?watermark AND role = 'tool' AND tool_name IS NOT NULL
   ORDER BY session_id, turn_index
   LIMIT ?batch
   ```
   (`role` value for tool turns — plan confirms the exact string used by `record_turn`; recon shows a `role` column + a `tool_name` column.)
3. Walk consecutive rows: for each adjacent pair in the **same session** with `turn_index` strictly increasing (treat as A→B; the natural ordering already enforces it), `upsert_transition(space_id, A.tool_name, B.tool_name, success = !B.is_error, B.created_at)`. **`success` = the destination tool B did not error** — "B tends to follow A and works."
4. Advance the watermark to the max `created_at` processed. Best-effort: per-pair errors log + continue.

`space_id`: agent_turns has `session_id`, not `space_id` directly — the plan pins how to resolve space (likely a fixed/default space, or join `agent_sessions`; if no clean space link, use the default space constant the rest of memory uses). Cross-session pairs are never joined (the `session_id` equality guard).

Cadence: a `%N` tick (e.g. `%120` ≈ 1h, staggered from C/D's `%360`). Gated by config.

### §3 Revived `suggest_tool_chain`

Repoint `suggest_tool_chain` (keep the public name + `ToolSuggestion` return type) to read `tool_transitions`:

```
suggest_tool_chain(space_id, last_tool: Option<&str>, _task) -> Vec<ToolSuggestion>:
    rows = top_transitions_from(space_id, last_tool, scan_limit)   // empty if last_tool None
    for r in rows:
        recency = exp(-(age_days(r.last_seen_ms, now) / half_life))   // Slice B shape; 1.0 if half_life<=0
        success_rate = if r.count>0 { r.success_count / r.count } else { 0.0 }
        score = (r.count as f64) * recency * success_rate
    sort by score desc, take top-N → ToolSuggestion{tool_name=to_tool, reason, success_rate, priority=score}
```

`reason` summarizes (e.g. "常接在 {last_tool} 之后, {count} 次, 成功率 {pct}%"). `last_tool == None` (session has no prior tool) → empty (graceful). The old per-tool-stats + `edges::neighbors(co_used)` logic is removed.

### §4 Agent-context injection (replace keyword heuristics)

In `proactive/proactive_recall.rs`, replace the hardcoded `tool_suggestions` keyword block with:
- Derive `last_tool` = the most recent tool turn for the current session (`SELECT tool_name FROM agent_turns WHERE session_id=? AND role='tool' AND tool_name IS NOT NULL ORDER BY turn_index DESC LIMIT 1`). The plan pins whether `session_id` is available at this call site; if not, thread it in or fall back to `None`.
- `suggest_tool_chain(space_id, last_tool, user_query)` → take the top tool_names → existing `format_background_for_prompt` `## 推荐工具:` line.
- Empty result → omit the line (current code already guards `if !tool_suggestions.is_empty()`).

### §5 Config (`memubot_config.rs` + `MemoryOsRuntimeConfig`)

Mirror C/D exactly (field + `#[serde(default="fn")]` + default fn in `MemoryOsConfig`; AND the runtime `MemoryOsRuntimeConfig` struct + From-mapping + test-ctor `enabled=false` + Default-impl `enabled=true` — **both structs**, the Slice-D lesson):
- `tool_transitions_enabled: bool` (default `true`) — gates the aggregation job + suggestion.
- `tool_transition_recency_half_life_days: f64` (default `30.0`).
- `tool_transition_batch_size: u32` (default `500`; `0` disables the job) — per-aggregation-run cap on agent_turns rows.

### §6 Finish-line cleanup

Once `suggest_tool_chain` reads `tool_transitions`, the undirected path is dead: `record_co_usage`, the `co_used` writes, and `edges::neighbors(co_used)` reads have no remaining prod caller. **Plan confirms no live callers, then removes them** (and the now-unused `list_all_tool_nodes` / `ToolNodeStats` Procedure-reading path if orphaned). If `edges.rs` itself has other live users (non-`co_used` kinds), leave the facade; only remove the tool-co-usage usage.

## Data flow (after E)

```
agent turn loop → agent_turns(session_id, turn_index, tool_name, is_error, created_at)
proactive %N tick → aggregation job: new turns since watermark → A→B pairs (same session)
                    → upsert tool_transitions(count++, success_count += !B.is_error, last_seen)
agent context assembly (proactive_recall) → last_tool = session's latest tool turn
                    → suggest_tool_chain(space, last_tool) → score = count·recency·success
                    → top-N → "## 推荐工具:" line in system prompt
```

## Out of scope

Task-description semantic keying (`_task_description` stays unused — future); multi-step chain prediction beyond 1-hop (just next-tool); per-tool success stats UI; the existing `tool_stats` facade (left as-is; E reads transitions, not per-tool stats); openhuman has no reference impl (E is uClaw-native).

## Error handling

Aggregation job best-effort: DB-lock fail → skip tick; per-pair upsert error → log + continue; watermark only advances over successfully-scanned rows. `suggest_tool_chain` read-only + null-safe (no transitions / `last_tool=None` → empty, never panics). Injection failure → omit the suggestion line (never block the turn). Migration V58 additive.

## Testing

1. **`upsert_transition`**: first call inserts count=1; repeat increments count; success=false leaves success_count unchanged; last_seen updates.
2. **Aggregation**: seed `agent_turns` (one session: toolA@idx0 ok, toolB@idx1 ok, toolC@idx2 error) → job produces A→B (count1, success1), B→C (count1, success0, since C errored); a second session's turns don't cross-link; watermark advances; re-run with no new turns is a no-op.
3. **`suggest_tool_chain`**: with transitions A→B(count10,succ9,recent) and A→C(count10,succ2,old), B outranks C (success + recency); `last_tool=None` → empty.
4. **Scoring**: equal count+success, fresher `last_seen` ranks higher; `half_life<=0` → recency 1.0.
5. **Config**: 3 knobs deserialize with defaults in `MemoryOsConfig`; present in `MemoryOsRuntimeConfig` (all sites).
6. **Injection**: proactive_recall with a session whose last tool is A → suggestion line lists A's top successors; no session history → no line.
7. `cargo build`/clippy clean; `cargo test --lib` for the touched modules + the broad dependent run (Slice-C/D lesson — fixtures building `agent_turns`/`tool_transitions`); grep gate: `record_co_usage`/`co_used` have no remaining prod callers after removal.

## Scope / files

| File | Change |
|---|---|
| `db/migrations.rs` | V58 `tool_transitions` table + index, registered in `run()` |
| `memory_graph/tool_transitions.rs` (new) | `upsert_transition`, `top_transitions_from`, `TransitionRow`, the aggregation `run_tool_transition_aggregation_blocking(conn, batch, now)` + watermark read/write |
| `proactive/tool_memory.rs` | repoint `suggest_tool_chain` to `tool_transitions` + scoring; remove dead `record_co_usage`/Procedure-read path |
| `proactive/service.rs` | `%N` aggregation job (spawn_blocking) + `MemoryOsRuntimeConfig` fields |
| `proactive/proactive_recall.rs` | replace keyword heuristics with `suggest_tool_chain(last_tool)` |
| `memubot_config.rs` | 3 `tool_transition*` config knobs |

## Risk

Med. New table + an incremental aggregation job + a read-path rewrite + a context-injection swap. Main risks: (1) **watermark correctness** — must not double-count on re-run nor skip rows; bounded-batch + `created_at` watermark, with a test for re-run idempotency; (2) **space_id resolution** from agent_turns (no direct column) — plan pins; (3) the **two-config-struct lesson** (D) — knobs must land in both `MemoryOsConfig` and `MemoryOsRuntimeConfig`; (4) **session_id availability** in proactive_recall for `last_tool` — plan pins/threads; (5) the Slice-C/D **fixture lesson** — new table read by the job can break partial-schema fixtures → route through `db::migrations::run`. Bisectable: table+store → aggregation → suggest rewrite → config → injection+cleanup → verify. After E, the agent's suggested tools are learned from its own directed, recency/success-weighted tool-usage history instead of hardcoded keywords.
