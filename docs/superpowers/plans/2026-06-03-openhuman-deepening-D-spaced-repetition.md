# openhuman Slice D — Spaced Repetition Scheduling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Schedule the dormant SM-2 ladder (`memory_graph/spaced_repetition.rs`) so important memories are periodically re-consolidated — importance is the grader (zero LLM), the recall layer is the reinforcement target. Closes the importance→spaced-repetition loop.

**Architecture:** A blocking orchestrator `run_spaced_repetition_blocking(&Connection, …)` runs inside the proactive `%360` tick (right after Slice C's importance recompute). It (1) auto-enrolls high-importance nodes, (2) reviews due nodes using Slice C's `memory_importance_scores` as the pass/drop grader, and returns the passed nodes so the tick's async half re-projects them into the `graph_facts` recall surface (reusing Slice A's `project_fact` + Slice B's `reinforce_recalled`). No migration (V45 table exists). No LLM.

**Tech Stack:** Rust, rusqlite, tokio (`spawn_blocking`), Tauri (proactive service). Config via `memubot_config.rs`.

**Spec:** `docs/superpowers/specs/2026-06-03-openhuman-deepening-D-spaced-repetition-design.md`

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src-tauri/src/memory_graph/spaced_repetition.rs` | SM-2 state machine (exists) | ADD: `SR_KINDS` const, `select_enrollable`, `SpacedRepetitionReinforce`, `SpacedRepetitionOutcome`, `run_spaced_repetition_blocking` + tests |
| `src-tauri/src/memubot_config.rs` | Runtime config knobs | ADD: 3 `spaced_repetition_*` fields + defaults + Default-impl values + test |
| `src-tauri/src/proactive/service.rs` | Background tick scheduler | ADD: `%360` SR job after the importance-decay block |

No new migration. No new Tauri command. No changes to `main.rs`.

**Key pinned facts (from recon — do not re-derive):**
- Kind strings: `"reference"`, `"episode"`, `"user_profile"` (`memory_graph/models.rs:44-57`, `MemoryNodeKind::as_str`).
- Current version text = `memory_versions WHERE node_id=? AND status='active' ORDER BY created_at DESC LIMIT 1` → `content` (canonical, used by `store.rs:677`, `importance_decay.rs:373`, `recall_projection` roundtrip).
- `memory_nodes.archived_at` exists (V57, nullable INTEGER ms); `NULL` = not archived.
- `memory_importance_scores.importance` is REAL 0..1 (V44).
- **Deadlock guard:** the orchestrator holds `&Connection` (the caller already did `store.conn.lock()`); it MUST run raw SQL, NEVER call `store.get_active_version`/`store.get_node` (they re-lock the same non-reentrant `std::sync::Mutex`).
- `enroll_node(conn, node_id, now_ms)`, `record_pass(conn, node_id, now_ms)`, `set_enabled(conn, node_id, bool)`, `due_now(conn, now_ms, limit)` — all `pub fn(&Connection, …) -> rusqlite::Result<…>`. `set_enabled` calls `enforce_freeze` (no-op unless frozen). `record_pass` calls `get_state` internally (errors if node not enrolled — safe, since `due_now` only returns enrolled rows).
- Module imports already present: `use rusqlite::{params, Connection, OptionalExtension};`
- `project_fact(adapter: &Arc<BucketSealAdapter>, node_id: &str, text: &str)` is `async`, returns `()` (logs on err). `reinforce_recalled(&self, summary_ids: &[String], now_ms: i64) -> anyhow::Result<()>` is `async`.
- `RECALL_PROJECTION_NAMESPACE = "graph_facts"` at `memory_adapter::recall_projection`.
- Test fixtures: `db::migrations::run(&conn)` builds the full schema (V45 included). Seed helper shape: `INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json) VALUES (?, 'default', ?, 'test-title', ?)`; versions: `INSERT INTO memory_versions (id, node_id, status, content) VALUES (?, ?, 'active', ?)`; importance: `INSERT INTO memory_importance_scores (...) VALUES (...)` (see Task 1 helper for the full column list).

---

## Task 1: `SR_KINDS` const + `select_enrollable` query

**Files:**
- Modify: `src-tauri/src/memory_graph/spaced_repetition.rs`

- [ ] **Step 1: Add the `SR_KINDS` constant**

Near the existing `INTERVAL_LADDER_DAYS` / `ENROLLMENT_IMPORTANCE_THRESHOLD` consts (top of file, ~line 31-37), add:

```rust
/// openhuman-D — the memory-node kinds eligible for spaced-repetition
/// enrollment. Matches Slice C's recall-projectable high-value kinds
/// (`reference`/`episode`/`user_profile`); these are the reflection facts
/// worth periodically re-consolidating. Strings match `MemoryNodeKind::as_str`.
pub const SR_KINDS: &[&str] = &["reference", "episode", "user_profile"];
```

- [ ] **Step 2: Write the failing test for `select_enrollable`**

Add to the existing `#[cfg(test)] mod tests` block. First add these shared seed helpers if not already present in the test module (place at the top of the test module):

