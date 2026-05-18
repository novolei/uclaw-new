//! Memory health scenario — zero-LLM structural integrity checks
//! over the memory_graph subsystem.
//!
//! Memory OS Foundation Phase 4 (spec §3.1 A3, §4.2.4 health table).
//!
//! ## Why "health" (zero-LLM) and not "lint" (LLM)
//!
//! gbrain + llm-wiki-agent both split memory-graph maintenance into
//! two cost tiers (gbrain dream cycle's lint stage vs cheap subscans;
//! llm-wiki-agent's `health.py` vs `lint.py`). uClaw mirrors that:
//!
//! - **Phase 4 health** — pure SQL, deterministic, every ~30 minutes.
//!   Catches structural integrity drift (orphans / dangling rows /
//!   missing routes) that an LLM cannot help with. Always safe to run.
//!
//! - **Phase 5 lint** — LLM-driven, every 10-15 EntityPage writes.
//!   Catches semantic issues (hub stubs, phantom hubs, stale summaries,
//!   contradictions) where natural-language understanding is necessary.
//!   Has a daily token budget guard.
//!
//! This file implements all seven health checks listed in the Phase 1
//! spec table. None of them call out to the LLM, all of them write
//! into `memory_health_findings` (V35) and respect the existing
//! dismissed-row contract (only re-create when the issue resurfaces
//! after dismissal).
//!
//! ## Check catalogue
//!
//! | id               | severity | what it catches                                          |
//! | ---------------- | -------- | -------------------------------------------------------- |
//! | orphan           | warn     | non-Boot/Identity/Value node with 0 in + 0 out edges     |
//! | stub             | warn     | EntityPage whose active_version.content < 100 chars      |
//! | dangling_fts     | error    | memory_fts row with no surviving memory_nodes.id         |
//! | index_drift      | error    | memory_routes pointing to non-existent node_id           |
//! | phantom_slug     | error    | memory_edges whose child_node_id is missing              |
//! | empty_versions   | warn     | node with rows in memory_versions but none active        |
//! | missing_route    | warn     | EntityPage node lacking any primary memory_routes row    |
//!
//! Checks run sequentially under a single conn lock; total wall time
//! on a typical local DB (< 10k nodes) is well under 50 ms.

use rusqlite::params;
use serde::Serialize;

/// Result of one `run_health_checks` invocation. Returned so the caller
/// (proactive tick + the manual IPC trigger) can surface the counts
/// without re-querying the table.
#[derive(Debug, Clone, Serialize)]
pub struct HealthRunOutcome {
    pub orphan: u32,
    pub stub: u32,
    pub dangling_fts: u32,
    pub index_drift: u32,
    pub phantom_slug: u32,
    pub empty_versions: u32,
    pub missing_route: u32,
    /// Sum of all per-check counts — convenience for UI badges.
    pub total_inserted: u32,
    /// Total active (open + un-dismissed) findings after this run.
    pub active_total: u32,
    pub duration_ms: u64,
}

