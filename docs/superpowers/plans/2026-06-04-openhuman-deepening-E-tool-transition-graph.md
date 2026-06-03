# openhuman Slice E — Tool Transition Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the boolean undirected `co_used` edges facade with a directed, weighted tool-transition graph; revive `suggest_tool_chain`; feed its suggestions into the agent context (replacing hardcoded keyword heuristics).

**Architecture:** A new `tool_transitions` table (V58) records directed A→B counts/success/recency, aggregated incrementally from `agent_turns` by a proactive `%N` job (watermark in `settings`). `suggest_tool_chain` reads it with a `count × recency_decay × success_rate` score. The proactive-recall context injection calls it instead of keyword matching.

**Tech Stack:** Rust, rusqlite, tokio spawn_blocking, Tauri proactive service. Config via `memubot_config.rs`.

**Spec:** `docs/superpowers/specs/2026-06-04-openhuman-deepening-E-tool-transition-graph-design.md`

---

## Pinned facts (from recon — do not re-derive)

- **Next migration = V58** (V56 automation_approval_requests, V57 archived_at taken). V57 is registered in `db/migrations.rs` `run()` as: `for stmt in SQL_V57.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) { if let Err(e) = conn.execute(stmt, []) { tracing::warn!("V57 stmt skipped: {} :: {}", e, stmt); } }`. Mirror this exactly for V58.
- **All in ONE db** (`uclaw.db`): `agent_turns`, `agent_sessions`, `memory_nodes`, `settings`, and the new `tool_transitions` share `refs.memory_graph_store.conn` (`Arc<std::sync::Mutex<Connection>>`, field is `pub(crate) conn`). One lock covers reads of agent_turns + writes of tool_transitions.
- **Tool turn shape:** `agent_turns` row with `role = "tool"` (exact string, set in `agent/tool_dispatch/mod.rs:597`), `tool_name = Some(...)`, `is_error` (0/1), `created_at` (i64 ms), `turn_index` (increments across ALL turns incl. `assistant` turns). **So two tool turns are NOT turn_index-adjacent** — an assistant turn sits between. A→B = consecutive **tool-role** turns within a session ordered by `turn_index`.
- **space_id:** `agent_turns` has no space_id; `agent_sessions(id, space_id DEFAULT 'default')` does. Resolve via `LEFT JOIN agent_sessions s ON s.id = t.session_id`, `COALESCE(s.space_id,'default')`.
- **settings kv table:** `CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)`. Read: `SELECT value FROM settings WHERE key=?1` (`.optional()`); write: `INSERT OR REPLACE INTO settings(key,value) VALUES(?1,?2)`.
- **`ToolSuggestion`** (keep): `proactive/tool_memory.rs:65` `{ tool_name: String, reason: String, success_rate: f32, priority: f32 }`.
- **`ToolUsageMemoryManager`** (`tool_memory.rs:111`): fields `store: Arc<MemoryGraphStore>`, `adapter: Arc<dyn MemoryAdapter>`. `suggest_tool_chain` is a method here.
- **`ProactiveRecallService`** (`proactive/proactive_recall.rs:72`): holds `store: Arc<MemoryGraphStore>` + `tool_memory: Arc<ToolUsageMemoryManager>`. The keyword block is inside `build_background_context(&self, space_id: &str, user_query: &str, max_results: usize)` at lines 369-385; the `## 推荐工具:` formatting is in `format_background_for_prompt` (the `ctx.tool_suggestions` Vec<String>).
- **Dead/removable:** `record_co_usage` (zero prod callers), and after the suggest rewrite: `list_all_tool_nodes` + `ToolNodeStats` (only used by the old suggest path). **Keep** `record_tool_usage` + the `tool_stats` facade (live: called from `proactive/service.rs:945`).
- **Slice C/D `%360` job template** (`proactive/service.rs:1442`): `if cfg && tick % N == 0 { let store = refs.memory_graph_store.clone(); let now=...; spawn_blocking(move || { let conn = store.conn.lock()...; helper(&conn, ...) }).await.unwrap_or_default(); }`.
- **Two config structs** (Slice-D lesson): `MemoryOsConfig` (serde, `memubot_config.rs`) AND `MemoryOsRuntimeConfig` (`proactive/service.rs` — struct + From-map + test-ctor `enabled=false` + Default-impl `enabled=true`).
- **Test fixtures** route through `crate::db::migrations::run(&conn)` (full schema incl. V58 after Task 1).