```rust
fn d_conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::migrations::run(&conn).unwrap();
    conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
    conn
}

fn d_seed_node(conn: &Connection, id: &str, kind: &str) {
    conn.execute(
        "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json)
         VALUES (?1, 'default', ?2, 'test-title', NULL)",
        params![id, kind],
    )
    .unwrap();
}

fn d_seed_version(conn: &Connection, node_id: &str, content: &str) {
    conn.execute(
        "INSERT INTO memory_versions (id, node_id, status, content)
         VALUES (?1, ?2, 'active', ?3)",
        params![format!("ver-{node_id}"), node_id, content],
    )
    .unwrap();
}

fn d_seed_importance(conn: &Connection, node_id: &str, importance: f64) {
    conn.execute(
        "INSERT INTO memory_importance_scores
            (node_id, base_value, citation_factor, edge_factor, recency_factor,
             status_bonus, penalty, importance, decay_half_life_days, last_computed_at)
         VALUES (?1, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, ?2, 30.0, 0)",
        params![node_id, importance],
    )
    .unwrap();
}

fn d_set_archived(conn: &Connection, node_id: &str, archived_at_ms: i64) {
    conn.execute(
        "UPDATE memory_nodes SET archived_at = ?2 WHERE id = ?1",
        params![node_id, archived_at_ms],
    )
    .unwrap();
}
```

Then the test:

```rust
#[test]
fn select_enrollable_filters_correctly() {
    let conn = d_conn();
    // eligible: high-importance, right kind, not archived, not enrolled
    d_seed_node(&conn, "n-ok", "reference");
    d_seed_importance(&conn, "n-ok", 0.8);
    // excluded: below threshold
    d_seed_node(&conn, "n-low", "reference");
    d_seed_importance(&conn, "n-low", 0.4);
    // excluded: wrong kind
    d_seed_node(&conn, "n-kind", "boot");
    d_seed_importance(&conn, "n-kind", 0.9);
    // excluded: archived
    d_seed_node(&conn, "n-arch", "episode");
    d_seed_importance(&conn, "n-arch", 0.9);
    d_set_archived(&conn, "n-arch", 123);
    // excluded: already enrolled
    d_seed_node(&conn, "n-enr", "user_profile");
    d_seed_importance(&conn, "n-enr", 0.9);
    enroll_node(&conn, "n-enr", 1_000).unwrap();
    // excluded: no importance score row
    d_seed_node(&conn, "n-noscore", "reference");

    let got = select_enrollable(&conn, SR_KINDS, 0.6, 10).unwrap();
    assert_eq!(got, vec!["n-ok".to_string()]);
}

#[test]
fn select_enrollable_orders_by_importance_desc_and_limits() {
    let conn = d_conn();
    d_seed_node(&conn, "a", "reference");
    d_seed_importance(&conn, "a", 0.7);
    d_seed_node(&conn, "b", "reference");
    d_seed_importance(&conn, "b", 0.9);
    d_seed_node(&conn, "c", "reference");
    d_seed_importance(&conn, "c", 0.8);

    let got = select_enrollable(&conn, SR_KINDS, 0.6, 2).unwrap();
    assert_eq!(got, vec!["b".to_string(), "c".to_string()]);
}
```