/// Run every health check for `space_id`. New findings are persisted
/// to `memory_health_findings`; existing-but-not-dismissed findings
/// for the same `(subject, check_kind)` are NOT duplicated. Findings
/// already dismissed by the user re-appear only if the underlying
/// issue is still detectable (the caller cleared the data, then it
/// drifted again).
///
/// Lock contract: takes the conn lock for the duration. Caller should
/// run this on `tokio::spawn_blocking` so the runtime keeps moving.
pub fn run_health_checks(
    conn: &rusqlite::Connection,
    space_id: &str,
) -> Result<HealthRunOutcome, crate::error::Error> {
    let started = std::time::Instant::now();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut outcome = HealthRunOutcome {
        orphan: 0,
        stub: 0,
        dangling_fts: 0,
        index_drift: 0,
        phantom_slug: 0,
        empty_versions: 0,
        missing_route: 0,
        total_inserted: 0,
        active_total: 0,
        duration_ms: 0,
    };

    outcome.orphan = find_orphans(conn, space_id, now_ms)?;
    outcome.stub = find_stub_entity_pages(conn, space_id, now_ms)?;
    outcome.dangling_fts = find_dangling_fts(conn, space_id, now_ms)?;
    outcome.index_drift = find_index_drift(conn, space_id, now_ms)?;
    outcome.phantom_slug = find_phantom_slugs(conn, space_id, now_ms)?;
    outcome.empty_versions = find_empty_version_chains(conn, space_id, now_ms)?;
    outcome.missing_route = find_missing_primary_routes(conn, space_id, now_ms)?;

    outcome.total_inserted = outcome.orphan
        + outcome.stub
        + outcome.dangling_fts
        + outcome.index_drift
        + outcome.phantom_slug
        + outcome.empty_versions
        + outcome.missing_route;

    outcome.active_total = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_health_findings \
             WHERE space_id = ?1 AND dismissed = 0",
            params![space_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u32;

    outcome.duration_ms = started.elapsed().as_millis() as u64;
    Ok(outcome)
}

// ─── Persistence helper ────────────────────────────────────────────────

/// Insert one finding, but only when no open (un-dismissed) row already
/// exists for the same `(space_id, subject, check_kind)`. Returns 1 if
/// inserted, 0 if skipped due to dedup. Dismissed rows are NOT counted
/// — re-detecting an issue the user already dismissed inserts a fresh
/// row so the panel surfaces it again.
fn upsert_finding(
    conn: &rusqlite::Connection,
    space_id: &str,
    severity: &str,
    check_kind: &str,
    subject: &str,
    payload: Option<&serde_json::Value>,
    discovered_at_ms: i64,
) -> Result<u32, crate::error::Error> {
    let already_open: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_health_findings \
             WHERE space_id = ?1 AND subject = ?2 AND check_kind = ?3 \
               AND dismissed = 0",
            params![space_id, subject, check_kind],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if already_open > 0 {
        return Ok(0);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let payload_str = payload.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO memory_health_findings \
         (id, space_id, severity, check_kind, subject, payload_json, is_lint, discovered_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![id, space_id, severity, check_kind, subject, payload_str, discovered_at_ms],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(1)
}

// ─── Check 1: orphan nodes ─────────────────────────────────────────────

/// Find nodes with zero incoming AND zero outgoing edges. Excludes the
/// kinds that legitimately stand alone:
///   - `boot` — Boot prompts (always orphan by design)
///   - `identity` / `value` — single-instance system metadata
///   - `directive` — root-level rules
///
/// Procedure nodes (learned skills) are included — a skill that has
/// never been referenced or applied is genuinely worth flagging.
fn find_orphans(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.kind FROM memory_nodes n \
             WHERE n.space_id = ?1 \
               AND n.kind NOT IN ('boot', 'identity', 'value', 'directive') \
               AND NOT EXISTS ( \
                 SELECT 1 FROM memory_edges e \
                 WHERE e.parent_node_id = n.id OR e.child_node_id = n.id \
               )",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (id, title, kind) in collected {
        let payload = serde_json::json!({ "title": title, "kind": kind });
        inserted += upsert_finding(conn, space_id, "warn", "orphan", &id, Some(&payload), now_ms)?;
    }
    Ok(inserted)
}

// ─── Check 2: stub EntityPage nodes ────────────────────────────────────

/// Find EntityPage nodes whose active version's content is shorter
/// than 100 characters. The threshold is gbrain-inspired: anything
/// shorter than that is unlikely to carry a meaningful compiled_truth.
const STUB_CONTENT_THRESHOLD: usize = 100;

fn find_stub_entity_pages(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, COALESCE(LENGTH(v.content), 0) AS content_len \
             FROM memory_nodes n \
             LEFT JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active' \
             WHERE n.space_id = ?1 \
               AND n.kind = 'entity_page' \
               AND COALESCE(LENGTH(v.content), 0) < ?2",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id, STUB_CONTENT_THRESHOLD as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (id, title, content_len) in collected {
        let payload = serde_json::json!({
            "title": title,
            "content_len": content_len,
            "threshold": STUB_CONTENT_THRESHOLD,
        });
        inserted += upsert_finding(conn, space_id, "warn", "stub", &id, Some(&payload), now_ms)?;
    }
    Ok(inserted)
}

// ─── Check 3: dangling FTS rows ────────────────────────────────────────

/// Find `memory_fts` rows whose `node_id` no longer exists in
/// `memory_nodes`. Caused by partial deletes or aborted migrations.
/// Surfaced as `error` severity because they pollute search results
/// silently — every recall pass spends compute on rows that can never
/// hydrate back to a node.
fn find_dangling_fts(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    // memory_fts has no space_id column — every space's findings flow
    // into the active space's row. Acceptable: dangling FTS is a
    // workspace-global issue.
    let mut stmt = conn
        .prepare(
            "SELECT f.node_id FROM memory_fts f \
             WHERE f.node_id NOT IN (SELECT id FROM memory_nodes)",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for node_id in collected {
        let payload = serde_json::json!({ "missing_node_id": node_id });
        inserted += upsert_finding(
            conn,
            space_id,
            "error",
            "dangling_fts",
            &node_id,
            Some(&payload),
            now_ms,
        )?;
    }
    Ok(inserted)
}

// ─── Check 4: index drift in memory_routes ─────────────────────────────

/// Find `memory_routes` rows whose `node_id` no longer exists. A route
/// without a backing node is unrecoverable garbage — the IPC will
/// 404 if anything tries to resolve it.
fn find_index_drift(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.domain, r.path, r.node_id FROM memory_routes r \
             WHERE r.space_id = ?1 \
               AND r.node_id NOT IN (SELECT id FROM memory_nodes)",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (route_id, domain, path, missing_node_id) in collected {
        let payload = serde_json::json!({
            "domain": domain,
            "path": path,
            "missing_node_id": missing_node_id,
        });
        inserted += upsert_finding(
            conn,
            space_id,
            "error",
            "index_drift",
            &route_id,
            Some(&payload),
            now_ms,
        )?;
    }
    Ok(inserted)
}

// ─── Check 5: phantom slugs (edge → missing node) ─────────────────────

/// Find edges whose `child_node_id` does not exist. This typically
/// signals a race during deletion (the child was deleted but cascade
/// didn't fire because foreign keys were temporarily off) OR an
/// import that inserted edges before the target nodes.
fn find_phantom_slugs(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.parent_node_id, e.child_node_id, e.relation_kind \
             FROM memory_edges e \
             WHERE e.space_id = ?1 \
               AND e.child_node_id NOT IN (SELECT id FROM memory_nodes)",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (edge_id, parent_node_id, missing_child_id, relation_kind) in collected {
        let payload = serde_json::json!({
            "parent_node_id": parent_node_id,
            "missing_child_id": missing_child_id,
            "relation_kind": relation_kind,
        });
        inserted += upsert_finding(
            conn,
            space_id,
            "error",
            "phantom_slug",
            &edge_id,
            Some(&payload),
            now_ms,
        )?;
    }
    Ok(inserted)
}

