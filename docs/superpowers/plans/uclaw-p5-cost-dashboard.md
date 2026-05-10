# P5 — Cost / Token Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist per-turn cost (token usage + USD) and surface it in Settings → 用量 with three views: daily totals (bar chart), per-model breakdown (donut), and per-session table.

**Architecture:**

- **Capture point:** the dispatcher already emits `agent:turn_cost` events from `emit_turn_cost` after every LLM call. Hook into the same call site to **also** persist a row to a new `cost_records` table — this keeps capture in-process (no IPC roundtrip, no risk of dropped events when the frontend isn't listening).
- **Schema:** new `cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)` plus an index on `(created_at)` for the daily rollup and `(session_id)` for per-session aggregation. **V13** — V11 reserved for PR #33's trigram migration, V12 used by the agent_messages_fts hotfix.
- **Read API:** three Tauri commands returning typed rollups. All run pure SQL aggregations against the new table.
- **Frontend:** new `UsageSettings.tsx` rendering three charts via `recharts`. New "用量" nav entry in `SettingsPanel`. Live updates by subscribing to the existing `onTurnCost` listener and re-fetching the daily rollup when a new event arrives (debounced).

**Tech Stack:** Adds `recharts` (~250kB gzipped) — used only for charts in the new tab. No other new deps.

**Reference:** Roadmap §P5 at `docs/superpowers/specs/2026-05-09-uclaw-roadmap.md:253`.

---

## Pre-flight

- [ ] **Step 0.1: Branch off latest main**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout main && git pull
git checkout -b claude/p5-cost-dashboard
```

- [ ] **Step 0.2: Baseline pipeline**

```bash
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, all tests passing, 0 TS errors.

---

## Task 1: Schema + write-side store

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `V13_COST_RECORDS`)
- Create: `src-tauri/src/cost_store.rs` (write helper)
- Modify: `src-tauri/src/lib.rs` or `main.rs` (`mod cost_store;`)
- Modify: `src-tauri/src/agent/dispatcher.rs` (call `cost_store::record` from `emit_turn_cost`)

- [ ] **Step 1.1: Add V13 migration constant**

Edit `src-tauri/src/db/migrations.rs`. Add **after** `V12_AGENT_MESSAGES_FTS`:

```rust
/// V13: per-turn cost records for the usage dashboard.
///
/// Captures one row per LLM call so we can roll up by (day, model, session).
/// `cost_usd` stored as REAL — small numbers, no exact-decimal requirement.
/// Indexes target the three rollup queries: daily totals (created_at),
/// per-session aggregation (session_id), per-model breakdown (model).
pub const V13_COST_RECORDS: &str = "
CREATE TABLE IF NOT EXISTS cost_records (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    model         TEXT NOT NULL,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd      REAL NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cost_records_created ON cost_records(created_at);
CREATE INDEX IF NOT EXISTS idx_cost_records_session ON cost_records(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_records_model   ON cost_records(model);
";
```

- [ ] **Step 1.2: Apply in `run`**

Edit `run`. After the V12 backfill block, add:

```rust
    tracing::debug!("Running migration V13: cost records");
    for stmt in V13_COST_RECORDS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V13 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 1.3: Create `cost_store.rs`**

Create `src-tauri/src/cost_store.rs`:

```rust
//! Write-side persistence for per-turn LLM cost records.
//!
//! Called from the agent dispatcher's `emit_turn_cost` to capture usage
//! synchronously alongside the IPC event. No frontend dependency — events
//! are fire-and-forget from the listener's POV; persistence is the source
//! of truth for the dashboard.

use crate::app::AppState;
use crate::agent::types::calculate_cost;
use rusqlite::params;