- [ ] **Step 3: Run the tests — verify they fail**

Run: `cd src-tauri && cargo test --lib memory_graph::spaced_repetition::tests::select_enrollable 2>&1 | tail -20`
Expected: FAIL — `cannot find function select_enrollable`.

- [ ] **Step 4: Implement `select_enrollable`**

Add (place after `due_now`, before the test module). Note the `kinds` slice is interpolated into the SQL via generated `?` placeholders (kind strings are compile-time constants, not user input — but use bound params anyway for hygiene):

```rust
/// openhuman-D — select nodes eligible for spaced-repetition enrollment:
/// importance >= `threshold`, kind in `kinds`, not archived, and not already
/// enrolled. Highest-importance first. Read-only.
pub fn select_enrollable(
    conn: &Connection,
    kinds: &[&str],
    threshold: f64,
    limit: usize,
) -> rusqlite::Result<Vec<String>> {
    if limit == 0 || kinds.is_empty() {
        return Ok(vec![]);
    }
    let placeholders = std::iter::repeat("?")
        .take(kinds.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT n.id
         FROM memory_nodes n
         JOIN memory_importance_scores s ON s.node_id = n.id
         LEFT JOIN spaced_repetition_state sr ON sr.node_id = n.id
         WHERE n.kind IN ({placeholders})
           AND n.archived_at IS NULL
           AND s.importance >= ?
           AND sr.node_id IS NULL
         ORDER BY s.importance DESC
         LIMIT ?"
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(kinds.len() + 2);
    for k in kinds {
        params.push(Box::new(k.to_string()));
    }
    params.push(Box::new(threshold));
    params.push(Box::new(limit as i64));
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<String> = stmt
        .query_map(params_refs.as_slice(), |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}
```

- [ ] **Step 5: Run the tests — verify they pass**

Run: `cd src-tauri && cargo test --lib memory_graph::spaced_repetition::tests::select_enrollable 2>&1 | tail -20`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_graph/spaced_repetition.rs
git commit -m "feat(memory): SR_KINDS + select_enrollable (importance>=threshold, unenrolled, non-archived) for Slice D"
```

---

## Task 2: `run_spaced_repetition_blocking` orchestrator

**Files:**
- Modify: `src-tauri/src/memory_graph/spaced_repetition.rs`

- [ ] **Step 1: Add the output structs**

Place near the top of the file (after `SpacedRepetitionState`'s `impl`):

```rust
/// openhuman-D — a node that PASSED review this tick and should be
/// re-projected into the recall surface by the async half of the tick.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacedRepetitionReinforce {
    pub node_id: String,
    pub text: String,
}

/// openhuman-D — result of one spaced-repetition tick. `reinforce` carries
/// the passed nodes (with current text) for the async re-projection half.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpacedRepetitionOutcome {
    pub enrolled: usize,
    pub passed: usize,
    pub dropped: usize,
    pub reinforce: Vec<SpacedRepetitionReinforce>,
}
```

- [ ] **Step 2: Write the failing tests**

Add to the test module:

```rust
#[test]
fn run_sr_enrolls_high_importance_nodes() {
    let conn = d_conn();
    d_seed_node(&conn, "fresh", "reference");
    d_seed_version(&conn, "fresh", "important fact");
    d_seed_importance(&conn, "fresh", 0.9);

    let out = run_spaced_repetition_blocking(&conn, SR_KINDS, 0.6, 50, 10_000);
    assert_eq!(out.enrolled, 1);
    // newly enrolled → next_review_at = now + 1 day → NOT due this tick → not passed
    assert_eq!(out.passed, 0);
    let state = get_state(&conn, "fresh").unwrap();
    assert_eq!(state.interval_idx, 0);
    assert!(state.enabled);
}