---

## Task 1: V58 migration + `tool_transitions` store module

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Create: `src-tauri/src/memory_graph/tool_transitions.rs`
- Modify: `src-tauri/src/memory_graph/mod.rs` (add `pub mod tool_transitions;`)

- [ ] **Step 1: Add the V58 const + register it in `run()`**

In `db/migrations.rs`, after `SQL_V57`:
```rust
const SQL_V58: &str = "\
CREATE TABLE IF NOT EXISTS tool_transitions (\
    space_id      TEXT NOT NULL,\
    from_tool     TEXT NOT NULL,\
    to_tool       TEXT NOT NULL,\
    count         INTEGER NOT NULL DEFAULT 0,\
    success_count INTEGER NOT NULL DEFAULT 0,\
    last_seen_ms  INTEGER NOT NULL,\
    PRIMARY KEY (space_id, from_tool, to_tool)\
);\
CREATE INDEX IF NOT EXISTS idx_tool_transitions_from ON tool_transitions(space_id, from_tool);\
";
```
In `run()`, after the V57 block:
```rust
    tracing::debug!("Running migration V58: tool_transitions (directed weighted tool graph)");
    for stmt in SQL_V58.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V58 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 2: Create the module skeleton + register it**

Create `src-tauri/src/memory_graph/tool_transitions.rs`:
```rust
//! openhuman-E — directed, weighted tool-transition graph.
//!
//! Records "tool A was followed by tool B in the same session" as a
//! directed edge with a count, a success count (B did not error), and a
//! recency timestamp. Aggregated incrementally from `agent_turns` by a
//! proactive job; read by `suggest_tool_chain` with a
//! `count × recency_decay × success_rate` score. Replaces the old
//! boolean-undirected `co_used` edges facade.

use rusqlite::{params, Connection, OptionalExtension};

/// One outgoing transition row (the scoring is applied by the caller).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionRow {
    pub to_tool: String,
    pub count: i64,
    pub success_count: i64,
    pub last_seen_ms: i64,
}
```
In `src-tauri/src/memory_graph/mod.rs`, add `pub mod tool_transitions;` next to the other `pub mod` lines.

- [ ] **Step 3: Write failing tests for `upsert_transition` + `top_transitions_from`**

Add a `#[cfg(test)] mod tests` to `tool_transitions.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&c).unwrap();
        c
    }

    #[test]
    fn upsert_inserts_then_increments() {
        let c = conn();
        upsert_transition(&c, "default", "a", "b", true, 1000).unwrap();
        upsert_transition(&c, "default", "a", "b", false, 2000).unwrap();
        let rows = top_transitions_from(&c, "default", "a", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].to_tool, "b");
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[0].success_count, 1); // only the first was success
        assert_eq!(rows[0].last_seen_ms, 2000);
    }

    #[test]
    fn top_transitions_from_filters_and_orders_by_count() {
        let c = conn();
        for _ in 0..3 { upsert_transition(&c, "default", "a", "b", true, 100).unwrap(); }
        upsert_transition(&c, "default", "a", "c", true, 100).unwrap();
        upsert_transition(&c, "default", "x", "y", true, 100).unwrap(); // different from_tool
        let rows = top_transitions_from(&c, "default", "a", 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.to_tool.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
        assert_eq!(rows[0].count, 3);
    }
}
```

- [ ] **Step 4: Run tests — verify FAIL**

Run: `cd src-tauri && cargo test --lib memory_graph::tool_transitions 2>&1 | tail -20`
Expected: `cannot find function upsert_transition` / `top_transitions_from`.

- [ ] **Step 5: Implement `upsert_transition` + `top_transitions_from`**

Add to `tool_transitions.rs` (before the test module):
```rust
/// Bump (or insert) the directed A→B transition. `success` = B did not error.
pub fn upsert_transition(
    conn: &Connection,
    space_id: &str,
    from_tool: &str,
    to_tool: &str,
    success: bool,
    last_seen_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO tool_transitions
             (space_id, from_tool, to_tool, count, success_count, last_seen_ms)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)
         ON CONFLICT(space_id, from_tool, to_tool) DO UPDATE SET
             count = count + 1,
             success_count = success_count + ?4,
             last_seen_ms = ?5",
        params![space_id, from_tool, to_tool, success as i64, last_seen_ms],
    )?;
    Ok(())
}