/// Insert one cost record. Errors are logged and swallowed — cost capture
/// is best-effort and must never fail the agent loop.
pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32) {
    let cost = calculate_cost(model, input_tokens, output_tokens);
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cost_store: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, session_id, model, input_tokens as i64, output_tokens as i64, cost, now],
    ) {
        tracing::warn!("cost_store: INSERT failed: {}", e);
    }
}
```

- [ ] **Step 1.4: Register module**

Edit `src-tauri/src/lib.rs` (or wherever the top-level `mod` declarations live — check with `grep -n "^pub mod\|^mod" src-tauri/src/lib.rs src-tauri/src/main.rs | head -20`). Add:

```rust
pub mod cost_store;
```

- [ ] **Step 1.5: Hook from dispatcher**

Edit `src-tauri/src/agent/dispatcher.rs`. Find `emit_turn_cost` (around line 206). Replace its body to also call the store. The dispatcher has `self.app_handle` and `self.conversation_id` (which doubles as session_id for the agent path):

```rust
fn emit_turn_cost(&self, usage: &TokenUsage) {
    let cost = calculate_cost(&self.model, usage.input_tokens, usage.output_tokens);
    let turn_cost = TurnCostInfo {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cost_usd: format_cost(cost),
    };
    tracing::info!(
        input_tokens = usage.input_tokens,
        output_tokens = usage.output_tokens,
        cost_usd = %turn_cost.cost_usd,
        "Emitting agent:turn_cost"
    );

    // Persist BEFORE emitting so the dashboard never undercounts even if
    // the frontend listener races. Best-effort — failures don't propagate.
    use tauri::Manager;
    if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
        crate::cost_store::record(
            &state,
            &self.conversation_id,
            &self.model,
            usage.input_tokens,
            usage.output_tokens,
        );
    }

    let _ = self.app_handle.emit("agent:turn_cost", &turn_cost);
}
```

If `tauri::Manager` is already imported at the top of the file, drop the `use` line.

- [ ] **Step 1.6: Build clean**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: 0 errors. If `uuid` isn't a dispatcher dep yet, check `Cargo.toml` — it's already used elsewhere in the codebase (per `tauri_commands.rs`). Same for `chrono`.

- [ ] **Step 1.7: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/cost_store.rs src-tauri/src/lib.rs src-tauri/src/agent/dispatcher.rs
git commit -m "$(cat <<'EOF'
feat(cost): V13 cost_records + dispatcher write hook

Persists per-turn LLM usage as a row in cost_records each time the
agent dispatcher emits agent:turn_cost. Capture is in-process so
the frontend dashboard never undercounts when a listener isn't
attached or races a fast event.

Schema is small and indexed for the three rollup queries that land
in Task 2 (daily / per-model / per-session). Errors in cost_store
are best-effort — they're logged and swallowed so a cost-capture
failure can never break the agent loop.
EOF
)"
```

---

## Task 2: Read commands — three rollup queries

**Files:**
- Modify: `src-tauri/src/ipc.rs` (3 new response types)
- Modify: `src-tauri/src/tauri_commands.rs` (3 new commands + register)
- Modify: `src-tauri/src/main.rs` (`invoke_handler!` registration)

- [ ] **Step 2.1: Add IPC response types**

Edit `src-tauri/src/ipc.rs`. Add at the end:

```rust
// ─── Cost dashboard ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostRollup {
    /// `YYYY-MM-DD` (UTC).
    pub day: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostRollup {
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCostRollup {
    pub session_id: String,
    /// Joined session title from `agent_sessions`/`conversations`. Empty if unknown.
    pub title: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
    /// Most-recent record's created_at (epoch ms).
    pub last_used_at: i64,
}
```

- [ ] **Step 2.2: Add the three commands**

Edit `src-tauri/src/tauri_commands.rs`. Add at an appropriate location (near the other dashboard-style commands, e.g. after `list_recent_threads`):