#[test]
fn run_sr_passes_due_high_importance_and_collects_text() {
    let conn = d_conn();
    d_seed_node(&conn, "due", "reference");
    d_seed_version(&conn, "due", "still valuable");
    d_seed_importance(&conn, "due", 0.9);
    // enroll in the past so it's due now
    enroll_node(&conn, "due", 0).unwrap();

    let out = run_spaced_repetition_blocking(&conn, SR_KINDS, 0.6, 50, 5 * 86_400_000);
    assert_eq!(out.passed, 1);
    assert_eq!(out.dropped, 0);
    assert_eq!(
        out.reinforce,
        vec![SpacedRepetitionReinforce {
            node_id: "due".to_string(),
            text: "still valuable".to_string()
        }]
    );
    // ladder advanced 0 -> 1
    let state = get_state(&conn, "due").unwrap();
    assert_eq!(state.interval_idx, 1);
    assert_eq!(state.reviews_passed, 1);
}

#[test]
fn run_sr_drops_due_low_importance() {
    let conn = d_conn();
    d_seed_node(&conn, "faded", "reference");
    d_seed_version(&conn, "faded", "no longer useful");
    d_seed_importance(&conn, "faded", 0.2);
    enroll_node(&conn, "faded", 0).unwrap();

    let out = run_spaced_repetition_blocking(&conn, SR_KINDS, 0.6, 50, 5 * 86_400_000);
    assert_eq!(out.passed, 0);
    assert_eq!(out.dropped, 1);
    assert!(out.reinforce.is_empty());
    let state = get_state(&conn, "faded").unwrap();
    assert!(!state.enabled); // un-enrolled
}

#[test]
fn run_sr_drops_due_archived_node() {
    let conn = d_conn();
    d_seed_node(&conn, "gone", "episode");
    d_seed_version(&conn, "gone", "archived content");
    d_seed_importance(&conn, "gone", 0.9); // high importance but archived
    enroll_node(&conn, "gone", 0).unwrap();
    d_set_archived(&conn, "gone", 1);

    let out = run_spaced_repetition_blocking(&conn, SR_KINDS, 0.6, 50, 5 * 86_400_000);
    assert_eq!(out.dropped, 1);
    assert_eq!(out.passed, 0);
    let state = get_state(&conn, "gone").unwrap();
    assert!(!state.enabled);
}
```

- [ ] **Step 3: Run the tests — verify they fail**

Run: `cd src-tauri && cargo test --lib memory_graph::spaced_repetition::tests::run_sr 2>&1 | tail -20`
Expected: FAIL — `cannot find function run_spaced_repetition_blocking`.

- [ ] **Step 4: Implement the orchestrator**

Add after `select_enrollable`:

```rust
/// openhuman-D — one spaced-repetition tick on a held `&Connection`.
///
/// Phase 1: auto-enroll high-importance, recall-worthy, unenrolled nodes.
/// Phase 2: review every due node using Slice C's importance score as the
///          grader — `importance >= threshold` (and not archived) ⇒ PASS
///          (advance the ladder, collect for re-projection); otherwise DROP
///          (`set_enabled(false)` — C's archival owns the node from here).
///
/// MUST be called with a `&Connection` the caller already holds; it runs raw
/// SQL and never re-locks the store mutex (non-reentrant → would deadlock).
/// All per-node DB errors log + continue so one bad node never aborts the batch.
pub fn run_spaced_repetition_blocking(
    conn: &Connection,
    kinds: &[&str],
    threshold: f64,
    batch_size: usize,
    now_ms: i64,
) -> SpacedRepetitionOutcome {
    let mut outcome = SpacedRepetitionOutcome::default();

    // Phase 1 — enroll.
    match select_enrollable(conn, kinds, threshold, batch_size) {
        Ok(ids) => {
            for id in ids {
                match enroll_node(conn, &id, now_ms) {
                    Ok(()) => outcome.enrolled += 1,
                    Err(e) => tracing::warn!(node_id = %id, error = %e, "sr: enroll failed"),
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "sr: select_enrollable failed"),
    }

    // Phase 2 — review due nodes.
    let due = match due_now(conn, now_ms, batch_size) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "sr: due_now failed");
            return outcome;
        }
    };
    for node_id in due {
        // One combined read: archived_at + current active text + importance.
        let row: Option<(Option<i64>, Option<String>, Option<f64>)> = conn
            .query_row(
                "SELECT n.archived_at, v.content, s.importance
                 FROM memory_nodes n
                 LEFT JOIN memory_versions v
                     ON v.node_id = n.id AND v.status = 'active'
                 LEFT JOIN memory_importance_scores s ON s.node_id = n.id
                 WHERE n.id = ?1
                 ORDER BY v.created_at DESC
                 LIMIT 1",
                params![node_id],
                |r| {
                    Ok((
                        r.get::<_, Option<i64>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<f64>>(2)?,
                    ))
                },
            )
            .optional()
            .unwrap_or(None);

        let (archived_at, content, importance) = match row {
            Some(t) => t,
            None => {
                // node vanished — drop it from the queue
                let _ = set_enabled(conn, &node_id, false);
                outcome.dropped += 1;
                continue;
            }
        };

        let keep = archived_at.is_none() && importance.map(|i| i >= threshold).unwrap_or(false);
        if keep {
            if let Err(e) = record_pass(conn, &node_id, now_ms) {
                tracing::warn!(node_id = %node_id, error = %e, "sr: record_pass failed");
                continue;
            }
            outcome.passed += 1;
            if let Some(text) = content {
                outcome.reinforce.push(SpacedRepetitionReinforce {
                    node_id: node_id.clone(),
                    text,
                });
            }
        } else {
            if let Err(e) = set_enabled(conn, &node_id, false) {
                tracing::warn!(node_id = %node_id, error = %e, "sr: set_enabled(false) failed");
                continue;
            }
            outcome.dropped += 1;
        }
    }

    outcome
}
```

- [ ] **Step 5: Run the tests — verify they pass**

Run: `cd src-tauri && cargo test --lib memory_graph::spaced_repetition 2>&1 | tail -20`
Expected: PASS (all SR tests, old + new).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_graph/spaced_repetition.rs
git commit -m "feat(memory): run_spaced_repetition_blocking orchestrator (enroll + importance-graded review) for Slice D"
```