/// Outgoing transitions from `from_tool`, highest count first (scoring applied
/// by the caller). Read-only.
pub fn top_transitions_from(
    conn: &Connection,
    space_id: &str,
    from_tool: &str,
    limit: usize,
) -> rusqlite::Result<Vec<TransitionRow>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT to_tool, count, success_count, last_seen_ms
         FROM tool_transitions
         WHERE space_id = ?1 AND from_tool = ?2
         ORDER BY count DESC
         LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![space_id, from_tool, limit as i64], |r| {
            Ok(TransitionRow {
                to_tool: r.get(0)?,
                count: r.get(1)?,
                success_count: r.get(2)?,
                last_seen_ms: r.get(3)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}
```

- [ ] **Step 6: Run tests — verify PASS**

Run: `cd src-tauri && cargo test --lib memory_graph::tool_transitions 2>&1 | tail -20` → both pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/memory_graph/tool_transitions.rs src-tauri/src/memory_graph/mod.rs
git commit -m "feat(memory): V58 tool_transitions table + upsert/top_transitions_from store helpers (Slice E)"
```

---

## Task 2: Aggregation orchestrator + watermark

**Files:**
- Modify: `src-tauri/src/memory_graph/tool_transitions.rs`

- [ ] **Step 1: Write failing tests for the aggregation + watermark**

Add to the test module (reuse the `conn()` helper). Add a seed helper first:
```rust
fn seed_session(c: &Connection, sid: &str, space: &str) {
    c.execute(
        "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, 'test', '{}', 0, 0, 0, 0, 0)",
        params![sid, space],
    ).unwrap();
}
fn seed_tool_turn(c: &Connection, sid: &str, idx: i64, tool: &str, is_err: bool, ts: i64) {
    c.execute(
        "INSERT INTO agent_turns (id, session_id, turn_index, role, content, tool_name, tool_args, tool_result, reasoning, is_error, duration_ms, created_at)
         VALUES (?1, ?2, ?3, 'tool', NULL, ?4, NULL, NULL, NULL, ?5, 0, ?6)",
        params![format!("{sid}-{idx}"), sid, idx, tool, is_err as i64, ts],
    ).unwrap();
}
```
Then:
```rust
#[test]
fn aggregation_pairs_consecutive_tool_turns_in_session() {
    let c = conn();
    seed_session(&c, "s1", "default");
    // turn_index gaps (assistant turns between) — pairing is by consecutive TOOL turns
    seed_tool_turn(&c, "s1", 1, "read", false, 100);
    seed_tool_turn(&c, "s1", 3, "edit", false, 200);
    seed_tool_turn(&c, "s1", 5, "bash", true, 300); // bash errored
    // a different session must NOT cross-link
    seed_session(&c, "s2", "default");
    seed_tool_turn(&c, "s2", 1, "grep", false, 400);

    let out = run_tool_transition_aggregation_blocking(&c, 500);
    assert_eq!(out.pairs_processed, 2); // read->edit, edit->bash (s2 has only 1 turn)
    let from_read = top_transitions_from(&c, "default", "read", 10).unwrap();
    assert_eq!(from_read[0].to_tool, "edit");
    assert_eq!(from_read[0].success_count, 1); // edit ok
    let from_edit = top_transitions_from(&c, "default", "edit", 10).unwrap();
    assert_eq!(from_edit[0].to_tool, "bash");
    assert_eq!(from_edit[0].success_count, 0); // bash errored → success=false
    assert_eq!(out.new_watermark_ms, 400);
}

#[test]
fn aggregation_is_incremental_and_idempotent_on_rerun() {
    let c = conn();
    seed_session(&c, "s1", "default");
    seed_tool_turn(&c, "s1", 1, "read", false, 100);
    seed_tool_turn(&c, "s1", 3, "edit", false, 200);
    let out1 = run_tool_transition_aggregation_blocking(&c, 500);
    assert_eq!(out1.pairs_processed, 1);
    // re-run with no new turns → no double-count
    let out2 = run_tool_transition_aggregation_blocking(&c, 500);
    assert_eq!(out2.pairs_processed, 0);
    let rows = top_transitions_from(&c, "default", "read", 10).unwrap();
    assert_eq!(rows[0].count, 1); // not 2
}
```

- [ ] **Step 2: Run — verify FAIL**

Run: `cd src-tauri && cargo test --lib memory_graph::tool_transitions::tests::aggregation 2>&1 | tail -20`
Expected: `cannot find function run_tool_transition_aggregation_blocking`.

- [ ] **Step 3: Implement watermark helpers + the aggregation orchestrator**

Add to `tool_transitions.rs`:
```rust
const WATERMARK_KEY: &str = "tool_transitions_watermark_ms";

fn read_watermark(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![WATERMARK_KEY],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|s| s.parse::<i64>().ok())
    .unwrap_or(0)
}

fn write_watermark(conn: &Connection, ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![WATERMARK_KEY, ms.to_string()],
    )?;
    Ok(())
}

/// A tool turn, just the fields the aggregation needs.
struct TurnLite {
    session_id: String,
    space_id: String,
    tool_name: String,
    is_error: bool,
    created_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolTransitionOutcome {
    pub pairs_processed: usize,
    pub new_watermark_ms: i64,
}

/// openhuman-E — fold new `agent_turns` (since the watermark) into the
/// directed transition graph. Loads tool turns in `created_at` order (so the
/// watermark advances correctly), re-sorts in memory by (session, turn_index),
/// and pairs each tool turn with the next tool turn IN THE SAME SESSION
/// (A→B; success = B did not error). A pair split exactly across the batch
/// boundary is undercounted by 1 — acceptable statistical noise; it self-
/// corrects when the same sequence recurs.
pub fn run_tool_transition_aggregation_blocking(
    conn: &Connection,
    batch_size: usize,
) -> ToolTransitionOutcome {
    let mut outcome = ToolTransitionOutcome::default();
    if batch_size == 0 {
        outcome.new_watermark_ms = read_watermark(conn);
        return outcome;
    }
    let watermark = read_watermark(conn);
    outcome.new_watermark_ms = watermark;

    // Load in created_at order so the watermark covers a contiguous prefix.
    let mut rows: Vec<(i64, TurnLite)> = {
        let mut stmt = match conn.prepare(
            "SELECT t.session_id, COALESCE(s.space_id,'default'), t.tool_name,
                    t.is_error, t.created_at, t.turn_index
             FROM agent_turns t
             LEFT JOIN agent_sessions s ON s.id = t.session_id
             WHERE t.created_at > ?1 AND t.role = 'tool' AND t.tool_name IS NOT NULL
             ORDER BY t.created_at ASC
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "tool_transitions: prepare failed");
                return outcome;
            }
        };
        let mapped = stmt.query_map(params![watermark, batch_size as i64], |r| {
            Ok((
                r.get::<_, i64>(5)?, // turn_index
                TurnLite {
                    session_id: r.get(0)?,
                    space_id: r.get(1)?,
                    tool_name: r.get(2)?,
                    is_error: r.get::<_, i64>(3)? != 0,
                    created_at: r.get(4)?,
                },
            ))
        });
        match mapped {
            Ok(m) => m.filter_map(Result::ok).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "tool_transitions: query failed");
                return outcome;
            }
        }
    };
    if rows.is_empty() {
        return outcome;
    }

    // Watermark = max created_at in this (created_at-ordered) batch.
    let new_watermark = rows.iter().map(|(_, t)| t.created_at).max().unwrap_or(watermark);

    // Re-sort by (session_id, turn_index) so consecutive same-session rows are
    // adjacent tool turns; then pair them.
    rows.sort_by(|(ia, a), (ib, b)| {
        a.session_id.cmp(&b.session_id).then(ia.cmp(ib))
    });
    for w in rows.windows(2) {
        let (_, a) = &w[0];
        let (_, b) = &w[1];
        if a.session_id == b.session_id {
            if let Err(e) =
                upsert_transition(conn, &b.space_id, &a.tool_name, &b.tool_name, !b.is_error, b.created_at)
            {
                tracing::warn!(error = %e, "tool_transitions: upsert failed");
            } else {
                outcome.pairs_processed += 1;
            }
        }
    }

    if let Err(e) = write_watermark(conn, new_watermark) {
        tracing::warn!(error = %e, "tool_transitions: watermark write failed");
    }
    outcome.new_watermark_ms = new_watermark;
    outcome
}
```

- [ ] **Step 4: Run — verify PASS**

Run: `cd src-tauri && cargo test --lib memory_graph::tool_transitions 2>&1 | tail -20` → all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_graph/tool_transitions.rs
git commit -m "feat(memory): tool-transition aggregation orchestrator (agent_turns → A→B graph, watermark) for Slice E"
```

---

## Task 3: Rewrite `suggest_tool_chain` + remove dead Procedure path

**Files:**
- Modify: `src-tauri/src/proactive/tool_memory.rs`

- [ ] **Step 1: Write the failing test for the rewritten `suggest_tool_chain`**

In `tool_memory.rs` tests, add (the manager owns `store`; seed via its conn). Add a helper that seeds the transition graph + a recent tool turn so `last_tool` resolves:
```rust
#[tokio::test]
async fn suggest_tool_chain_ranks_by_weighted_score() {
    let store = make_test_store();
    let manager = make_manager(store.clone());
    {
        let c = store.conn.lock().unwrap();
        // last tool used in the space = "read"
        c.execute("INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES ('s1','default','t','{}',0,0,0,0,0)", []).unwrap();
        c.execute("INSERT INTO agent_turns (id, session_id, turn_index, role, content, tool_name, tool_args, tool_result, reasoning, is_error, duration_ms, created_at) VALUES ('t1','s1',1,'tool',NULL,'read',NULL,NULL,NULL,0,0,9999)", []).unwrap();
        // transitions from read: edit (high count+success), grep (high count, low success)
        for ts in 0..10 { crate::memory_graph::tool_transitions::upsert_transition(&c,"default","read","edit",true, 9000+ts).unwrap(); }
        for ts in 0..10 { crate::memory_graph::tool_transitions::upsert_transition(&c,"default","read","grep", ts<2, 1000+ts).unwrap(); }
    }
    let s = manager.suggest_tool_chain("default", "anything").unwrap();
    assert!(!s.is_empty());
    assert_eq!(s[0].tool_name, "edit"); // higher success + more recent → outranks grep
}

#[tokio::test]
async fn suggest_tool_chain_empty_when_no_prior_tool() {
    let store = make_test_store();
    let manager = make_manager(store);
    let s = manager.suggest_tool_chain("default", "x").unwrap();
    assert!(s.is_empty());
}
```
(If `make_test_store`/`make_manager` differ in the existing test module, match their actual names. The old `test_suggest_tool_chain_returns_empty_until_ported` test — UPDATE or remove it, since the behavior is now ported.)

- [ ] **Step 2: Run — verify FAIL** (old impl reads Procedure nodes → wrong result / signature)

Run: `cd src-tauri && cargo test --lib proactive::tool_memory 2>&1 | tail -20`

- [ ] **Step 3: Rewrite `suggest_tool_chain`; add a `recency_half_life_days` param**

Replace the body of `suggest_tool_chain`. New signature adds `recency_half_life_days: f64`:
```rust
/// openhuman-E — suggest the tools that most often, most recently, and most
/// successfully follow the space's most-recent tool. Reads the directed
/// `tool_transitions` graph. Score = count × recency_decay × success_rate.
pub fn suggest_tool_chain(
    &self,
    space_id: &str,
    _task_description: &str,
    recency_half_life_days: f64,
) -> Result<Vec<ToolSuggestion>, crate::error::Error> {
    let conn = self
        .store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

    // last tool used in this space (the from-node for the suggestion)
    let last_tool: Option<String> = conn
        .query_row(
            "SELECT t.tool_name
             FROM agent_turns t
             JOIN agent_sessions s ON s.id = t.session_id
             WHERE s.space_id = ?1 AND t.role = 'tool' AND t.tool_name IS NOT NULL
             ORDER BY t.created_at DESC
             LIMIT 1",
            rusqlite::params![space_id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(crate::error::Error::Database)?;

    let Some(last) = last_tool else {
        return Ok(Vec::new());
    };

    let rows = crate::memory_graph::tool_transitions::top_transitions_from(&conn, space_id, &last, 50)
        .map_err(crate::error::Error::Database)?;

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut suggestions: Vec<ToolSuggestion> = rows
        .into_iter()
        .map(|r| {
            let success_rate = if r.count > 0 {
                r.success_count as f32 / r.count as f32
            } else {
                0.0
            };
            let recency = if recency_half_life_days <= 0.0 {
                1.0_f64
            } else {
                let age_days = ((now_ms - r.last_seen_ms).max(0) as f64) / 86_400_000.0;
                (-(age_days / recency_half_life_days)).exp()
            };
            let priority = (r.count as f64) * recency * (success_rate as f64);
            ToolSuggestion {
                tool_name: r.to_tool.clone(),
                reason: format!(
                    "常接在 {} 之后（{} 次, 成功率 {:.0}%）",
                    last, r.count, success_rate * 100.0
                ),
                success_rate,
                priority: priority as f32,
            }
        })
        .filter(|s| s.priority > 0.0)
        .collect();

    suggestions.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    suggestions.truncate(10);
    Ok(suggestions)
}
```

- [ ] **Step 4: Remove the dead Procedure-reading path**

Delete `list_all_tool_nodes` (the `fn list_all_tool_nodes` method) and the `ToolNodeStats` struct + its `impl Default` — they were only used by the old `suggest_tool_chain`. Confirm no other references: `grep -n "list_all_tool_nodes\|ToolNodeStats" src-tauri/src` returns nothing after deletion. Remove now-unused imports (e.g. `MemoryNode`/`MemoryNodeKind` if only used there — let the compiler tell you).

- [ ] **Step 5: Run — verify PASS**

Run: `cd src-tauri && cargo test --lib proactive::tool_memory 2>&1 | tail -20` → green.
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/proactive/tool_memory.rs
git commit -m "feat(memory): rewrite suggest_tool_chain over tool_transitions (weighted) + drop dead Procedure path (Slice E)"
```

---

## Task 4: Config knobs (both structs)

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`
- Modify: `src-tauri/src/proactive/service.rs`

- [ ] **Step 1: `memubot_config.rs` — default fns + `MemoryOsConfig` fields + Default-impl values**

Mirror the Slice-D `spaced_repetition_*` pattern exactly. Default fns:
```rust
fn default_tool_transitions_enabled() -> bool { true }
fn default_tool_transition_recency_half_life_days() -> f64 { 30.0 }
fn default_tool_transition_batch_size() -> u32 { 500 }
```
`MemoryOsConfig` fields (after the `spaced_repetition_*` fields):
```rust
/// openhuman-E — gates the tool-transition aggregation job + suggestion.
#[serde(default = "default_tool_transitions_enabled")]
pub tool_transitions_enabled: bool,
/// openhuman-E — recency half-life (days) for the suggest_tool_chain score.
#[serde(default = "default_tool_transition_recency_half_life_days")]
pub tool_transition_recency_half_life_days: f64,
/// openhuman-E — per-aggregation-run cap on agent_turns rows scanned. 0 disables.
#[serde(default = "default_tool_transition_batch_size")]
pub tool_transition_batch_size: u32,
```
Default impl (after the `spaced_repetition_*` literals):
```rust
tool_transitions_enabled: true,
tool_transition_recency_half_life_days: 30.0,
tool_transition_batch_size: 500,
```

- [ ] **Step 2: `proactive/service.rs` `MemoryOsRuntimeConfig` — 4 sites**

Add the same 3 fields after the `spaced_repetition_*` lines in EACH of: (1) the struct definition, (2) the From/builder mapping (`tool_transitions_enabled: cfg.tool_transitions_enabled,` etc.), (3) the test/"off in tests" constructor (`tool_transitions_enabled: false,` + the two values), (4) the `impl Default` (`tool_transitions_enabled: true,` + the two values). Grep `spaced_repetition_importance_threshold` in service.rs to find all 4 sites; add the 3 tool_transition lines after each.

- [ ] **Step 3: Add a defaults test**

In `memubot_config.rs` tests, mirror `memory_os_config_spaced_repetition_defaults`:
```rust
#[test]
fn memory_os_config_tool_transitions_defaults() {
    let cfg = MemoryOsConfig::default();
    assert!(cfg.tool_transitions_enabled);
    assert!((cfg.tool_transition_recency_half_life_days - 30.0).abs() < f64::EPSILON);
    assert_eq!(cfg.tool_transition_batch_size, 500);
}
```

- [ ] **Step 4: Verify + Commit**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty. `cargo test --lib memubot_config proactive 2>&1 | tail -5` → green.
```bash
git add src-tauri/src/memubot_config.rs src-tauri/src/proactive/service.rs
git commit -m "feat(memory): tool_transition* config knobs (both config structs) for Slice E"
```

---

## Task 5: Proactive job wiring + recall injection + remove `record_co_usage`

**Files:**
- Modify: `src-tauri/src/proactive/service.rs`
- Modify: `src-tauri/src/proactive/proactive_recall.rs`
- Modify: `src-tauri/src/proactive/tool_memory.rs`

- [ ] **Step 1: Add the aggregation job to the proactive tick**

In `proactive/service.rs`, after the Slice-D spaced-repetition block, add (mirror its shape):
```rust
// openhuman-E — Tool-transition aggregation. Folds new agent_turns into the
// directed weighted graph. Runs every 120 ticks (~1h), staggered from the
// %360 memory jobs. SQL-only; batch-bounded.
if refs.memory_os.tool_transitions_enabled
    && refs.memory_os.tool_transition_batch_size > 0
    && refs.tick_count.load(Ordering::SeqCst) % 120 == 0
{
    let store = refs.memory_graph_store.clone();
    let batch = refs.memory_os.tool_transition_batch_size as usize;
    let outcome = tokio::task::spawn_blocking(
        move || -> crate::memory_graph::tool_transitions::ToolTransitionOutcome {
            let conn = match store.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, "[ProactiveService] tool_transitions: DB lock failed");
                    return Default::default();
                }
            };
            crate::memory_graph::tool_transitions::run_tool_transition_aggregation_blocking(&conn, batch)
        },
    )
    .await
    .unwrap_or_default();
    if outcome.pairs_processed > 0 {
        tracing::info!(
            pairs = outcome.pairs_processed,
            watermark = outcome.new_watermark_ms,
            "[ProactiveService] tool_transitions aggregation"
        );
    }
}
```

- [ ] **Step 2: Thread the half-life into `ProactiveRecallService` + swap the injection**

In `proactive_recall.rs`:
- Add a field `tool_transition_half_life_days: f64` to `ProactiveRecallService` and a matching `new(...)` parameter (placed last). Update the `new` body to store it.
- In `build_background_context`, replace the hardcoded keyword block (lines 369-385) with:
```rust
// 3. 工具使用建议（基于学习到的有向工具转移图）
let tool_suggestions: Vec<String> = self
    .tool_memory
    .suggest_tool_chain(space_id, user_query, self.tool_transition_half_life_days)
    .map(|sugs| sugs.into_iter().map(|s| s.tool_name).collect())
    .unwrap_or_default();
```
- Find the `ProactiveRecallService::new(...)` call site (grep `ProactiveRecallService::new`) and pass the config value `cfg.tool_transition_recency_half_life_days` (or the runtime config's field, whichever is in scope at that bootstrap). If the config isn't in scope there, pass the default `30.0` and leave a `// TODO(config)` — but prefer wiring the real value; report which you did.

- [ ] **Step 3: Remove the dead `record_co_usage`**

In `tool_memory.rs`, delete the `record_co_usage` method (zero prod callers) and any test that only tested it. Confirm: `grep -rn "record_co_usage" src-tauri/src` → only deletions, no remaining callers. (Leave `memory_adapter::edges` itself alone — it may have non-`co_used` users; only the tool-co-usage method goes.)

- [ ] **Step 4: Verify**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
Run: `cd src-tauri && cargo test --lib proactive 2>&1 | tail -5` → green (no new failures).
Run: `cd src-tauri && cargo clippy --lib 2>&1 | grep -E "tool_transitions|tool_memory|proactive_recall" | head` → no new warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/proactive/service.rs src-tauri/src/proactive/proactive_recall.rs src-tauri/src/proactive/tool_memory.rs
git commit -m "feat(memory): wire tool-transition aggregation tick + recall injection over transitions + drop dead record_co_usage (Slice E)"
```

---

## Task 6: Whole-slice verification + ship

**Files:** none (verification + ship only)

- [ ] **Step 1: Build + clippy clean**

`cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
`cd src-tauri && cargo clippy --lib 2>&1 | grep -E "tool_transitions|tool_memory|proactive_recall|service.rs|memubot" | grep -iE "warning|error" | head` → no NEW warnings from E's files.

- [ ] **Step 2: Targeted + broad dependent test run**

Run each separately (cargo test --lib takes one filter):
```
cargo test --lib memory_graph::tool_transitions
cargo test --lib proactive::tool_memory
cargo test --lib proactive
cargo test --lib memubot_config
```
All green.

- [ ] **Step 3: Grep gates**

```bash
# aggregation called from exactly one prod site (the tick) + tests
grep -rn "run_tool_transition_aggregation_blocking" src-tauri/src --include=*.rs | grep -v tool_transitions.rs
# dead paths gone:
grep -rn "record_co_usage\|list_all_tool_nodes\|ToolNodeStats" src-tauri/src --include=*.rs
# expect: empty (all removed)
```

- [ ] **Step 4: GitNexus reindex** — `npx gitnexus analyze` (from repo root).

- [ ] **Step 5: PR** — push branch, open PR with a `## Commits (bisectable)` table (T1 table+store, T2 aggregation, T3 suggest rewrite, T4 config, T5 wiring+cleanup). Note adjacent edits: config in both structs; dead-code removal (record_co_usage/list_all_tool_nodes/ToolNodeStats). Link spec.

- [ ] **Step 6: Merge + closeout** — rebase onto latest origin/main (other sessions may have merged); rebase-merge; sync local main; delete branch + worktree; reindex on main; update memory (`project-openhuman-deepening.md` → E SHIPPED + next recommend F; `MEMORY.md`).

---

## Self-Review

**Spec coverage:** §1 table → T1; §2 aggregation+watermark → T2; §3 suggest rewrite → T3; §4 injection → T5; §5 config → T4; §6 cleanup → T3 (Procedure path) + T5 (record_co_usage). Testing items 1-7 → T1/T2/T3/T4 tests + T6 broad run + grep gates. ✓

**Placeholder scan:** All SQL/signatures/structs concrete. The two conditional instructions (T3 Step 1 "match actual `make_*` helper names"; T5 Step 2 "pass real config or default 30.0 + report") are guards against unseen local detail with a concrete fallback, not TODOs. ✓

**Type consistency:** `TransitionRow{to_tool,count,success_count,last_seen_ms}`, `ToolTransitionOutcome{pairs_processed,new_watermark_ms}`, `upsert_transition`/`top_transitions_from`/`run_tool_transition_aggregation_blocking` names identical across def/test/job. `suggest_tool_chain` new signature `(space_id, _task, recency_half_life_days)` consistent in T3 def + T5 call. `ToolSuggestion` reused as-is. ✓

**Watermark correctness:** load ordered by `created_at` (contiguous prefix), watermark = max created_at of batch, re-sort by (session,turn_index) in memory for pairing → completeness preserved; boundary under-count documented + accepted. Re-run idempotency tested. ✓

**Deadlock:** aggregation takes `&Connection` (job locks `store.conn` once), runs raw SQL only; suggest_tool_chain locks `store.conn` itself (called from recall, not inside the job) — no nested lock. ✓