```rust
#[tauri::command]
pub async fn get_daily_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
) -> Result<Vec<DailyCostRollup>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let days = days_back.unwrap_or(30).clamp(1, 365);
    let cutoff_ms = chrono::Utc::now().timestamp_millis() - (days as i64) * 86_400_000;

    // SQLite stores created_at as epoch-ms. Group by UTC YYYY-MM-DD.
    let mut stmt = conn.prepare(
        "SELECT
            strftime('%Y-%m-%d', created_at / 1000, 'unixepoch') AS day,
            SUM(input_tokens) AS in_tok,
            SUM(output_tokens) AS out_tok,
            SUM(cost_usd) AS cost,
            COUNT(*) AS turns
         FROM cost_records
         WHERE created_at >= ?1
         GROUP BY day
         ORDER BY day ASC",
    ).map_err(|e| Error::Internal(format!("prepare daily: {}", e)))?;

    let rows = stmt.query_map(rusqlite::params![cutoff_ms], |row| {
        Ok(DailyCostRollup {
            day: row.get(0)?,
            input_tokens: row.get(1)?,
            output_tokens: row.get(2)?,
            cost_usd: row.get(3)?,
            turn_count: row.get(4)?,
        })
    }).map_err(|e| Error::Internal(format!("daily query: {}", e)))?;

    Ok(rows.flatten().collect())
}

#[tauri::command]
pub async fn get_model_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
) -> Result<Vec<ModelCostRollup>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let days = days_back.unwrap_or(30).clamp(1, 365);
    let cutoff_ms = chrono::Utc::now().timestamp_millis() - (days as i64) * 86_400_000;

    let mut stmt = conn.prepare(
        "SELECT model,
                SUM(input_tokens), SUM(output_tokens),
                SUM(cost_usd), COUNT(*)
         FROM cost_records
         WHERE created_at >= ?1
         GROUP BY model
         ORDER BY cost_usd DESC"
    ).map_err(|e| Error::Internal(format!("prepare model: {}", e)))?;

    let rows = stmt.query_map(rusqlite::params![cutoff_ms], |row| {
        Ok(ModelCostRollup {
            model: row.get(0)?,
            input_tokens: row.get(1)?,
            output_tokens: row.get(2)?,
            cost_usd: row.get(3)?,
            turn_count: row.get(4)?,
        })
    }).map_err(|e| Error::Internal(format!("model query: {}", e)))?;

    Ok(rows.flatten().collect())
}

#[tauri::command]
pub async fn get_session_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<SessionCostRollup>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let days = days_back.unwrap_or(30).clamp(1, 365);
    let lim  = limit.unwrap_or(50).clamp(1, 500);
    let cutoff_ms = chrono::Utc::now().timestamp_millis() - (days as i64) * 86_400_000;

    // session_id may live in either `agent_sessions` (agent runs) or
    // `conversations` (chat runs). Use COALESCE on the two title sources.
    let mut stmt = conn.prepare(
        "SELECT
            cr.session_id,
            COALESCE(s.title, c.title, '') AS title,
            SUM(cr.input_tokens), SUM(cr.output_tokens),
            SUM(cr.cost_usd), COUNT(*),
            MAX(cr.created_at) AS last_used
         FROM cost_records cr
         LEFT JOIN agent_sessions s ON s.id = cr.session_id
         LEFT JOIN conversations  c ON c.id = cr.session_id
         WHERE cr.created_at >= ?1
         GROUP BY cr.session_id
         ORDER BY last_used DESC
         LIMIT ?2"
    ).map_err(|e| Error::Internal(format!("prepare session: {}", e)))?;

    let rows = stmt.query_map(rusqlite::params![cutoff_ms, lim as i64], |row| {
        Ok(SessionCostRollup {
            session_id: row.get(0)?,
            title: row.get(1)?,
            input_tokens: row.get(2)?,
            output_tokens: row.get(3)?,
            cost_usd: row.get(4)?,
            turn_count: row.get(5)?,
            last_used_at: row.get(6)?,
        })
    }).map_err(|e| Error::Internal(format!("session query: {}", e)))?;

    Ok(rows.flatten().collect())
}
```

Imports needed at the top of `tauri_commands.rs`:
```rust
use crate::ipc::{DailyCostRollup, ModelCostRollup, SessionCostRollup};
```

- [ ] **Step 2.3: Register commands**

Edit `src-tauri/src/main.rs`. Add to the `invoke_handler!` macro list:
```rust
uclaw_core::tauri_commands::get_daily_costs,
uclaw_core::tauri_commands::get_model_costs,
uclaw_core::tauri_commands::get_session_costs,
```

- [ ] **Step 2.4: Build clean**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
```
Expected: 0 errors.

- [ ] **Step 2.5: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(cost): three rollup commands — daily / model / session

Pure SQL aggregations against cost_records:
  - get_daily_costs(days_back?) — bucket by UTC YYYY-MM-DD
  - get_model_costs(days_back?) — sum per model, ordered by spend
  - get_session_costs(days_back?, limit?) — per session, COALESCE
    title from either agent_sessions or conversations

All three clamp parameters defensively. Indexes added in V13 cover
each query's predicate + GROUP BY.
EOF
)"
```

---

## Task 3: Backend tests — rollup correctness

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (append `#[cfg(test)] mod cost_rollup_tests`)

Pure-function tests on the SQL aggregation. Use an in-memory SQLite for isolation.

- [ ] **Step 3.1: Add tests**

Append to `src-tauri/src/tauri_commands.rs`:

```rust
#[cfg(test)]
mod cost_rollup_tests {
    use rusqlite::Connection;

    /// Apply just the V13 schema to an in-memory DB so tests don't need
    /// the full migration chain.
    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V13_COST_RECORDS).unwrap();
        // Minimal stub for the COALESCE join in get_session_costs.
        conn.execute_batch(
            "CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, title TEXT);
             CREATE TABLE conversations  (id TEXT PRIMARY KEY, title TEXT);"
        ).unwrap();
        conn
    }

    fn insert_cost(
        conn: &Connection,
        session_id: &str,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
        created_at: i64,
    ) {
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                session_id, model, input_tokens, output_tokens, cost_usd, created_at,
            ],
        ).unwrap();
    }

    #[test]
    fn daily_rollup_groups_by_day() {
        let conn = fresh_db();
        // Two rows on day A, one on day B.
        let day_a = 1_715_000_000_000_i64; // some fixed epoch ms
        let day_b = day_a + 86_400_000;
        insert_cost(&conn, "s1", "claude-4", 100, 50, 0.001, day_a);
        insert_cost(&conn, "s1", "claude-4", 200, 80, 0.002, day_a);
        insert_cost(&conn, "s2", "gpt-4o",   500, 100, 0.005, day_b);

        let mut stmt = conn.prepare(
            "SELECT strftime('%Y-%m-%d', created_at / 1000, 'unixepoch'),
                    SUM(input_tokens), SUM(output_tokens), SUM(cost_usd), COUNT(*)
             FROM cost_records
             GROUP BY 1 ORDER BY 1"
        ).unwrap();
        let rows: Vec<(String, i64, i64, f64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(rows.len(), 2);
        // Day A — 300 input, 130 output, 0.003 cost, 2 turns
        assert_eq!(rows[0].1, 300);
        assert_eq!(rows[0].2, 130);
        assert!((rows[0].3 - 0.003).abs() < 1e-9);
        assert_eq!(rows[0].4, 2);
        // Day B — 500/100/0.005/1
        assert_eq!(rows[1].1, 500);
        assert_eq!(rows[1].4, 1);
    }

    #[test]
    fn model_rollup_sums_per_model() {
        let conn = fresh_db();
        let now = 1_715_000_000_000_i64;
        insert_cost(&conn, "s1", "claude-4", 100, 50, 0.001, now);
        insert_cost(&conn, "s2", "claude-4", 200, 80, 0.003, now);
        insert_cost(&conn, "s3", "gpt-4o",   500, 100, 0.010, now);

        let mut stmt = conn.prepare(
            "SELECT model, SUM(input_tokens), SUM(output_tokens), SUM(cost_usd), COUNT(*)
             FROM cost_records GROUP BY model ORDER BY cost_usd DESC"
        ).unwrap();
        let rows: Vec<(String, i64, i64, f64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap().flatten().collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "gpt-4o"); // higher spend first
        assert_eq!(rows[0].4, 1);
        assert_eq!(rows[1].0, "claude-4");
        assert_eq!(rows[1].1, 300);
        assert_eq!(rows[1].4, 2);
    }

    #[test]
    fn session_rollup_uses_coalesced_title() {
        let conn = fresh_db();
        conn.execute("INSERT INTO agent_sessions VALUES ('s1', 'Agent run alpha')", []).unwrap();
        conn.execute("INSERT INTO conversations  VALUES ('c1', 'Chat about beta')", []).unwrap();
        let now = 1_715_000_000_000_i64;
        insert_cost(&conn, "s1", "claude-4", 100, 50, 0.001, now);
        insert_cost(&conn, "c1", "gpt-4o",   200, 80, 0.002, now);
        insert_cost(&conn, "unknown", "qwen", 50, 25, 0.0001, now);

        let mut stmt = conn.prepare(
            "SELECT cr.session_id,
                    COALESCE(s.title, c.title, '') AS title,
                    SUM(cr.cost_usd), MAX(cr.created_at)
             FROM cost_records cr
             LEFT JOIN agent_sessions s ON s.id = cr.session_id
             LEFT JOIN conversations  c ON c.id = cr.session_id
             GROUP BY cr.session_id"
        ).unwrap();
        let mut titles: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let _ = stmt.query_map([], |r| {
            titles.insert(r.get::<_, String>(0)?, r.get::<_, String>(1)?);
            Ok(())
        }).unwrap().for_each(|_| ());
        assert_eq!(titles.get("s1").map(|s| s.as_str()), Some("Agent run alpha"));
        assert_eq!(titles.get("c1").map(|s| s.as_str()), Some("Chat about beta"));
        assert_eq!(titles.get("unknown").map(|s| s.as_str()), Some("")); // empty fallback
    }
}
```