---

## Task 3: Config knobs

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`

- [ ] **Step 1: Add the default functions**

Near `default_importance_archive_threshold` (~line 442-452), add:

```rust
fn default_spaced_repetition_enabled() -> bool {
    true
}

fn default_spaced_repetition_batch_size() -> u32 {
    50
}

fn default_spaced_repetition_importance_threshold() -> f64 {
    0.6
}
```

- [ ] **Step 2: Add the struct fields**

In the `MemoryOsRuntimeConfig` struct, after the `importance_archive_user_profile` field (~line 636), add:

```rust
/// openhuman-D — gates the periodic spaced-repetition scan. When ON,
/// every 360 ticks (~3h, right after the importance recompute) the loop
/// auto-enrolls high-importance nodes and reviews due ones (importance is
/// the grader; zero LLM). Default true. Reversible via this flag.
#[serde(default = "default_spaced_repetition_enabled")]
pub spaced_repetition_enabled: bool,

/// openhuman-D — max nodes per SR batch (enrollment + due review each
/// bounded by this). SQL-only, single-digit ms. Default 50; 0 disables.
#[serde(default = "default_spaced_repetition_batch_size")]
pub spaced_repetition_batch_size: u32,

/// openhuman-D — importance score a node needs to stay enrolled / be
/// enrolled. Gates both enrollment and the pass/drop decision. Default 0.6
/// (matches `spaced_repetition::ENROLLMENT_IMPORTANCE_THRESHOLD`).
#[serde(default = "default_spaced_repetition_importance_threshold")]
pub spaced_repetition_importance_threshold: f64,
```

- [ ] **Step 3: Add the Default-impl construction values**

In the `MemoryOsRuntimeConfig` Default/construction site, after `importance_archive_user_profile: false,` (~line 813), add:

```rust
// openhuman-D defaults — see field docs + default_spaced_repetition_*().
spaced_repetition_enabled: true,
spaced_repetition_batch_size: 50,
spaced_repetition_importance_threshold: 0.6,
```

- [ ] **Step 4: Write the failing test**

Find the existing `memory_os_config_*` test(s) near the bottom of the file. Add:

```rust
#[test]
fn memory_os_config_spaced_repetition_defaults() {
    // empty JSON → all SR knobs fall back to documented defaults
    let cfg: MemoryOsRuntimeConfig = serde_json::from_str("{}").unwrap();
    assert!(cfg.spaced_repetition_enabled);
    assert_eq!(cfg.spaced_repetition_batch_size, 50);
    assert!((cfg.spaced_repetition_importance_threshold - 0.6).abs() < f64::EPSILON);
}
```

(If `MemoryOsRuntimeConfig` does not `#[derive(Deserialize)]` directly or the existing tests use a different deserialization entry point, mirror the exact pattern of the adjacent `importance_archive` config test instead — match how that one constructs/deserializes.)