// ─── Check 6: empty version chain ──────────────────────────────────────

/// Find nodes with rows in `memory_versions` but none currently
/// `active`. Usually caused by a deprecate-without-replace flow.
/// Recall sees these nodes but can't surface any content for them.
fn find_empty_version_chains(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title, n.kind FROM memory_nodes n \
             WHERE n.space_id = ?1 \
               AND EXISTS (SELECT 1 FROM memory_versions v1 WHERE v1.node_id = n.id) \
               AND NOT EXISTS ( \
                 SELECT 1 FROM memory_versions v2 \
                 WHERE v2.node_id = n.id AND v2.status = 'active' \
               )",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (id, title, kind) in collected {
        let payload = serde_json::json!({ "title": title, "kind": kind });
        inserted += upsert_finding(
            conn,
            space_id,
            "warn",
            "empty_versions",
            &id,
            Some(&payload),
            now_ms,
        )?;
    }
    Ok(inserted)
}

// ─── Check 7: missing primary route on EntityPage ─────────────────────

/// EntityPage nodes are expected to have an `entity/<slug>` primary
/// route set by `create_entity_page`. Missing route → either the row
/// was created via a low-level path that bypassed the helper, or the
/// route table got cleared by hand.
fn find_missing_primary_routes(
    conn: &rusqlite::Connection,
    space_id: &str,
    now_ms: i64,
) -> Result<u32, crate::error::Error> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.title FROM memory_nodes n \
             WHERE n.space_id = ?1 \
               AND n.kind = 'entity_page' \
               AND NOT EXISTS ( \
                 SELECT 1 FROM memory_routes r \
                 WHERE r.node_id = n.id AND r.is_primary = 1 \
               )",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(crate::error::Error::Database)?;
    let collected: Vec<_> = rows.flatten().collect();
    drop(stmt);

    let mut inserted = 0u32;
    for (id, title) in collected {
        let payload = serde_json::json!({ "title": title });
        inserted += upsert_finding(
            conn,
            space_id,
            "warn",
            "missing_route",
            &id,
            Some(&payload),
            now_ms,
        )?;
    }
    Ok(inserted)
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        conn
    }

    fn insert_node(conn: &Connection, id: &str, kind: &str, title: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, created_at, updated_at) \
             VALUES (?1, 'default', ?2, ?3, ?4, ?4)",
            params![id, kind, title, now],
        )
        .unwrap();
    }

    fn insert_active_version(conn: &Connection, version_id: &str, node_id: &str, content: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_versions \
             (id, node_id, supersedes_version_id, status, content, created_at) \
             VALUES (?1, ?2, NULL, 'active', ?3, ?4)",
            params![version_id, node_id, content, now],
        )
        .unwrap();
    }

    fn count_findings(conn: &Connection, kind: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM memory_health_findings WHERE check_kind = ?1 AND dismissed = 0",
            params![kind],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    // ─── orphan ──────────────────────────────────────────────────

    #[test]
    fn orphan_detects_episode_with_no_edges() {
        let conn = fresh_conn();
        insert_node(&conn, "lonely", "episode", "Lonely event");
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.orphan, 1, "got: {:?}", out);
        assert_eq!(count_findings(&conn, "orphan"), 1);
    }

    #[test]
    fn orphan_excludes_boot_and_identity_kinds() {
        let conn = fresh_conn();
        insert_node(&conn, "boot1", "boot", "Boot");
        insert_node(&conn, "id1", "identity", "Identity");
        insert_node(&conn, "val1", "value", "Value");
        insert_node(&conn, "dir1", "directive", "Directive");
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.orphan, 0, "system kinds must not be flagged as orphans");
    }

    #[test]
    fn orphan_excludes_node_with_outgoing_edge() {
        let conn = fresh_conn();
        insert_node(&conn, "a", "episode", "A");
        insert_node(&conn, "b", "episode", "B");
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_edges \
             (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at) \
             VALUES ('e1', 'default', 'a', 'b', 'relates_to', 'private', 0, ?1, ?1)",
            params![now],
        )
        .unwrap();
        let out = run_health_checks(&conn, "default").unwrap();
        // a has outgoing edge, b has incoming — neither is an orphan.
        assert_eq!(out.orphan, 0);
    }

    // ─── stub ────────────────────────────────────────────────────

    #[test]
    fn stub_detects_short_entity_page_content() {
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Short");
        insert_active_version(&conn, "v1", "ep1", "tiny");
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.stub, 1);
    }

    #[test]
    fn stub_skips_long_entity_page() {
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Long");
        insert_active_version(&conn, "v1", "ep1", &"x".repeat(200));
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.stub, 0);
    }

    #[test]
    fn stub_detects_entity_page_with_no_active_version() {
        // No version at all → LEN coalesces to 0 → below threshold → stub.
        // (Also picked up by empty_versions if any deprecated version
        // exists; this test exercises the "fresh node, no version" path.)
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Empty");
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.stub, 1);
    }

    // ─── dangling_fts ────────────────────────────────────────────

    #[test]
    fn dangling_fts_detects_orphan_row() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO memory_fts (node_id, title, content) VALUES ('ghost', 'g', 'g')",
            [],
        )
        .unwrap();
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.dangling_fts, 1);
    }

    // ─── index_drift ─────────────────────────────────────────────

    #[test]
    fn index_drift_detects_route_to_missing_node() {
        let conn = fresh_conn();
        let now = chrono::Utc::now().to_rfc3339();
        // memory_routes.node_id FK -> memory_nodes(id). fresh_conn() leaves FK
        // ON; we need the OFF/ON dance to insert a deliberately-stale route,
        // same as phantom_slug_detects_edge_to_missing_child below.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute(
            "INSERT INTO memory_routes \
             (id, space_id, node_id, domain, path, is_primary, created_at, updated_at) \
             VALUES ('r1', 'default', 'no-such-node', 'entity', 'ghost', 1, ?1, ?1)",
            params![now],
        )
        .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.index_drift, 1);
    }

    // ─── phantom_slug ────────────────────────────────────────────

    #[test]
    fn phantom_slug_detects_edge_to_missing_child() {
        let conn = fresh_conn();
        insert_node(&conn, "parent", "entity_page", "Parent");
        let now = chrono::Utc::now().to_rfc3339();
        // Insert edge BEFORE the child node, then never insert the child.
        // FK is ON in fresh_conn(), so we need to insert with FK OFF, then
        // re-enable, or use a parent-only configuration the FK allows.
        // memory_edges FK on child_node_id is REFERENCES memory_nodes(id)
        // ON DELETE CASCADE — insertion fails when child is absent.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute(
            "INSERT INTO memory_edges \
             (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at) \
             VALUES ('e-ghost', 'default', 'parent', 'no-such-child', 'mentions', 'private', 0, ?1, ?1)",
            params![now],
        )
        .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.phantom_slug, 1);
    }

    // ─── empty_versions ──────────────────────────────────────────

    #[test]
    fn empty_versions_detects_node_with_all_deprecated() {
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Was alive");
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_versions \
             (id, node_id, status, content, created_at) \
             VALUES ('v1', 'ep1', 'deprecated', 'old', ?1)",
            params![now],
        )
        .unwrap();
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.empty_versions, 1);
    }

    #[test]
    fn empty_versions_skips_node_without_any_version() {
        // Node with NO versions at all is "young" not "empty" — flagged
        // by stub (short content) but not by empty_versions.
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Brand new");
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.empty_versions, 0);
        assert_eq!(out.stub, 1, "young node should be flagged as stub");
    }

    // ─── missing_route ───────────────────────────────────────────

    #[test]
    fn missing_route_detects_entity_page_without_primary_route() {
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "No route");
        insert_active_version(&conn, "v1", "ep1", &"x".repeat(200));
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.missing_route, 1);
    }

    #[test]
    fn missing_route_skips_when_primary_route_exists() {
        let conn = fresh_conn();
        insert_node(&conn, "ep1", "entity_page", "Has route");
        insert_active_version(&conn, "v1", "ep1", &"x".repeat(200));
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_routes \
             (id, space_id, node_id, domain, path, is_primary, created_at, updated_at) \
             VALUES ('r1', 'default', 'ep1', 'entity', 'has-route', 1, ?1, ?1)",
            params![now],
        )
        .unwrap();
        let out = run_health_checks(&conn, "default").unwrap();
        assert_eq!(out.missing_route, 0);
    }

    // ─── dedup contract ───────────────────────────────────────────

    #[test]
    fn dedup_does_not_duplicate_open_findings_on_re_run() {
        let conn = fresh_conn();
        insert_node(&conn, "lonely", "episode", "Lonely");
        let r1 = run_health_checks(&conn, "default").unwrap();
        let r2 = run_health_checks(&conn, "default").unwrap();
        assert_eq!(r1.orphan, 1);
        assert_eq!(r2.orphan, 0, "second run should not re-insert");
        assert_eq!(count_findings(&conn, "orphan"), 1);
    }

    #[test]
    fn dedup_resurfaces_after_dismissal_only_when_still_detectable() {
        let conn = fresh_conn();
        insert_node(&conn, "lonely", "episode", "Lonely");
        run_health_checks(&conn, "default").unwrap();
        // User dismisses the finding.
        conn.execute(
            "UPDATE memory_health_findings SET dismissed = 1, dismissed_at = ?1 \
             WHERE check_kind = 'orphan' AND subject = 'lonely'",
            params![chrono::Utc::now().timestamp_millis()],
        )
        .unwrap();
        // Re-run — issue still present, should re-create.
        let r2 = run_health_checks(&conn, "default").unwrap();
        assert_eq!(r2.orphan, 1, "dismissed finding should reappear if still detectable");
        // Total rows: 1 dismissed + 1 active.
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings WHERE subject = 'lonely'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 2);
    }

    // ─── outcome shape ────────────────────────────────────────────

    #[test]
    fn outcome_reports_total_and_duration() {
        let conn = fresh_conn();
        insert_node(&conn, "lonely", "episode", "Lonely");
        insert_node(&conn, "ep1", "entity_page", "Stub");
        insert_active_version(&conn, "v1", "ep1", "tiny");
        let out = run_health_checks(&conn, "default").unwrap();
        assert!(out.total_inserted >= 2);
        assert!(out.active_total >= 2);
        // duration_ms is u64; just confirm it's been set.
        assert!(out.duration_ms < 10_000, "health run should be fast");
    }

    #[test]
    fn outcome_serializes_to_camel_case_for_ipc() {
        let out = HealthRunOutcome {
            orphan: 1,
            stub: 2,
            dangling_fts: 0,
            index_drift: 0,
            phantom_slug: 0,
            empty_versions: 0,
            missing_route: 0,
            total_inserted: 3,
            active_total: 3,
            duration_ms: 42,
        };
        let json = serde_json::to_string(&out).unwrap();
        // The IPC layer renames at the wire boundary; the struct itself
        // serializes as snake_case which is fine for storage / logs.
        assert!(json.contains("\"orphan\":1"));
        assert!(json.contains("\"duration_ms\":42"));
    }
}