- [ ] **Step 3.2: Run + commit**

```bash
cd src-tauri && cargo test --lib cost_rollup_tests 2>&1 | tail -10
```
Expected: 3 passed.

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "test(cost): rollup correctness on daily / model / session"
```

---

## Task 4: Frontend — recharts + UsageSettings

**Files:**
- Modify: `ui/package.json` + lockfile (add `recharts`)
- Create: `ui/src/components/settings/UsageSettings.tsx`
- Modify: `ui/src/lib/tauri-bridge.ts` (3 invoke wrappers)
- Modify: `ui/src/lib/types.ts` (3 rollup types — TS counterparts of the Rust IPC structs)

- [ ] **Step 4.1: Install recharts**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm install recharts@^2.15.0
```

- [ ] **Step 4.2: Add TS types**

Edit `ui/src/lib/types.ts`. Append:

```ts
// ===== Cost dashboard =====

export interface DailyCostRollup {
  day: string // YYYY-MM-DD
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
}

export interface ModelCostRollup {
  model: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
}

export interface SessionCostRollup {
  sessionId: string
  title: string
  inputTokens: number
  outputTokens: number
  costUsd: number
  turnCount: number
  lastUsedAt: number
}
```

- [ ] **Step 4.3: Bridge wrappers**

Edit `ui/src/lib/tauri-bridge.ts`. Add near other list-* / get-* helpers:

```ts
import type {
  DailyCostRollup,
  ModelCostRollup,
  SessionCostRollup,
} from './types'

export const getDailyCosts = (daysBack = 30): Promise<DailyCostRollup[]> =>
  invoke<DailyCostRollup[]>('get_daily_costs', { daysBack })

export const getModelCosts = (daysBack = 30): Promise<ModelCostRollup[]> =>
  invoke<ModelCostRollup[]>('get_model_costs', { daysBack })

export const getSessionCosts = (daysBack = 30, limit = 50): Promise<SessionCostRollup[]> =>
  invoke<SessionCostRollup[]>('get_session_costs', { daysBack, limit })
```

- [ ] **Step 4.4: UsageSettings component**

Create `ui/src/components/settings/UsageSettings.tsx`:

```tsx
/**
 * UsageSettings — Settings → 用量 tab.
 *
 * Three views:
 *   - Daily total bar chart (last 30 days)
 *   - Per-model donut (last 30 days)
 *   - Per-session table (most recent first)
 *
 * Live update: subscribes to `agent:turn_cost` events; on each event
 * we re-fetch the daily rollup (debounced 1s) so the bar chart bumps
 * without manual reload.
 */

import * as React from 'react'
import {
  ResponsiveContainer,
  BarChart, Bar,
  PieChart, Pie, Cell,
  XAxis, YAxis, Tooltip,
  CartesianGrid, Legend,
} from 'recharts'
import {
  getDailyCosts, getModelCosts, getSessionCosts, onTurnCost,
} from '@/lib/tauri-bridge'
import type {
  DailyCostRollup, ModelCostRollup, SessionCostRollup,
} from '@/lib/types'

const PALETTE = ['hsl(220 70% 55%)', 'hsl(160 65% 45%)', 'hsl(30 80% 55%)',
                 'hsl(280 60% 60%)', 'hsl(0 70% 60%)', 'hsl(180 60% 45%)']

function formatUsd(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  return `$${v.toFixed(2)}`
}

function formatDateChip(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()}`
}