- [ ] **Step 5: Run — verify fail then pass**

Run: `cd src-tauri && cargo test --lib memubot_config::tests::memory_os_config_spaced_repetition 2>&1 | tail -20`
Expected first: FAIL (no such field / test). After steps 1-3 it should already compile+pass — if you wrote the test first against missing fields it fails to compile, which counts as the failing state. Confirm PASS after fields exist.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memubot_config.rs
git commit -m "feat(memory): spaced_repetition_* config knobs (enabled/batch_size/threshold) for Slice D"
```

---

## Task 4: Proactive tick wiring

**Files:**
- Modify: `src-tauri/src/proactive/service.rs`

- [ ] **Step 1: Locate the insertion point**

Find the end of the Slice C importance-decay block (the `}` closing `if refs.memory_os.importance_decay_enabled … % 360 == 0 { … }`, ~line 1501). The SR job goes immediately after it (same scope, same tick function), so it shares the `%360` cadence and reads freshly-recomputed importance.

- [ ] **Step 2: Add the SR job**

Insert after the importance-decay block's closing brace:

```rust
// openhuman-D — Spaced-repetition scan. Importance is the grader (zero LLM).
// Runs every 360 ticks (~3h), right after the importance recompute above so
// it reads fresh scores. Phase 1 enroll + Phase 2 review run in the blocking
// closure (holds conn); the async half re-projects passed nodes into the
// bucket_seal recall surface (reuses Slice A project_fact + Slice B hotness).
if refs.memory_os.spaced_repetition_enabled
    && refs.memory_os.spaced_repetition_batch_size > 0
    && refs.tick_count.load(Ordering::SeqCst) % 360 == 0
{
    let store = refs.memory_graph_store.clone();
    let batch = refs.memory_os.spaced_repetition_batch_size as usize;
    let threshold = refs.memory_os.spaced_repetition_importance_threshold;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let outcome = tokio::task::spawn_blocking(
        move || -> crate::memory_graph::spaced_repetition::SpacedRepetitionOutcome {
            let conn = match store.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, "[ProactiveService] spaced_repetition: DB lock failed");
                    return Default::default();
                }
            };
            crate::memory_graph::spaced_repetition::run_spaced_repetition_blocking(
                &conn,
                crate::memory_graph::spaced_repetition::SR_KINDS,
                threshold,
                batch,
                now_ms,
            )
        },
    )
    .await
    .unwrap_or_default();

    // async half (conn dropped) — re-project passed nodes back into the
    // recall surface + bump hotness. Best-effort: per-node failure logs.
    if !outcome.reinforce.is_empty() {
        if let Some(adapter) = &refs.bucket_seal_adapter {
            for r in &outcome.reinforce {
                crate::memory_adapter::recall_projection::project_fact(
                    adapter,
                    &r.node_id,
                    &r.text,
                )
                .await;
                if let Err(e) = adapter.reinforce_recalled(&[r.node_id.clone()], now_ms).await {
                    tracing::debug!(node_id = %r.node_id, error = %e, "sr: reinforce_recalled failed");
                }
            }
        }
    }
    if outcome.enrolled + outcome.passed + outcome.dropped > 0 {
        tracing::info!(
            enrolled = outcome.enrolled,
            passed = outcome.passed,
            dropped = outcome.dropped,
            "[ProactiveService] spaced_repetition tick"
        );
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (clean). If `SpacedRepetitionOutcome` or `SR_KINDS` aren't found, confirm they're `pub` (Task 1/2) and the module path `crate::memory_graph::spaced_repetition::` is correct.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/proactive/service.rs
git commit -m "feat(memory): wire spaced-repetition scan into proactive %360 tick + async re-project half (Slice D)"
```

---

## Task 5: Whole-slice verification + ship

**Files:** none (verification + ship only)

- [ ] **Step 1: Build + clippy clean**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → expect empty.
Run: `cd src-tauri && cargo clippy --lib 2>&1 | grep -E "^error|^warning: unused" | head` → expect empty (no new warnings from D's files).

- [ ] **Step 2: Targeted + broad dependent test run (the Slice-C lesson)**

Run: `cd src-tauri && cargo test --lib memory_graph::spaced_repetition memory_graph::importance_decay memubot_config proactive memory_adapter 2>&1 | tail -25`
Expected: all green. The broad set catches any fixture that builds `memory_graph`/`proactive` and now touches the `memory_importance_scores ⋈ spaced_repetition_state` join.

- [ ] **Step 3: Grep gates (confirm scope discipline)**

```bash
# SR orchestrator is called from exactly one prod site (the tick) — and tests
grep -rn "run_spaced_repetition_blocking" src-tauri/src --include=*.rs | grep -v "spaced_repetition.rs"
# expect: exactly one hit in proactive/service.rs
# no LLM call added in the SR path
grep -n "complete\|chat\|llm\|prompt" src-tauri/src/memory_graph/spaced_repetition.rs
# expect: no LLM invocation (only the const/struct/fn code)
```

- [ ] **Step 4: Update GitNexus index**

Run: `npx gitnexus analyze` (from repo root) → expect success (node/edge counts printed).

- [ ] **Step 5: Open the PR**

Push the branch and open a PR with a `## Commits (bisectable)` table (T1 select_enrollable, T2 orchestrator, T3 config, T4 tick wiring). Body notes the cross-cutting facts: no migration (V45 reused), zero LLM (importance-as-grader), reuses A `project_fact` + B `reinforce_recalled`, drop = `set_enabled(false)` (not record_fail). Link the spec.

- [ ] **Step 6: Merge + closeout**

After merge: sync local main (`git fetch origin -q && git pull --ff-only origin main`), delete remote+local branch, remove the worktree, re-run `npx gitnexus analyze` on main, update memory (`project-openhuman-deepening.md` → Slice D SHIPPED + next recommend E; `MEMORY.md` index line).

---

## Self-Review

**Spec coverage:** §1 enrollment → Task 1. §2 orchestrator (enroll + importance-graded pass/drop, reinforce collection) → Task 2. §3 tick wiring + async re-project → Task 4. §4 config (3 knobs) → Task 3. §5 reinforce semantics (re-project + reinforce_recalled) → Task 4 async half. Testing items 1-7 → Task 1/2/3 tests + Task 5 broad run. ✓

**Placeholder scan:** All SQL, signatures, and struct literals are concrete (pinned from recon). The only conditional instruction is Task 3 Step 4's "mirror the adjacent test if deserialization differs" — that's a guard against an unseen entry point, with a concrete fallback (copy the importance_archive test), not a TODO. ✓

**Type consistency:** `SpacedRepetitionOutcome` fields (`enrolled/passed/dropped/reinforce`) used identically in Task 2 impl, Task 2 tests, and Task 4 tick. `SpacedRepetitionReinforce {node_id, text}` consistent across def/test/tick. `SR_KINDS`/`run_spaced_repetition_blocking`/`select_enrollable` names identical everywhere. `record_pass`/`set_enabled`/`due_now`/`enroll_node`/`get_state` match the pinned live signatures. ✓

**Deadlock check:** orchestrator takes `&Connection` and runs only raw SQL + the SR module's `&Connection` fns — never `store.*` lockers. Tick does `store.conn.lock()` once, passes `&conn`. ✓