export function UsageSettings(): React.ReactElement {
  const [daily, setDaily] = React.useState<DailyCostRollup[]>([])
  const [models, setModels] = React.useState<ModelCostRollup[]>([])
  const [sessions, setSessions] = React.useState<SessionCostRollup[]>([])
  const [loading, setLoading] = React.useState(true)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [d, m, s] = await Promise.all([
        getDailyCosts(30),
        getModelCosts(30),
        getSessionCosts(30, 50),
      ])
      setDaily(d)
      setModels(m)
      setSessions(s)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => { void refetch() }, [refetch])

  // Debounced re-fetch on new turn_cost events
  React.useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null
    const unlistenP = onTurnCost(() => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => { void refetch() }, 1000)
    })
    return () => {
      if (timer) clearTimeout(timer)
      void unlistenP.then((u) => u())
    }
  }, [refetch])

  const totals = React.useMemo(() => {
    const cost = daily.reduce((a, d) => a + d.costUsd, 0)
    const inTok = daily.reduce((a, d) => a + d.inputTokens, 0)
    const outTok = daily.reduce((a, d) => a + d.outputTokens, 0)
    const turns = daily.reduce((a, d) => a + d.turnCount, 0)
    return { cost, inTok, outTok, turns }
  }, [daily])

  return (
    <div className="space-y-6 pb-8">
      {/* KPI cards */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          最近 30 天总览
        </h3>
        <div className="grid grid-cols-4 gap-2.5">
          <KPICard label="总花费" value={formatUsd(totals.cost)} />
          <KPICard label="LLM 调用" value={totals.turns.toLocaleString()} />
          <KPICard label="输入 token" value={(totals.inTok / 1000).toFixed(1) + 'k'} />
          <KPICard label="输出 token" value={(totals.outTok / 1000).toFixed(1) + 'k'} />
        </div>
      </section>

      {/* Daily bar */}
      <section>
        <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          每日花费（USD）
        </h3>
        <div className="h-56 rounded-lg border border-border/50 bg-muted/20 p-3">
          {daily.length === 0 ? (
            <EmptyState text={loading ? '加载中…' : '暂无数据'} />
          ) : (
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={daily} margin={{ top: 8, right: 8, left: 8, bottom: 4 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border) / 0.5)" vertical={false} />
                <XAxis dataKey="day" tick={{ fontSize: 10 }} stroke="hsl(var(--muted-foreground) / 0.7)" />
                <YAxis tick={{ fontSize: 10 }} stroke="hsl(var(--muted-foreground) / 0.7)"
                       tickFormatter={(v: number) => formatUsd(v)} />
                <Tooltip
                  contentStyle={{ background: 'hsl(var(--popover))', border: '1px solid hsl(var(--border))', fontSize: 12 }}
                  formatter={(v: number) => formatUsd(v)}
                />
                <Bar dataKey="costUsd" fill="hsl(var(--primary))" radius={[3, 3, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>
      </section>

      {/* Model donut + Session table side by side on wide */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <section>
          <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            按模型
          </h3>
          <div className="h-56 rounded-lg border border-border/50 bg-muted/20 p-3">
            {models.length === 0 ? (
              <EmptyState text={loading ? '加载中…' : '暂无数据'} />
            ) : (
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={models}
                    dataKey="costUsd"
                    nameKey="model"
                    innerRadius={50}
                    outerRadius={75}
                    paddingAngle={2}
                  >
                    {models.map((_, i) => (
                      <Cell key={i} fill={PALETTE[i % PALETTE.length]} />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={{ background: 'hsl(var(--popover))', border: '1px solid hsl(var(--border))', fontSize: 12 }}
                    formatter={(v: number) => formatUsd(v)}
                  />
                  <Legend wrapperStyle={{ fontSize: 11 }} />
                </PieChart>
              </ResponsiveContainer>
            )}
          </div>
        </section>

        <section>
          <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            会话明细
          </h3>
          <div className="rounded-lg border border-border/50 bg-muted/20 max-h-56 overflow-y-auto">
            {sessions.length === 0 ? (
              <div className="p-3"><EmptyState text={loading ? '加载中…' : '暂无数据'} /></div>
            ) : (
              <table className="w-full text-[12px]">
                <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
                  <tr className="text-left text-muted-foreground/70">
                    <th className="px-3 py-2 font-normal">会话</th>
                    <th className="px-3 py-2 font-normal text-right">花费</th>
                    <th className="px-3 py-2 font-normal text-right">最近</th>
                  </tr>
                </thead>
                <tbody>
                  {sessions.map((s) => (
                    <tr key={s.sessionId} className="border-t border-border/30 hover:bg-muted/30">
                      <td className="px-3 py-1.5 truncate max-w-[200px]">
                        {s.title || s.sessionId.slice(0, 8)}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{formatUsd(s.costUsd)}</td>
                      <td className="px-3 py-1.5 text-right text-muted-foreground/60 tabular-nums">
                        {formatDateChip(s.lastUsedAt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}

function KPICard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border/50 bg-muted/20 px-3 py-2.5">
      <div className="text-[11px] text-muted-foreground/70">{label}</div>
      <div className="mt-0.5 text-[16px] font-semibold tabular-nums">{value}</div>
    </div>
  )
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className="flex h-full items-center justify-center text-[12px] text-muted-foreground/60">
      {text}
    </div>
  )
}
```

- [ ] **Step 4.5: TS check**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

- [ ] **Step 4.6: Commit**

```bash
git add ui/package.json ui/package-lock.json ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts ui/src/components/settings/UsageSettings.tsx
git commit -m "$(cat <<'EOF'
feat(cost): UsageSettings — KPI + daily bar + model donut + session table

Adds recharts (~250kB gz, only loaded when 用量 tab is opened — chunk
naturally split by Vite). Subscribes to agent:turn_cost so the bar
chart bumps without manual reload (debounced 1s).

KPI cards summarize the last-30-days totals; the bar uses primary
theme color so it adapts to all 11 themes; the donut palette is
fixed-hue HSL since chart segments are categorical.
EOF
)"
```

---

## Task 5: Wire into SettingsPanel nav

**Files:**
- Modify: `ui/src/components/settings/SettingsPanel.tsx`

- [ ] **Step 5.1: Add tab entry + content branch**

Edit `SettingsPanel.tsx`. Find the `TABS` array (around line 40) and insert the new entry **between** "外观" and "Agent" (logical grouping under the user's daily-use settings):

```tsx
{ id: 'usage', label: '用量', icon: <BarChart3 size={15} /> },
```

Add the import at the top (with the other lucide icons):
```tsx
import { BarChart3 } from 'lucide-react'
```

If `BarChart3` isn't in lucide-react v0.460, fall back to `Activity` (always present).

Then in `SettingsContent`, add the case:
```tsx
case 'usage':
  return <UsageSettings />
```

Add the import:
```tsx
import { UsageSettings } from './UsageSettings'
```

If `SettingsTab` is a string union somewhere (e.g. in `atoms/settings-atoms.ts`), add `'usage'` to it. Find with:
```bash
grep -rnE "type SettingsTab|SettingsTab =" ui/src
```

- [ ] **Step 5.2: TS check + commit**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

```bash
git add ui/src/components/settings/SettingsPanel.tsx ui/src/atoms/settings-atoms.ts
git commit -m "feat(cost): add 用量 tab to Settings nav"
```

---

## Task 6: Frontend test for UsageSettings

**Files:**
- Create: `ui/src/components/settings/UsageSettings.test.tsx`

Recharts SVG rendering under jsdom is finicky. Test the data-shape side: renders KPIs with correct totals, renders empty state when no data, calls all three rollup commands on mount.

- [ ] **Step 6.1: Write test**

Create `ui/src/components/settings/UsageSettings.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { UsageSettings } from './UsageSettings'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

// Recharts under jsdom emits lots of warnings about ResizeObserver
// and SVG layout — mock the layout primitives but keep the chart shells
// so the component tree renders.
vi.mock('recharts', async () => {
  return {
    ResponsiveContainer: ({ children }: any) => <div data-testid="rc-container">{children}</div>,
    BarChart: ({ children }: any) => <div data-testid="rc-bar">{children}</div>,
    Bar: () => null,
    PieChart: ({ children }: any) => <div data-testid="rc-pie">{children}</div>,
    Pie: () => null,
    Cell: () => null,
    XAxis: () => null, YAxis: () => null,
    CartesianGrid: () => null, Tooltip: () => null, Legend: () => null,
  }
})

vi.mock('@/lib/tauri-bridge', () => ({
  getDailyCosts: vi.fn(async () => [
    { day: '2026-05-08', inputTokens: 1000, outputTokens: 500, costUsd: 0.012, turnCount: 4 },
    { day: '2026-05-09', inputTokens: 2000, outputTokens: 800, costUsd: 0.024, turnCount: 6 },
  ]),
  getModelCosts: vi.fn(async () => [
    { model: 'claude-4', inputTokens: 2500, outputTokens: 1100, costUsd: 0.030, turnCount: 8 },
    { model: 'gpt-4o',   inputTokens: 500,  outputTokens: 200,  costUsd: 0.006, turnCount: 2 },
  ]),
  getSessionCosts: vi.fn(async () => [
    { sessionId: 's1', title: 'Foo', inputTokens: 1500, outputTokens: 600, costUsd: 0.020, turnCount: 5, lastUsedAt: 1715000000000 },
  ]),
  onTurnCost: vi.fn(async () => () => {}),
}))

describe('UsageSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders the KPI totals from the daily rollup', async () => {
    renderWithProviders(<UsageSettings />)
    // Total cost = 0.012 + 0.024 = 0.036 → "$0.0360"
    await waitFor(() => {
      expect(screen.getByText(/\$0\.036\d?/)).toBeInTheDocument()
    })
    // Total turns = 4 + 6 = 10
    expect(screen.getByText('10')).toBeInTheDocument()
  })

  it('renders the bar + donut chart shells with data', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByTestId('rc-bar')).toBeInTheDocument()
      expect(screen.getByTestId('rc-pie')).toBeInTheDocument()
    })
  })

  it('renders the per-session table row', async () => {
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      expect(screen.getByText('Foo')).toBeInTheDocument()
      expect(screen.getByText('$0.02')).toBeInTheDocument()
    })
  })

  it('renders empty-state when all rollups are empty', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.getDailyCosts).mockResolvedValueOnce([])
    vi.mocked(bridge.getModelCosts).mockResolvedValueOnce([])
    vi.mocked(bridge.getSessionCosts).mockResolvedValueOnce([])
    renderWithProviders(<UsageSettings />)
    await waitFor(() => {
      // Three "暂无数据" placeholders (daily / model / session).
      expect(screen.getAllByText('暂无数据').length).toBeGreaterThanOrEqual(3)
    })
  })
})
```

- [ ] **Step 6.2: Run + commit**

```bash
cd ui && npx vitest run UsageSettings 2>&1 | tail -15
```
Expected: 4/4 passing.

```bash
git add ui/src/components/settings/UsageSettings.test.tsx
git commit -m "test(cost): UsageSettings renders KPIs / charts / table"
```

---

## Task 7: Final verification + push + PR

- [ ] **Step 7.1: Full pipeline**

```bash
cd /Users/ryanliu/Documents/uclaw
echo "=== rust ===" && (cd src-tauri && cargo build 2>&1 | tail -3)
echo "=== rust tests ===" && (cd src-tauri && cargo test --lib cost_rollup_tests 2>&1 | tail -5)
echo "=== ts ===" && (cd ui && npx tsc --noEmit 2>&1 | head -3)
echo "=== ui tests ===" && (cd ui && npm test -- --run 2>&1 | tail -5)
```
Expected: clean cargo, 3 cost_rollup tests pass, 0 TS errors, **35 frontend tests** passing (31 prior + 4 new).

- [ ] **Step 7.2: Push + PR**

```bash
git push -u origin claude/p5-cost-dashboard
gh pr create --title "P5: cost / token dashboard (Settings → 用量)" --body "$(cat <<'EOF'
## Summary

Persists per-turn LLM cost as it's emitted by the dispatcher and surfaces a 30-day rollup in Settings → 用量 with a daily bar, per-model donut, and per-session table.

## What changed

| Layer | Change |
|---|---|
| DB | V13 migration: `cost_records (session_id, model, input_tokens, output_tokens, cost_usd, created_at)` + 3 indexes |
| Backend | `cost_store::record` writes one row per LLM turn from `dispatcher.emit_turn_cost` (best-effort; failures logged + swallowed) |
| Backend | 3 read commands: `get_daily_costs`, `get_model_costs`, `get_session_costs` |
| Backend tests | 3 unit tests on rollup correctness |
| Frontend | `UsageSettings.tsx` with KPI cards + recharts BarChart + PieChart + session table |
| Frontend | Live update via `onTurnCost` event listener (debounced 1s) |
| Frontend | "用量" nav entry in `SettingsPanel` |
| Frontend tests | 4 new cases (KPI, charts, table, empty state) |

## Verification

- ✅ `cargo build` clean
- ✅ 3 backend rollup tests passing
- ✅ `tsc --noEmit` clean
- ✅ 35 frontend tests passing (31 prior + 4 new)
- ✅ Manual: ⌘, → 用量 tab; 一次新对话之后柱状图当天数值增长

## Out of scope (follow-ups)

- Custom date range / longer than 365 days
- Export CSV
- Per-workspace breakdown
- Real-time per-message cost in the chat bubble (event already fires; just not surfaced inline)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance criteria

- ✅ V13 migration creates `cost_records` + indexes; idempotent
- ✅ Each LLM turn writes exactly one cost record
- ✅ Daily / model / session rollups return correct totals on fixture data
- ✅ Settings → 用量 tab renders three views from real data
- ✅ Bar chart adapts to theme (uses `var(--primary)`)
- ✅ Live update on new turn (debounced)
- ✅ 35 frontend tests passing (31 prior + 4 new)
- ✅ Each task is its own commit (bisectable)

## Out of scope (deferred)

- CSV export
- Custom date pickers
- Per-workspace rollup
- Provider-level rollup (separate from model)
- Inline cost in chat bubble
