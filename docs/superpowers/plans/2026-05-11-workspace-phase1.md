# Workspace Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [`docs/superpowers/specs/2026-05-11-workspace-phase1-design.md`](../specs/2026-05-11-workspace-phase1-design.md)

**Goal:** Stop the workspace feature's silent failures: persist `default`, heal orphan agent_sessions, remove 17 phantom IPC wrappers + their broken UI, add IPC validation and application-layer delete-cascade.

**Architecture:** One database migration (V16) for data healing. Three small Rust helper functions extracted from existing Tauri commands so the validation logic is unit-testable without an `AppState` mock. Frontend deletes phantom wrappers first; the TypeScript compiler then fails at every dependent call site, which forces explicit cleanup of `WorkspaceSelector`, `WorkspaceFilesView` (formerly `SidePanel`), and `AgentView`.

**Tech Stack:** Rust + rusqlite + Tauri 2 + React 18 + TypeScript + Jotai (no new dependencies).

---

## File Structure

**Modify (Rust):**
- `src-tauri/src/db/migrations.rs` — add `V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL` const, run-block, and `#[cfg(test)] mod tests` (Task 1)
- `src-tauri/src/tauri_commands.rs` — remove synthetic-default branch in `list_spaces` (Task 2); add three helper fns + wire them into `create_agent_session`, `move_agent_session_to_workspace`, `delete_workspace`; add `#[cfg(test)] mod workspace_integrity_tests` (Task 3)

**Modify (TS):**
- `ui/src/lib/tauri-bridge.ts` — delete 17 phantom wrappers (Task 4)
- `ui/src/components/agent/WorkspaceSelector.tsx` — drop rename + reorder UI; switch to real IPC (Task 5)
- `ui/src/components/agent/SidePanel.tsx` — drop attached-dirs subtree, slug derivation, in-Finder buttons (Task 6)
- `ui/src/components/agent/AgentView.tsx` — drop `getWorkspaceFilesPath` + `attachDirectory` flows (Task 7)
- `ui/src/atoms/agent-atoms.ts` — delete `agentAttachedDirectoriesMapAtom`, `workspaceAttachedDirectoriesMapAtom` (Task 8)
- `ui/src/lib/agent-types.ts` — drop `slug` from `AgentWorkspace` (Task 8)

**No file creations.**

---

## Conventions for this plan

- Run from repo root `/Users/ryanliu/Documents/uclaw` unless noted.
- Branch is already `claude/workspace-phase1` (created during brainstorming, contains the spec).
- Each task ends with a commit. Commit messages are pre-written — copy verbatim.
- After each commit, `cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head` should be empty (no compile errors). For TS-only tasks, run `cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10`.

---

### Task 1: V16 migration — persist default workspace + heal orphans

**Files:**
- Modify: `src-tauri/src/db/migrations.rs:619-739` (append the V16 const and run-block; append a `#[cfg(test)] mod tests` block at end of file)

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/db/migrations.rs` AFTER the `pub fn run(...)` function ends (around line 739):

```rust

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    /// Apply only the migrations needed to set up `spaces` and `agent_sessions`,
    /// stopping BEFORE V16 so tests can drive V16 themselves and observe
    /// pre/post state.
    fn db_pre_v16() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // V1 creates `spaces`. V8 creates `agent_sessions`. We don't need the
        // intermediate migrations because none of them touch the columns
        // we're testing here.
        conn.execute_batch(super::V1_INITIAL).unwrap();
        // V8 contains a multi-statement block; use execute_batch.
        conn.execute_batch(super::V8_AGENT_SESSIONS).unwrap();
        conn
    }

    fn run_v16(conn: &Connection) {
        for stmt in super::V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v16_inserts_default_idempotent() {
        let conn = db_pre_v16();

        // First run inserts 'default'.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "first V16 run should insert one 'default' row");

        // Second run is a no-op.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "second V16 run must not create a duplicate");
    }

    #[test]
    fn v16_heals_orphan_agent_sessions() {
        let conn = db_pre_v16();

        // Pre-V16: insert an agent_session pointing at a workspace that does
        // not exist in `spaces`.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-orphan', 'ghost-workspace', 'orphaned session', 0, 0)",
            [],
        )
        .unwrap();

        run_v16(&conn);

        // Post-V16: orphan should be re-homed to 'default'.
        let space_id: String = conn
            .query_row(
                "SELECT space_id FROM agent_sessions WHERE id = 's-orphan'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(space_id, "default", "orphan session must be re-homed to 'default'");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib v16 2>&1 | tail -30`

Expected: compile error — `V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL` not defined.

- [ ] **Step 3: Add the V16 const**

Insert immediately after the `V15_AGENT_MESSAGE_METRICS` declaration (currently ends around line 626 with `";`):

```rust

/// V16: persist the 'default' workspace as a real DB row (replaces the
/// synthetic in-memory fallback in list_spaces) and re-home agent_sessions
/// whose space_id points at a workspace that doesn't exist (orphan healing
/// from before this migration). Idempotent — safe to re-run.
pub const V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL: &str = "
INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('default', '默认工作区', '📁', NULL, datetime('now'), datetime('now'));

UPDATE agent_sessions
SET space_id = 'default'
WHERE space_id NOT IN (SELECT id FROM spaces);
";
```

- [ ] **Step 4: Wire V16 into `run()`**

Inside `pub fn run(...)`, AFTER the V15 block (currently ends around line 736 with `}`), add:

```rust
    // V16: persist 'default' workspace + heal orphan agent_sessions.
    tracing::debug!("Running migration V16: workspace default + orphan heal");
    for stmt in V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V16 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib v16 2>&1 | tail -10`

Expected:
```
running 2 tests
test db::migrations::tests::v16_heals_orphan_agent_sessions ... ok
test db::migrations::tests::v16_inserts_default_idempotent ... ok

test result: ok. 2 passed; 0 failed; ...
```

- [ ] **Step 6: Run full migration chain to confirm no regression**

Run: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head`

Expected: no output (clean build). If output: stop and read errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations.rs
git commit -m "$(cat <<'EOF'
chore(db): V16 migration — persist 'default' workspace + heal orphans

Adds V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL: idempotent INSERT OR IGNORE of
the 'default' row into spaces, plus an UPDATE that re-homes any
agent_sessions whose space_id points at a workspace not in spaces. This is
the data-integrity foundation for Phase 1 — list_spaces() can stop
synthesizing the default in memory (Task 2), and downstream code can rely
on 'default' existing.

Inline tests:
- v16_inserts_default_idempotent: two runs produce one row, not two
- v16_heals_orphan_agent_sessions: pre-existing orphan gets re-homed

No FK constraint added — see spec §3 non-goals (Phase 2 work, requires
SQLite table-recreate dance).
EOF
)"
```

---

### Task 2: Remove synthetic-default branch from `list_spaces`

After V16, the `'default'` row always exists, so the in-memory fallback is dead.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs:933-963` (the `list_spaces` function body)

- [ ] **Step 1: Read the current implementation to confirm shape**

Run: `sed -n '933,963p' src-tauri/src/tauri_commands.rs`

Expected: the function reads `spaces` ordered by `created_at DESC`, then if `spaces.is_empty()` returns a synthetic `vec![SpaceResponse { id: "default", ... }]`; else returns the real list.

- [ ] **Step 2: Replace function body**

Replace lines 933-963 with:

```rust
pub async fn list_spaces(state: State<'_, AppState>) -> Result<Vec<SpaceResponse>, Error> {
    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let mut stmt = db.prepare(
        "SELECT id, name, icon, created_at, updated_at FROM spaces ORDER BY created_at DESC",
    ).map_err(Error::Database)?;

    let spaces: Vec<SpaceResponse> = stmt.query_map([], |row| {
        Ok(SpaceResponse {
            id: row.get(0)?,
            name: row.get(1)?,
            icon: row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }).map_err(Error::Database)?
    .filter_map(|r| r.ok())
    .collect();

    Ok(spaces)
}
```

(Drops the `if spaces.is_empty() { ... } else { Ok(spaces) }` block; always returns the DB result.)

- [ ] **Step 3: Verify compile**

Run: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head`

Expected: empty.

- [ ] **Step 4: Verify list_spaces returns 'default' after V16**

There is no targeted test for `list_spaces` in this task; the V16 migration test from Task 1 already proves `default` is in the table. Manual verification will happen during the smoke test phase.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
refactor(workspace): drop synthetic-default branch from list_spaces

V16 ensures 'default' is always a real row in spaces. The empty-table
fallback that synthesized a transient in-memory SpaceResponse is now
unreachable — delete it. list_spaces becomes a pure DB query.
EOF
)"
```

---

### Task 3: IPC validation + delete_workspace re-homes agent_sessions

Add three small helper functions extracted from existing commands so the integrity logic is unit-testable without `AppState`. Wire them into `create_agent_session` (tolerant), `move_agent_session_to_workspace` (strict), and `delete_workspace` (cascade + refuse-default).

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add 3 helper fns near top of workspace section ~line 4438; modify `create_agent_session` at 3763, `move_agent_session_to_workspace` at 4265, `delete_workspace` at 4459; append `#[cfg(test)] mod workspace_integrity_tests` near end of file)

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/tauri_commands.rs` near the end of the file (after the existing `#[cfg(test)] mod cost_rollup_tests` block):

```rust

#[cfg(test)]
mod workspace_integrity_tests {
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V1_INITIAL).unwrap();
        conn.execute_batch(crate::db::migrations::V8_AGENT_SESSIONS).unwrap();
        // Apply V16 to insert 'default'.
        for stmt in crate::db::migrations::V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
        conn
    }

    fn insert_workspace(conn: &Connection, id: &str, name: &str) {
        conn.execute(
            "INSERT INTO spaces (id, name, icon, created_at, updated_at)
             VALUES (?1, ?2, '📁', datetime('now'), datetime('now'))",
            rusqlite::params![id, name],
        ).unwrap();
    }

    fn insert_session(conn: &Connection, id: &str, space_id: &str) {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES (?1, ?2, 'test', 0, 0)",
            rusqlite::params![id, space_id],
        ).unwrap();
    }

    fn space_id_of(conn: &Connection, session_id: &str) -> String {
        conn.query_row(
            "SELECT space_id FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn resolve_workspace_id_passes_through_existing() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-real", "real");
        let resolved = super::resolve_workspace_id_or_default(&conn, Some("ws-real".into()));
        assert_eq!(resolved, "ws-real");
    }

    #[test]
    fn resolve_workspace_id_falls_back_for_unknown() {
        let conn = fresh_db();
        let resolved = super::resolve_workspace_id_or_default(&conn, Some("ghost".into()));
        assert_eq!(resolved, "default");
    }

    #[test]
    fn resolve_workspace_id_falls_back_for_none() {
        let conn = fresh_db();
        let resolved = super::resolve_workspace_id_or_default(&conn, None);
        assert_eq!(resolved, "default");
    }

    #[test]
    fn require_workspace_exists_ok_when_present() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-real", "real");
        assert!(super::require_workspace_exists(&conn, "ws-real").is_ok());
    }

    #[test]
    fn require_workspace_exists_err_when_missing() {
        let conn = fresh_db();
        assert!(super::require_workspace_exists(&conn, "ghost").is_err());
    }

    #[test]
    fn rehome_agent_sessions_moves_them_to_default() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "x");
        insert_session(&conn, "s-1", "ws-x");
        insert_session(&conn, "s-2", "ws-x");

        super::rehome_agent_sessions_to_default(&conn, "ws-x").unwrap();

        assert_eq!(space_id_of(&conn, "s-1"), "default");
        assert_eq!(space_id_of(&conn, "s-2"), "default");
    }

    #[test]
    fn rehome_does_nothing_when_no_sessions_in_workspace() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-empty", "empty");
        // No sessions inserted.
        let result = super::rehome_agent_sessions_to_default(&conn, "ws-empty");
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib workspace_integrity 2>&1 | tail -20`

Expected: compile errors — `resolve_workspace_id_or_default`, `require_workspace_exists`, `rehome_agent_sessions_to_default` not defined.

- [ ] **Step 3: Add the three helper functions**

Insert these three functions in `tauri_commands.rs` immediately BEFORE the `pub async fn create_workspace(` declaration (currently line 4441). They become module-private helpers that the Tauri commands call.

```rust
// ─── Workspace integrity helpers ──────────────────────────────────────
//
// Extracted as standalone fns so they can be unit-tested without an
// AppState mock. See `workspace_integrity_tests` at the bottom of this
// file. Phase 1 spec §4.3.

/// Validate `workspace_id` exists in `spaces`. Falls back to `'default'`
/// silently (with a warning log) for unknown values, including `None`.
/// Used by automatic flows like `create_agent_session` where a stale
/// frontend ID should not block session creation.
pub(crate) fn resolve_workspace_id_or_default(
    conn: &rusqlite::Connection,
    workspace_id: Option<String>,
) -> String {
    let candidate = match workspace_id {
        None => return "default".into(),
        Some(id) => id,
    };
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM spaces WHERE id = ?1",
            rusqlite::params![&candidate],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if exists {
        candidate
    } else {
        tracing::warn!(
            "create_agent_session: unknown workspace_id={candidate:?}, falling back to 'default'"
        );
        "default".into()
    }
}

/// Validate `workspace_id` exists. Returns `Err` if not. Used by explicit
/// user actions like `move_agent_session_to_workspace` where a silent
/// re-route would surprise the user.
pub(crate) fn require_workspace_exists(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<(), Error> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM spaces WHERE id = ?1",
            rusqlite::params![workspace_id],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if exists {
        Ok(())
    } else {
        Err(Error::Internal(format!(
            "workspace_id '{workspace_id}' does not exist"
        )))
    }
}

/// Re-home all agent_sessions in the given workspace to `'default'`.
/// Application-layer equivalent of `ON DELETE SET DEFAULT` (the FK does
/// not exist on agent_sessions.space_id — see Phase 1 spec §3 non-goals).
/// Called by `delete_workspace` BEFORE the DELETE FROM spaces statement.
pub(crate) fn rehome_agent_sessions_to_default(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE agent_sessions SET space_id = 'default', updated_at = ?2 WHERE space_id = ?1",
        rusqlite::params![workspace_id, chrono::Utc::now().timestamp_millis()],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Run helper tests to verify they pass**

Run: `cd src-tauri && cargo test --lib workspace_integrity 2>&1 | tail -15`

Expected: 7 tests pass.

- [ ] **Step 5: Wire `resolve_workspace_id_or_default` into `create_agent_session`**

Replace the body of `create_agent_session` (currently lines 3763-3792). The change is one line — `let space_id` derivation moves inside the conn lock and uses the helper. Find:

```rust
pub async fn create_agent_session(
    state: State<'_, AppState>,
    title: Option<String>,
    channel_id: Option<String>,
    workspace_id: Option<String>,
) -> Result<serde_json::Value, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "New session".into());
    let space_id = workspace_id.unwrap_or_else(|| "default".into());
    let now = chrono::Utc::now().timestamp_millis();
    let meta = serde_json::json!({ "channelId": channel_id });
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,0,0,?5,?5)",
            rusqlite::params![id, space_id, title, meta.to_string(), now],
        ).map_err(|e| Error::Database(e))?;
    }
    Ok(serde_json::json!({
        "id": id,
        "workspaceId": space_id,
        "title": title,
        "messageCount": 0,
        "pinned": false,
        "archived": false,
        "createdAt": now,
        "updatedAt": now,
    }))
}
```

Replace with:

```rust
pub async fn create_agent_session(
    state: State<'_, AppState>,
    title: Option<String>,
    channel_id: Option<String>,
    workspace_id: Option<String>,
) -> Result<serde_json::Value, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "New session".into());
    let now = chrono::Utc::now().timestamp_millis();
    let meta = serde_json::json!({ "channelId": channel_id });
    let space_id = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
        let resolved = resolve_workspace_id_or_default(&conn, workspace_id);
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES (?1,?2,?3,?4,0,0,0,?5,?5)",
            rusqlite::params![id, &resolved, title, meta.to_string(), now],
        ).map_err(|e| Error::Database(e))?;
        resolved
    };
    Ok(serde_json::json!({
        "id": id,
        "workspaceId": space_id,
        "title": title,
        "messageCount": 0,
        "pinned": false,
        "archived": false,
        "createdAt": now,
        "updatedAt": now,
    }))
}
```

- [ ] **Step 6: Wire `require_workspace_exists` into `move_agent_session_to_workspace`**

Replace the body of `move_agent_session_to_workspace` (lines 4265-4279). Find:

```rust
pub async fn move_agent_session_to_workspace(
    state: State<'_, AppState>,
    input: MoveSessionInput,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    conn.execute(
        "UPDATE agent_sessions SET space_id = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![
            input.target_workspace_id,
            chrono::Utc::now().timestamp_millis(),
            input.session_id,
        ],
    ).map_err(|e| Error::Database(e))?;
    Ok(())
}
```

Replace with:

```rust
pub async fn move_agent_session_to_workspace(
    state: State<'_, AppState>,
    input: MoveSessionInput,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    require_workspace_exists(&conn, &input.target_workspace_id)?;
    conn.execute(
        "UPDATE agent_sessions SET space_id = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![
            input.target_workspace_id,
            chrono::Utc::now().timestamp_millis(),
            input.session_id,
        ],
    ).map_err(|e| Error::Database(e))?;
    Ok(())
}
```

- [ ] **Step 7: Wire cascade + refuse-default into `delete_workspace`**

Replace `delete_workspace` (lines 4458-4475). Find:

```rust
pub async fn delete_workspace(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let active: Option<String> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'active_workspace_id'",
        [],
        |row| row.get::<_, String>(0),
    ).ok();
    if active.as_deref() == Some(&id) {
        let _ = conn.execute("DELETE FROM settings WHERE key = 'active_workspace_id'", []);
    }
    conn.execute("DELETE FROM spaces WHERE id = ?1", rusqlite::params![id])
        .map_err(Error::Database)?;
    Ok(())
}
```

Replace with:

```rust
pub async fn delete_workspace(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    if id == "default" {
        return Err(Error::Internal(
            "cannot delete the 'default' workspace".into(),
        ));
    }
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    // If this workspace is currently active, clear the setting so the next
    // active_workspace_root() call falls back to the global default.
    let active: Option<String> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'active_workspace_id'",
        [],
        |row| row.get::<_, String>(0),
    ).ok();
    if active.as_deref() == Some(&id) {
        let _ = conn.execute("DELETE FROM settings WHERE key = 'active_workspace_id'", []);
    }

    // Application-layer cascade: re-home agent_sessions to 'default' BEFORE
    // dropping the workspace row. agent_sessions has no FK constraint, so
    // without this, sessions would be silently orphaned. (Conversations
    // already cascade via FK ON DELETE CASCADE — see V1_INITIAL.)
    rehome_agent_sessions_to_default(&conn, &id).map_err(Error::Database)?;

    conn.execute("DELETE FROM spaces WHERE id = ?1", rusqlite::params![id])
        .map_err(Error::Database)?;
    Ok(())
}
```

- [ ] **Step 8: Verify all tests still pass + clean build**

Run: `cd src-tauri && cargo test --lib workspace_integrity 2>&1 | tail -10`

Expected: 7 helper tests still pass.

Run: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head`

Expected: empty.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "$(cat <<'EOF'
feat(workspace): IPC validation + delete_workspace re-homes sessions

Three application-layer integrity helpers extracted from existing Tauri
commands so the logic is unit-testable without an AppState mock:

- resolve_workspace_id_or_default: tolerant lookup (falls back to
  'default' on unknown / None) — used by create_agent_session
- require_workspace_exists: strict lookup (Err on unknown) — used by
  move_agent_session_to_workspace
- rehome_agent_sessions_to_default: UPDATE-style cascade — used by
  delete_workspace BEFORE the DELETE FROM spaces, since
  agent_sessions.space_id has no FK constraint

delete_workspace also now refuses to delete the 'default' row.

Inline tests cover each helper (7 unit tests).

Phase 1 spec §4.3.
EOF
)"
```

---

### Task 4: Remove 17 phantom IPC wrappers from `tauri-bridge.ts`

This is a deletion-only task. The TypeScript compiler will start failing in WorkspaceSelector / SidePanel / AgentView; Tasks 5-7 fix those.

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: Delete the workspace-directory wrappers**

Find this block (lines 824-846):

```typescript
export const getWorkspaceDirectories = (workspaceSlug: string): Promise<string[]> =>
  invoke<string[]>('get_workspace_directories', { workspaceSlug }).catch(() => [])

export const getWorkspaceFilesPath = (workspaceSlug: string): Promise<string> =>
  invoke<string>('get_workspace_files_path', { workspaceSlug }).catch(() => '')

export const attachDirectory = (input: { sessionId: string; directoryPath: string }): Promise<string[]> =>
  invoke<string[]>('attach_directory', { input })

export const detachDirectory = (input: { sessionId: string; directoryPath: string }): Promise<string[]> =>
  invoke<string[]>('detach_directory', { input })

export const attachWorkspaceDirectory = (input: { workspaceSlug: string; directoryPath: string }): Promise<string[]> =>
  invoke<string[]>('attach_workspace_directory', { input })

export const detachWorkspaceDirectory = (input: { workspaceSlug: string; directoryPath: string }): Promise<string[]> =>
  invoke<string[]>('detach_workspace_directory', { input })

export const listAttachedDirectory = (dirPath: string): Promise<any[]> =>
  invoke<any[]>('list_attached_directory', { dirPath }).catch(() => [])

export const readAttachedFile = (path: string, sessionId: string, workspaceSlug?: string): Promise<string> =>
  invoke('read_attached_file', { path, sessionId, workspaceSlug: workspaceSlug ?? null })
```

Delete it entirely (the whole 8-export block).

- [ ] **Step 2: Delete the file-action wrappers (the dialog + attached-file group)**

Find this block (lines 848-865):

```typescript
// --- File operations ---
export const openFile = (path: string): Promise<void> =>
  invoke<void>('open_file', { path }).catch(() => {})

export const openAttachedFile = (path: string): Promise<void> =>
  invoke<void>('open_attached_file', { path }).catch(() => {})

export const showAttachedInFolder = (path: string): Promise<void> =>
  invoke<void>('show_attached_in_folder', { path }).catch(() => {})

export const renameAttachedFile = (path: string, newName: string): Promise<void> =>
  invoke<void>('rename_attached_file', { path, newName }).catch(() => {})

export const moveAttachedFile = (path: string, destDir: string): Promise<void> =>
  invoke<void>('move_attached_file', { path, destDir }).catch(() => {})
```

Delete entirely. Keep the `// --- File operations ---` comment line — replace the whole block including the comment with nothing.

NOTE: `saveImageAs` and `openExternal` (lines 864-868) might also be phantom but are NOT in our spec's deletion list. Leave them alone. Verify by searching for their backend handlers separately if the user reports issues post-Phase-1.

- [ ] **Step 3: Delete `openFolderDialog`**

Find (lines 775-776):

```typescript
export const openFolderDialog = (): Promise<{ path: string; name: string } | null> =>
  invoke<{ path: string; name: string } | null>('open_folder_dialog').catch(() => null)
```

Delete entirely.

- [ ] **Step 4: Delete the agent workspace compat block**

Find this block (lines 919-930):

```typescript
// --- Agent workspace compat ---
export const createAgentWorkspace = (name: string): Promise<any> =>
  invoke('create_agent_workspace', { name }).catch(() => ({ id: crypto.randomUUID(), name, slug: name.toLowerCase(), createdAt: Date.now(), updatedAt: Date.now() }))

export const updateAgentWorkspace = (id: string, patch: any): Promise<any> =>
  invoke('update_agent_workspace', { id, patch }).catch(() => ({ id, ...patch, updatedAt: Date.now() }))

export const deleteAgentWorkspace = (id: string): Promise<void> =>
  invoke<void>('delete_agent_workspace', { id }).catch(() => {})

export const reorderAgentWorkspaces = (ids: string[]): Promise<void> =>
  invoke<void>('reorder_agent_workspaces', { ids }).catch(() => {})
```

Delete entirely (including the `// --- Agent workspace compat ---` comment).

- [ ] **Step 5: Verify the count of deleted wrappers**

Count check — there should now be ZERO occurrences of any phantom IPC name in tauri-bridge.ts:

```bash
grep -nE "invoke\(['\"](create_agent_workspace|update_agent_workspace|delete_agent_workspace|reorder_agent_workspaces|attach_workspace_directory|detach_workspace_directory|get_workspace_directories|get_workspace_files_path|attach_directory|detach_directory|list_attached_directory|read_attached_file|open_attached_file|show_attached_in_folder|rename_attached_file|move_attached_file|open_folder_dialog|open_file)['\"]" ui/src/lib/tauri-bridge.ts
```

Expected: empty.

- [ ] **Step 6: Verify the TypeScript compile fails (intentional)**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -30`

Expected: errors in `WorkspaceSelector.tsx`, `SidePanel.tsx`, `AgentView.tsx` saying these wrappers are not found. **This is intentional** — Tasks 5-7 fix the call sites.

Save the error output so the next tasks can verify they hit the expected sites:

```bash
cd ui && npx tsc --noEmit 2>&1 | grep -E "Cannot find name" | sort -u | head -20
```

Should mention names from: `createAgentWorkspace`, `updateAgentWorkspace`, `deleteAgentWorkspace`, `reorderAgentWorkspaces`, `getWorkspaceDirectories`, `attachDirectory`, `detachDirectory`, `attachWorkspaceDirectory`, `detachWorkspaceDirectory`, `listAttachedDirectory`, `readAttachedFile`, `getWorkspaceFilesPath`, `openFile`, `openAttachedFile`, `showAttachedInFolder`, `renameAttachedFile`, `moveAttachedFile`, `openFolderDialog`.

- [ ] **Step 7: Commit (despite TS errors — broken state expected)**

The build is intentionally broken at this commit; later commits restore it. CLAUDE.md says "create commits when requested by user" — this commit IS requested by the plan. The `## Commits (bisectable)` PR description should warn that commits 4-7 form an internal repair cycle (the build is green again at commit 8). Note: per CLAUDE.md "never skip hooks" — pre-commit hooks may complain about TS errors. If they do, discuss with the user whether the hook should be smarter (e.g., not run TS check on TS-only deletion commits) — do NOT use `--no-verify` to bypass.

```bash
git add ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
chore(ui): delete 17 phantom IPC wrappers from tauri-bridge

These wrappers invoked Tauri commands that have never existed in the
Rust backend. They are split into two categories:
- 4 workspace mutators with .catch fallbacks that returned fake stubs,
  giving the user the illusion that create/rename/reorder worked while
  nothing persisted (createAgentWorkspace, updateAgentWorkspace,
  deleteAgentWorkspace, reorderAgentWorkspaces).
- 13 file/dialog/attached-dir wrappers with no real implementation
  (open_file, open_folder_dialog, read/open/rename/move/show
  _attached_file, list/attach/detach _directory, get_workspace_*).

Removing the wrappers makes TypeScript fail at every dependent call
site. Tasks 5-7 (next 3 commits) repair WorkspaceSelector, SidePanel,
and AgentView. The build is intentionally red at this commit and green
again at commit 8.

Phase 1 spec §4.4 step 1.
EOF
)"
```

---

### Task 5: WorkspaceSelector uses real IPC, drops rename + reorder UI

**Files:**
- Modify: `ui/src/components/agent/WorkspaceSelector.tsx`

- [ ] **Step 1: Update imports**

Find the import block (lines 29-35):

```typescript
import {
  updateSettings,
  createAgentWorkspace,
  updateAgentWorkspace,
  deleteAgentWorkspace,
  reorderAgentWorkspaces,
} from '@/lib/tauri-bridge'
```

Replace with:

```typescript
import {
  updateSettings,
  createWorkspace,
  deleteWorkspace,
} from '@/lib/tauri-bridge'
```

(Drops 3 phantom imports; keeps `updateSettings`; brings in real `createWorkspace` and `deleteWorkspace`.)

- [ ] **Step 2: Update icons import**

Find (line 10):

```typescript
import { FolderOpen, Plus, Pencil, Trash2, GripVertical } from 'lucide-react'
```

Replace with:

```typescript
import { FolderOpen, Plus, Trash2 } from 'lucide-react'
```

(Drops `Pencil` and `GripVertical` — used only by rename/reorder UI.)

- [ ] **Step 3: Drop the resize-handle state and handler**

The resize handle for `workspaceListHeightAtom` is fine — keep it. **Skip this step.** (Just here as a no-op anchor in the plan to make it clear we considered and kept it.)

- [ ] **Step 4: Drop the rename state**

Find these state declarations (lines 91-94):

```typescript
  // 重命名状态
  const [editingId, setEditingId] = React.useState<string | null>(null)
  const [editName, setEditName] = React.useState('')
  const editInputRef = React.useRef<HTMLInputElement>(null)
```

Delete entirely.

- [ ] **Step 5: Drop the drag state**

Find (lines 99-101):

```typescript
  // 拖拽状态
  const [dragId, setDragId] = React.useState<string | null>(null)
  const [dropIndicator, setDropIndicator] = React.useState<{ id: string; position: 'before' | 'after' } | null>(null)
```

Delete entirely.

- [ ] **Step 6: Update `handleSelect` to drop `editingId` reference**

Find (lines 104-111):

```typescript
  /** 切换工作区 */
  const handleSelect = (workspace: AgentWorkspace): void => {
    if (editingId) return
    setCurrentWorkspaceId(workspace.id)

    updateSettings({
      agentWorkspaceId: workspace.id,
    }).catch(console.error)
  }
```

Replace with:

```typescript
  /** 切换工作区 */
  const handleSelect = (workspace: AgentWorkspace): void => {
    setCurrentWorkspaceId(workspace.id)

    updateSettings({
      agentWorkspaceId: workspace.id,
    }).catch(console.error)
  }
```

- [ ] **Step 7: Update `handleCreate` to use real `createWorkspace`**

Find (lines 123-148):

```typescript
  const handleCreate = async (): Promise<void> => {
    const trimmed = newName.trim()
    if (!trimmed) {
      setCreating(false)
      return
    }
    if (createInFlightRef.current) return
    createInFlightRef.current = true

    try {
      const workspace = await createAgentWorkspace(trimmed)
      setWorkspaces((prev) => [workspace, ...prev])
      setCurrentWorkspaceId(workspace.id)
      setCreating(false)

      updateSettings({
        agentWorkspaceId: workspace.id,
      }).catch(console.error)
    } catch (error) {
      const msg = error instanceof Error ? error.message : '创建失败'
      toast.error(msg)
      setCreating(false)
    } finally {
      createInFlightRef.current = false
    }
  }
```

Replace with:

```typescript
  const handleCreate = async (): Promise<void> => {
    const trimmed = newName.trim()
    if (!trimmed) {
      setCreating(false)
      return
    }
    if (createInFlightRef.current) return
    createInFlightRef.current = true

    try {
      // Real backend command: returns { id, name, icon, path, createdAt }.
      // We adapt to AgentWorkspace shape (no slug after Task 8).
      const created = await createWorkspace(trimmed)
      const workspace: AgentWorkspace = {
        id: created.id,
        name: created.name,
        createdAt: Date.parse(created.createdAt) || Date.now(),
        updatedAt: Date.parse(created.createdAt) || Date.now(),
      }
      setWorkspaces((prev) => [workspace, ...prev])
      setCurrentWorkspaceId(workspace.id)
      setCreating(false)

      updateSettings({
        agentWorkspaceId: workspace.id,
      }).catch(console.error)
    } catch (error) {
      const msg = error instanceof Error ? error.message : '创建失败'
      toast.error(msg)
      setCreating(false)
    } finally {
      createInFlightRef.current = false
    }
  }
```

(Note: `AgentWorkspace.slug` is removed in Task 8. If TS complains here that `slug` is required, that's expected at this step — Task 8 fixes the type. Move on.)

- [ ] **Step 8: Drop the rename handlers**

Find this section (lines 160-200, the `// ===== 重命名 =====` block through `handleRenameKeyDown`):

```typescript
  // ===== 重命名 =====

  const handleStartRename = (e: React.MouseEvent, ws: AgentWorkspace): void => {
    e.stopPropagation()
    setEditingId(ws.id)
    setEditName(ws.name)
    requestAnimationFrame(() => {
      editInputRef.current?.focus()
      editInputRef.current?.select()
    })
  }

  const handleRename = async (): Promise<void> => {
    if (!editingId) return
    const trimmed = editName.trim()

    if (!trimmed) {
      setEditingId(null)
      return
    }

    try {
      const updated = await updateAgentWorkspace(editingId, { name: trimmed })
      setWorkspaces((prev) => prev.map((w) => (w.id === updated.id ? updated : w)))
    } catch (error) {
      const msg = error instanceof Error ? error.message : '重命名失败'
      toast.error(msg)
    } finally {
      setEditingId(null)
    }
  }

  const handleRenameKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      if (e.nativeEvent.isComposing) return
      e.preventDefault()
      handleRename()
    } else if (e.key === 'Escape') {
      setEditingId(null)
    }
  }
```

Delete entirely (whole section through the closing `}` of `handleRenameKeyDown`).

- [ ] **Step 9: Update `handleConfirmDelete` to use real `deleteWorkspace`**

Find (lines 209-228):

```typescript
  const handleConfirmDelete = async (): Promise<void> => {
    if (!deleteTargetId) return

    try {
      await deleteAgentWorkspace(deleteTargetId)
      const remaining = workspaces.filter((w) => w.id !== deleteTargetId)
      setWorkspaces(remaining)

      if (deleteTargetId === currentWorkspaceId && remaining.length > 0) {
        setCurrentWorkspaceId(remaining[0]!.id)
        updateSettings({
          agentWorkspaceId: remaining[0]!.id,
        }).catch(console.error)
      }
    } catch (error) {
      console.error('[WorkspaceSelector] 删除工作区失败:', error)
    } finally {
      setDeleteTargetId(null)
    }
  }
```

Replace with:

```typescript
  const handleConfirmDelete = async (): Promise<void> => {
    if (!deleteTargetId) return

    try {
      await deleteWorkspace(deleteTargetId)
      const remaining = workspaces.filter((w) => w.id !== deleteTargetId)
      setWorkspaces(remaining)

      if (deleteTargetId === currentWorkspaceId && remaining.length > 0) {
        setCurrentWorkspaceId(remaining[0]!.id)
        updateSettings({
          agentWorkspaceId: remaining[0]!.id,
        }).catch(console.error)
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : '删除失败'
      toast.error(msg)
    } finally {
      setDeleteTargetId(null)
    }
  }
```

(Switches to real wrapper; surfaces backend `'cannot delete the default workspace'` error via toast instead of swallowed console.error.)

- [ ] **Step 10: Update `canDelete` to use `id` instead of `slug`**

Find (lines 230-232):

```typescript
  const canDelete = (ws: AgentWorkspace): boolean => {
    return ws.slug !== 'default' && workspaces.length > 1
  }
```

Replace with:

```typescript
  const canDelete = (ws: AgentWorkspace): boolean => {
    return ws.id !== 'default' && workspaces.length > 1
  }
```

- [ ] **Step 11: Drop drag handlers**

Find the drag handlers section (lines 234-303, from `// ===== 拖拽排序 =====` through `handleDragEnd`):

```typescript
  // ===== 拖拽排序 =====

  const handleDragStart = (e: React.DragEvent, wsId: string): void => {
    setDragId(wsId)
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', wsId)
  }

  const handleDragOver = (e: React.DragEvent, wsId: string): void => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    if (!dragId || wsId === dragId) {
      setDropIndicator(null)
      return
    }
    const rect = e.currentTarget.getBoundingClientRect()
    const ratio = (e.clientY - rect.top) / rect.height
    let position: 'before' | 'after'
    if (ratio < 0.35) {
      position = 'before'
    } else if (ratio > 0.65) {
      position = 'after'
    } else {
      if (dropIndicator?.id === wsId) return
      position = ratio < 0.5 ? 'before' : 'after'
    }
    if (dropIndicator?.id === wsId && dropIndicator.position === position) return
    setDropIndicator({ id: wsId, position })
  }

  const handleDragLeave = (e: React.DragEvent): void => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setDropIndicator(null)
    }
  }

  const handleDrop = (e: React.DragEvent, targetId: string): void => {
    e.preventDefault()
    if (!dragId || dragId === targetId || !dropIndicator || dropIndicator.id !== targetId) {
      setDragId(null)
      setDropIndicator(null)
      return
    }

    const fromIdx = workspaces.findIndex((w) => w.id === dragId)
    const toIdx = workspaces.findIndex((w) => w.id === targetId)
    if (fromIdx === -1 || toIdx === -1) return

    const reordered = [...workspaces]
    const [moved] = reordered.splice(fromIdx, 1)
    const adjustedToIdx = fromIdx < toIdx ? toIdx - 1 : toIdx
    const insertIdx = dropIndicator.position === 'after' ? adjustedToIdx + 1 : adjustedToIdx
    reordered.splice(insertIdx, 0, moved!)

    setWorkspaces(reordered)
    setDragId(null)
    setDropIndicator(null)

    reorderAgentWorkspaces(reordered.map((w) => w.id)).catch(console.error)
  }

  const handleDragEnd = (): void => {
    setDragId(null)
    setDropIndicator(null)
  }
```

Delete entirely (whole block).

- [ ] **Step 12: Simplify the JSX render — drop drag attrs, drop indicators, drop rename input, drop pencil button**

Find the workspace-list-item render section (lines 326-396, the `workspaces.map((ws) => ...)` block).

Replace the entire `workspaces.map` block with:

```typescript
          {workspaces.map((ws) => (
            <div key={ws.id} className="relative">
              <div
                onClick={() => handleSelect(ws)}
                className={cn(
                  'group w-full flex items-center gap-1 px-1 py-[5px] rounded-md text-[13px] transition-colors duration-100 cursor-pointer titlebar-no-drag',
                  ws.id === currentWorkspaceId
                    ? 'workspace-item-selected bg-foreground/[0.08] text-foreground shadow-[0_1px_2px_0_rgba(0,0,0,0.05)]'
                    : 'text-foreground/70 hover:bg-foreground/[0.04]',
                )}
              >
                <FolderOpen size={13} className="flex-shrink-0 text-foreground/40" />

                <span className="flex-1 min-w-0 truncate">{ws.name}</span>

                <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                  {canDelete(ws) && (
                    <button
                      onClick={(e) => handleStartDelete(e, ws.id)}
                      className="p-0.5 rounded hover:bg-destructive/10 text-foreground/30 hover:text-destructive transition-colors"
                      title="删除"
                    >
                      <Trash2 size={12} />
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
```

(Drops: `draggable`, `onDragStart`, `onDragOver`, `onDragLeave`, `onDrop`, `onDragEnd` props; drop indicators; `GripVertical`; rename input branch; pencil rename button. Keeps: select on click, hover-revealed delete button, selection highlight.)

- [ ] **Step 13: Verify TS compiles for this file**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep "WorkspaceSelector" | head`

Expected: NO errors mentioning WorkspaceSelector.tsx (lingering errors in SidePanel.tsx and AgentView.tsx are normal — Tasks 6-7 fix them; some `slug` errors may remain because the type still has it — fixed in Task 8).

- [ ] **Step 14: Commit**

```bash
git add ui/src/components/agent/WorkspaceSelector.tsx
git commit -m "$(cat <<'EOF'
feat(ui): WorkspaceSelector uses real create/delete IPC, drops rename+reorder

- createAgentWorkspace (phantom, .catch returned fake stub) →
  createWorkspace (real). Adapter normalizes the response shape into
  AgentWorkspace.
- deleteAgentWorkspace (phantom) → deleteWorkspace (real). Backend now
  also rejects deleting 'default' (Task 3); surface that as a toast
  rather than swallowed console.error.
- Inline rename UI removed: updateAgentWorkspace was phantom, the field
  appeared to work but never persisted.
- Drag-to-reorder UI removed: reorderAgentWorkspaces was phantom, no
  reorder ever persisted.
- canDelete uses ws.id instead of ws.slug (slug is removed from
  AgentWorkspace in Task 8).

Lost UX (until Phase 2 adds backends): rename, drag-reorder.

Phase 1 spec §4.4 steps 2.
EOF
)"
```

---

### Task 6: WorkspaceFilesView drops attached-dirs subtree, slug, in-Finder buttons

The file is `SidePanel.tsx` but exports `WorkspaceFilesView` (renamed in earlier work on this branch). It currently has ~880 lines; this task removes ~500 of them — the entire `AttachedDirsSection` / `AttachedDirTree` / `AttachedDirItem` subtree plus their handlers in the main component.

**Files:**
- Modify: `ui/src/components/agent/SidePanel.tsx`

- [ ] **Step 1: Trim the lucide-react imports to only what's still used**

Current import (line 10):

```typescript
import { X, FolderOpen, ExternalLink, RefreshCw, ChevronRight, MoreHorizontal, FolderSearch, Pencil, FolderInput, Info, FolderHeart, MessageSquarePlus } from 'lucide-react'
```

After removing AttachedDirsSection / AttachedDirTree / AttachedDirItem (Step 5) and the in-Finder buttons (Steps 6-7), the only icons still used are: `FolderOpen`, `RefreshCw`, `Info`, `FolderHeart`. Replace with:

```typescript
import { FolderOpen, RefreshCw, Info, FolderHeart } from 'lucide-react'
```

- [ ] **Step 2: Drop unused jotai atom imports**

Find the import block (lines 21-29):

```typescript
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
  agentWorkspacesAtom,
  agentAttachedDirectoriesMapAtom,
  workspaceAttachedDirectoriesMapAtom,
  agentPendingFilesAtom,
} from '@/atoms/agent-atoms'
```

Replace with:

```typescript
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
  agentWorkspacesAtom,
  agentPendingFilesAtom,
} from '@/atoms/agent-atoms'
```

(Drops `agentAttachedDirectoriesMapAtom` and `workspaceAttachedDirectoriesMapAtom` — both deleted in Task 8.)

- [ ] **Step 3: Trim the tauri-bridge imports**

Find the import block (lines 32-47):

```typescript
import {
  getWorkspaceDirectories,
  attachDirectory,
  openFolderDialog,
  detachDirectory,
  attachWorkspaceDirectory,
  detachWorkspaceDirectory,
  readAttachedFile,
  getWorkspaceFilesPath,
  openFile,
  listAttachedDirectory,
  openAttachedFile,
  showAttachedInFolder,
  renameAttachedFile,
  moveAttachedFile,
} from '@/lib/tauri-bridge'
```

Delete this entire `import { ... } from '@/lib/tauri-bridge'` block. (No tauri-bridge wrappers remain in use after this task.)

- [ ] **Step 4: Drop unused dropdown-menu / button / tooltip imports if they only support deleted handlers**

After deletion, `Tooltip` is still used (header has Info tooltips), `DropdownMenu` is no longer used (only AttachedDirItem used it), `Button` is no longer used.

Find (lines 11-18):

```typescript
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
```

Replace with:

```typescript
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
```

- [ ] **Step 5: Drop the `chat-types` and `agent-types` imports if unused**

Find (lines 30-31):

```typescript
import type { FileEntry } from '@/lib/chat-types'
import type { AgentPendingFile } from '@/lib/agent-types'
```

After deleting handlers, `FileEntry` is still used by `handleAddToChat` and the FileBrowser callback. `AgentPendingFile` is used by the `setPendingFiles` flow. Both stay. **No change.**

- [ ] **Step 6: Drop the slug-related state derivation and attached-dirs hooks**

Find this block (lines ~64-92, the slug derivation through attached-dirs map subscription):

```typescript
  const filesVersion = useAtomValue(workspaceFilesVersionAtom)
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)
  const hasFileChanges = filesVersion > 0

  // 派生当前工作区 slug：优先使用会话自身关联的 workspaceId，其次回落到全局当前工作区。
  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId
  const workspaces = useAtomValue(agentWorkspacesAtom)
  const workspaceSlug = workspaces.find((w) => w.id === currentWorkspaceId)?.slug ?? null

  // 附加目录列表（会话级）
  const attachedDirsMap = useAtomValue(agentAttachedDirectoriesMapAtom)
  const setAttachedDirsMap = useSetAtom(agentAttachedDirectoriesMapAtom)
  const attachedDirs = attachedDirsMap.get(sessionId) ?? []

  // 附加目录列表（工作区级）
  const wsAttachedDirsMap = useAtomValue(workspaceAttachedDirectoriesMapAtom)
  const setWsAttachedDirsMap = useSetAtom(workspaceAttachedDirectoriesMapAtom)
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedDirsMap.get(currentWorkspaceId) ?? []) : []
```

Replace with:

```typescript
  const filesVersion = useAtomValue(workspaceFilesVersionAtom)
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)

  // Derive current workspace from the session, fall back to the global
  // selection. (slug is removed from AgentWorkspace in Task 8 — we use id
  // throughout now.)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId
```

(Drops `hasFileChanges` (no longer rendered after attached-dirs gone), `workspaceSlug` (dead path), all attached-dirs maps. Keeps `currentWorkspaceId` since it gates the placeholder render.)

- [ ] **Step 7: Drop attached-dirs handlers**

Find these handlers and delete them entirely:

- `attachSessionDir` (lines ~96-106)
- `handleAttachFolder` (lines ~108-115)
- `handleSessionFoldersDropped` (lines ~117-123)
- `handleDetachDirectory` (lines ~125-136)
- `attachWorkspaceDir` (lines ~140-148)
- `handleAttachWorkspaceFolder` (lines ~150-158)
- `handleWorkspaceFoldersDropped` (lines ~160-166)
- `handleDetachWorkspaceDirectory` (lines ~168-180)
- The "工作区级附加目录" loader `useEffect` (lines ~83-94)

Tip: search for `attachDirectory(`, `attachWorkspaceDirectory(`, `detachDirectory(`, `detachWorkspaceDirectory(`, `getWorkspaceDirectories(` in the file — every block referencing them goes.

- [ ] **Step 8: Simplify `handleAddToChat`**

Find (lines ~204-246):

```typescript
  const handleAddToChat = React.useCallback(async (entry: FileEntry) => {
    if (pendingFiles.some((f) => f.sourcePath === entry.path)) return

    let previewUrl: string | undefined
    try {
      const base64 = await readAttachedFile(entry.path, sessionId, workspaceSlug ?? undefined)
      const ext = entry.name.split('.').pop()?.toLowerCase() ?? ''
      const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])
      const mimeExt = ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext
      const mediaType = imageExts.has(ext) ? `image/${mimeExt}` : 'application/octet-stream'

      if (imageExts.has(ext)) {
        const binary = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))
        const blob = new Blob([binary], { type: mediaType })
        previewUrl = URL.createObjectURL(blob)
      }

      const pending: AgentPendingFile = {
        id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
        filename: entry.name,
        mediaType,
        size: Math.round(base64.length * 0.75),
        previewUrl,
        sourcePath: entry.path,
      }

      setPendingFiles((prev) => [...prev, pending])
    } catch (error) {
      if (previewUrl) URL.revokeObjectURL(previewUrl)
      console.error('[SidePanel] 添加文件到聊天失败:', error)
    }
  }, [pendingFiles, setPendingFiles, sessionId, workspaceSlug])
```

Replace with:

```typescript
  const handleAddToChat = React.useCallback((entry: FileEntry) => {
    if (pendingFiles.some((f) => f.sourcePath === entry.path)) return

    // Image preview was driven by readAttachedFile (phantom IPC). Without it
    // we just record the path; the agent input bar / send pipeline reads
    // the file from the path at send time. Phase 2 will restore previews.
    const ext = entry.name.split('.').pop()?.toLowerCase() ?? ''
    const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])
    const mimeExt = ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext
    const mediaType = imageExts.has(ext) ? `image/${mimeExt}` : 'application/octet-stream'

    const pending: AgentPendingFile = {
      id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      filename: entry.name,
      mediaType,
      size: 0, // unknown without readAttachedFile; Phase 2 restores
      previewUrl: undefined,
      sourcePath: entry.path,
    }

    setPendingFiles((prev) => [...prev, pending])
  }, [pendingFiles, setPendingFiles])
```

- [ ] **Step 9: Drop the workspace-files-path useEffect and `workspaceFilesPath` state**

Find (lines ~250-260):

```typescript
  // 工作区文件目录路径
  const [workspaceFilesPath, setWorkspaceFilesPath] = React.useState<string | null>(null)
  React.useEffect(() => {
    if (!workspaceSlug) {
      setWorkspaceFilesPath(null)
      return
    }
    getWorkspaceFilesPath(workspaceSlug).then(setWorkspaceFilesPath).catch(() => setWorkspaceFilesPath(null))
  }, [workspaceSlug])
```

Replace with:

```typescript
  // Workspace files path: derive from the workspace's path column (set when
  // the workspace was created with a path; null otherwise — backend's
  // active_workspace_root falls back to the global workground root).
  const workspaces = useAtomValue(agentWorkspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = workspaces.find((w) => w.id === currentWorkspaceId)
    // AgentWorkspace doesn't carry path; fall back to null. The FileBrowser
    // below only renders when this is non-null. Phase 2 will add path to
    // the AgentWorkspace shape when create_workspace returns full data.
    return ws ? null : null
  }, [workspaces, currentWorkspaceId])
```

NOTE: `AgentWorkspace` doesn't have a `path` field today. We keep this memo as a hook for Phase 2 to extend the type and populate the path from `createWorkspace`'s response. For now the workspace files browser will render only when `workspaceFilesPath` is non-null, which is never — so the workspace files section becomes empty in Phase 1. This is consistent with Phase 1 being "止血": users still see the section header, just no files. **If you want the section header gone too, delete the entire `<div className="flex-1 min-h-0 flex flex-col mx-2 mb-2">` workspace-files block in Step 11.** Pick one — discuss with reviewer.

**Recommendation: keep the section header so users know where workspace files WILL appear in Phase 2. Hide content via the existing `{workspaceFilesPath && ...}` guard.**

- [ ] **Step 10: Update the auto-open effect (drop unused setIsOpen-style fallback)**

The current auto-open effect (lines ~262-272) is fine as-is:

```typescript
  // 自动打开右侧面板：文件变化时把外层 isOpen 设为 true
  const prevFilesVersionRef = React.useRef(filesVersion)
  React.useEffect(() => {
    if (filesVersion > prevFilesVersionRef.current && sessionPath) {
      setSidePanelOpenMap((prev) => {
        const map = new Map(prev)
        map.set(sessionId, true)
        return map
      })
    }
    prevFilesVersionRef.current = filesVersion
  }, [filesVersion, sessionPath, sessionId, setSidePanelOpenMap])
```

**No change needed for this step.** Skip.

- [ ] **Step 11: Trim the JSX render — drop attached-dirs sections and in-Finder buttons**

Find the JSX block from line ~272 (`return (`) through end of main component. Replace with:

```typescript
  return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">
          {/* ===== Session files section (only if sessionPath exists) ===== */}
          {sessionPath && (
            <>
              <div className="flex items-center gap-1 pl-3 pr-2 h-[32px] flex-shrink-0">
                <FolderOpen className="size-3 text-muted-foreground" />
                <span className="text-[11px] font-medium text-muted-foreground">会话文件</span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Info className="size-3 text-muted-foreground/50 cursor-help" />
                  </TooltipTrigger>
                  <TooltipContent side="bottom" className="max-w-[200px]">
                    <p>当前会话的专属文件，仅本次对话的 Agent 可以访问</p>
                  </TooltipContent>
                </Tooltip>
                <span
                  className="text-[10px] text-muted-foreground/75 truncate flex-1"
                  title={sessionPath}
                >
                  {breadcrumb}
                </span>
                <button
                  type="button"
                  onClick={handleRefresh}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="刷新文件列表"
                >
                  <RefreshCw className="size-2.5" />
                </button>
              </div>
              {/* Session files content (independent scroll) */}
              <div className="flex-1 min-h-0 overflow-y-auto">
                <FileBrowser
                  rootPath={sessionPath}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
                <FileDropZone
                  workspaceSlug={null}
                  sessionId={sessionId}
                  target="session"
                  onFilesUploaded={handleFilesUploaded}
                />
              </div>
              {/* ===== Divider ===== */}
              <div className="mx-3 my-3 border-t border-muted-foreground/20" />
            </>
          )}

          {/* ===== Workspace files section ===== */}
          <div className="flex-1 min-h-0 flex flex-col mx-2 mb-2">
            <div className="flex items-center gap-1 px-2 h-[32px] flex-shrink-0">
              <FolderHeart className="size-3 text-muted-foreground" />
              <span className="text-[11px] font-medium text-muted-foreground">工作区文件</span>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Info className="size-3 text-muted-foreground/50 cursor-help" />
                </TooltipTrigger>
                <TooltipContent side="bottom" className="max-w-[220px]">
                  <p>工作区内所有会话可访问的文件和文件夹，每个新对话都可以自动读取</p>
                </TooltipContent>
              </Tooltip>
            </div>
            {/* Workspace files content (independent scroll) */}
            <div className="flex-1 min-h-0 overflow-y-auto pb-1">
              {workspaceFilesPath && (
                <FileBrowser
                  rootPath={workspaceFilesPath}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
              )}
              <FileDropZone
                workspaceSlug={null}
                sessionId={null}
                target="workspace"
                onFilesUploaded={handleFilesUploaded}
              />
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-xs text-muted-foreground">
          请选择工作区
        </div>
      )}
    </div>
  )
}
```

NOTE: the JSX above uses `currentWorkspaceId` (set in Step 6) for the empty-state guard. `workspaceSlug` is no longer derived; the `<FileDropZone workspaceSlug={null}>` calls pass null directly because `FileDropZone` accepts null per its existing prop type — leave as-written.

- [ ] **Step 12: Delete the AttachedDirsSection / AttachedDirTree / AttachedDirItem subtree**

Find everything from `// ===== 附加目录容器（管理选中状态） =====` (around line 476 in the post-Step-11 file) through the end of the file (before any final `// helpers` if present). Delete all of:

- `interface AttachedDirsSectionProps`
- `function AttachedDirsSection`
- `interface AttachedDirTreeProps`
- `function AttachedDirTree`
- `interface AttachedDirItemProps`
- `function AttachedDirItem`

Approximately 400 lines deleted.

- [ ] **Step 13: Verify TS compiles for this file**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep "SidePanel" | head`

Expected: NO errors mentioning SidePanel.tsx (lingering AgentView errors normal — Task 7 fixes them; lingering `slug` errors normal — Task 8 fixes them).

- [ ] **Step 14: Commit**

```bash
git add ui/src/components/agent/SidePanel.tsx
git commit -m "$(cat <<'EOF'
feat(ui): WorkspaceFilesView drops attached-dirs subtree + slug + in-Finder

The SidePanel-formerly-known-as-WorkspaceFilesView is now a thin
two-section file browser (session files + workspace files), backed by
real load_artifact_children. Removed:

- AttachedDirsSection / AttachedDirTree / AttachedDirItem (~400 lines)
- All attach/detach/list/open/rename/move handlers (phantom IPCs)
- workspaceSlug derivation (slug never returned by backend)
- "在 Finder 中打开" buttons (open_file phantom)
- Image preview path in handleAddToChat (read_attached_file phantom);
  pending files now reference paths only

The workspace-files section header still renders so users know where
workspace files WILL appear in Phase 2; the FileBrowser inside
short-circuits because AgentWorkspace currently has no path field
(Phase 2 will populate it from create_workspace's response).

Phase 1 spec §4.4 step 3.
EOF
)"
```

---

### Task 7: AgentView drops `getWorkspaceFilesPath` + `attachDirectory` flows

AgentView is heavier — it both consumes attached-dirs state and produces it via folder-attach UI + drag-drop directory handling.

**Files:**
- Modify: `ui/src/components/agent/AgentView.tsx`

- [ ] **Step 1: Trim imports**

Find (around lines 88-100, the tauri-bridge imports inside AgentView):

```typescript
import {
  ...
  getWorkspaceFilesPath,
  ...
  openFolderDialog,
  attachDirectory,
  ...
} from '@/lib/tauri-bridge'
```

Delete the three lines for `getWorkspaceFilesPath`, `openFolderDialog`, and `attachDirectory` from the import list. Keep all other imports.

- [ ] **Step 2: Drop the attached-dirs atom subscriptions**

Find (lines 272-276):

```typescript
  const setAttachedDirsMap = useSetAtom(agentAttachedDirectoriesMapAtom)
  const attachedDirsMap = useAtomValue(agentAttachedDirectoriesMapAtom)
  const attachedDirs = attachedDirsMap.get(sessionId) ?? []
  const wsAttachedDirsMap = useAtomValue(workspaceAttachedDirectoriesMapAtom)
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedDirsMap.get(currentWorkspaceId) ?? []) : []
```

Replace with:

```typescript
  // Attached directories feature is gone in Phase 1 (phantom backend).
  // These constants keep prop-passing compiling without restructuring the
  // child component APIs. Phase 2 restores the data flow.
  const attachedDirs: string[] = []
  const wsAttachedDirs: string[] = []
```

(Drops both atom subscriptions; preserves the array variable names so downstream JSX/memos don't have to change. The `setAttachedDirsMap` setter is used in handlers we're removing in Step 4-5.)

- [ ] **Step 3: Drop the import of the now-unused atoms**

Find any imports like `agentAttachedDirectoriesMapAtom`, `workspaceAttachedDirectoriesMapAtom` in the AgentView import block (the agent-atoms.ts import). Remove these names from the import list.

```bash
grep -n "agentAttachedDirectoriesMapAtom\|workspaceAttachedDirectoriesMapAtom" ui/src/components/agent/AgentView.tsx
```

Should print zero matches after this step.

- [ ] **Step 4: Drop the workspace-files-path state and effect**

Find (lines 309-310):

```typescript
  const [workspaceFilesPath, setWorkspaceFilesPath] = React.useState<string | null>(null)
```

Delete.

Find (lines 383-393):

```typescript
  // 获取工作区共享文件目录路径（@ 引用时需要搜索）
  const workspaceSlug = workspaces.find((w) => w.id === currentWorkspaceId)?.slug ?? null
  React.useEffect(() => {
    if (!workspaceSlug) {
      setWorkspaceFilesPath(null)
      return
    }
    getWorkspaceFilesPath(workspaceSlug)
      .then(setWorkspaceFilesPath)
      .catch(() => setWorkspaceFilesPath(null))
  }, [workspaceSlug])
```

Replace with:

```typescript
  // Workspace shared files path: not reachable in Phase 1 (getWorkspaceFilesPath
  // is phantom; AgentWorkspace doesn't carry path). Stub to null; Phase 2 will
  // wire this up properly when AgentWorkspace gains a path field.
  const workspaceSlug: string | null = null
  const workspaceFilesPath: string | null = null
```

(Keeps the named bindings so `allAttachedDirs` and the JSX prop pass-throughs still compile. Note `workspaceSlug` was derived from `slug` which Task 8 removes, so we can't just keep the original line.)

- [ ] **Step 5: Update `allAttachedDirs` memo**

Find (lines 395-407):

```typescript
  // 合并工作区文件目录、工作区级附加目录和会话级附加目录，供 @ 引用搜索
  const allAttachedDirs = React.useMemo(() => {
    const dirs = [...attachedDirs]
    for (const d of wsAttachedDirs) {
      if (!dirs.includes(d)) dirs.push(d)
    }
    if (workspaceFilesPath && !dirs.includes(workspaceFilesPath)) {
      dirs.unshift(workspaceFilesPath)
    }
    return dirs
  }, [attachedDirs, wsAttachedDirs, workspaceFilesPath])
```

Leave AS-IS. With `attachedDirs=[]`, `wsAttachedDirs=[]`, and `workspaceFilesPath=null`, this memo always returns `[]`. The downstream consumers receive an empty list — same outcome as Phase 2 with no folders attached. **No change.**

- [ ] **Step 6: Drop `handleAttachFolder`**

Find (lines 646-668):

```typescript
  /** 附加文件夹（不复制，仅记录路径） */
  const handleAttachFolder = React.useCallback(async (): Promise<void> => {
    try {
      const result = await openFolderDialog()
      if (!result) return

      const updated = await attachDirectory({
        sessionId,
        directoryPath: result.path,
      })

      setAttachedDirsMap((prev) => {
        const map = new Map(prev)
        map.set(sessionId, updated)
        return map
      })

      toast.success(`已附加目录: ${result.name}`)
    } catch (error) {
      console.error('[AgentView] 附加文件夹失败:', error)
      toast.error('附加文件夹失败')
    }
  }, [sessionId, setAttachedDirsMap])
```

Delete entirely. Then search for `handleAttachFolder` references — they're typically passed as props to child components. Replace each with `undefined` or remove the prop:

```bash
grep -n "handleAttachFolder" ui/src/components/agent/AgentView.tsx
```

For each match outside the deleted block, examine the call site. If it's a JSX prop like `onAttachFolder={handleAttachFolder}`, delete the prop. If the child component requires the prop, set it to `undefined`. (Realistically there should only be 0-2 sites.)

- [ ] **Step 7: Trim the directory branch from `handleDrop`**

Find the `handleDrop` callback (lines 700-770ish). Within it, find the directory-handling block:

```typescript
        // 拖拽的文件夹直接附加
        for (const dirPath of directories) {
          try {
            const updated = await attachDirectory({
              sessionId,
              directoryPath: dirPath,
            })
            setAttachedDirsMap((prev) => {
              const map = new Map(prev)
              map.set(sessionId, updated)
              return map
            })
            const dirName = dirPath.split('/').pop() || dirPath
            toast.success(`已附加目录: ${dirName}`)
          } catch (error) {
            console.error('[AgentView] 拖拽附加文件夹失败:', error)
          }
```

Replace with:

```typescript
        // Phase 1: dropping folders is a no-op until Phase 2 implements
        // attach_directory. Show a toast so users aren't confused.
        for (const dirPath of directories) {
          const dirName = dirPath.split('/').pop() || dirPath
          toast.message(`Folder drag is disabled in Phase 1: ${dirName}`)
        }
```

(Preserves the loop shape so the surrounding closure structure doesn't change.)

- [ ] **Step 8: Drop `additionalDirectories` from agent-message payload**

Find (line ~966):

```typescript
      ...(attachedDirs.length > 0 && { additionalDirectories: attachedDirs }),
```

Delete this line entirely. (With `attachedDirs=[]` it would never fire anyway, but explicit deletion is clearer.)

- [ ] **Step 9: Update useEffect dep arrays / useCallback dep arrays**

Search for any `useCallback` or `useEffect` dep array that mentions deleted bindings (`setAttachedDirsMap`, `setWorkspaceFilesPath`, the now-stub `workspaceSlug`):

```bash
grep -n "setAttachedDirsMap\|setWorkspaceFilesPath\|workspaceSlug" ui/src/components/agent/AgentView.tsx
```

For each match: examine context. If it's a dep array of a hook, drop the now-deleted name (or replace with the constant). If it's a JSX prop like `workspaceSlug={workspaceSlug}` passed to a child, leave it (the variable still exists, just always null).

- [ ] **Step 10: Verify TS compiles for this file**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep "AgentView" | head`

Expected: NO errors mentioning AgentView.tsx (lingering `slug` errors elsewhere are still fine — Task 8 fixes them).

- [ ] **Step 11: Commit**

```bash
git add ui/src/components/agent/AgentView.tsx
git commit -m "$(cat <<'EOF'
feat(ui): AgentView drops getWorkspaceFilesPath + attachDirectory flows

Folder-attach UX is gone in Phase 1 (the IPC commands were phantom). The
component still receives the same prop names (attachedDirs,
allAttachedDirs, workspaceSlug) but they're stubbed to []/null so child
APIs don't churn. The agent-message payload no longer ships
additionalDirectories.

Folder drops show a toast explaining the feature is disabled in Phase 1
rather than silently failing. Phase 2 restores the wiring.

Phase 1 spec §4.4 step 4.
EOF
)"
```

---

### Task 8: Type & atom cleanup — drop `slug`, drop dead atoms

The cleanup commit. After this, build is green again.

**Files:**
- Modify: `ui/src/lib/agent-types.ts`
- Modify: `ui/src/atoms/agent-atoms.ts`

- [ ] **Step 1: Drop `slug` from `AgentWorkspace`**

Find (lines 91-98 in `agent-types.ts`):

```typescript
/** Agent 工作区 */
export interface AgentWorkspace {
  id: string
  name: string
  slug: string
  createdAt: number
  updatedAt: number
}
```

Replace with:

```typescript
/** Agent 工作区 */
export interface AgentWorkspace {
  id: string
  name: string
  createdAt: number
  updatedAt: number
}
```

- [ ] **Step 2: Delete dead atoms**

Find (lines 748-749 in `agent-atoms.ts`):

```typescript
export const agentAttachedDirectoriesMapAtom = atom<Map<string, string[]>>(new Map())
export const workspaceAttachedDirectoriesMapAtom = atom<Map<string, string[]>>(new Map())
```

Delete both lines.

- [ ] **Step 3: Verify no remaining references**

```bash
grep -rn "agentAttachedDirectoriesMapAtom\|workspaceAttachedDirectoriesMapAtom\|\.slug\b" ui/src 2>/dev/null | grep -v "node_modules"
```

Expected output: NO matches for the two atom names. `\.slug\b` may still match unrelated code (e.g. icon `slug`); examine each hit. If any hit is on `AgentWorkspace`-typed objects, fix the call site.

Likely candidates to verify:
- `useCloseTab.ts` — search for `agentAttachedDirectoriesMapAtom` usage:
  ```bash
  grep -n "agentAttachedDirectoriesMapAtom\|workspaceAttachedDirectoriesMapAtom" ui/src/hooks/useCloseTab.ts
  ```
  If found: this hook clears the atom on tab close — since the atom is gone, delete the lines that reference it.

- `LeftSidebar.tsx` — same check:
  ```bash
  grep -n "agentAttachedDirectoriesMapAtom\|workspaceAttachedDirectoriesMapAtom" ui/src/components/app-shell/LeftSidebar.tsx
  ```

For each remaining ref: open the file, locate the import + setter usage, delete both.

- [ ] **Step 4: Verify full TypeScript build**

Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`

Expected: zero errors. (`Cannot find name` errors should all be resolved; `slug does not exist on type AgentWorkspace` should all be resolved.)

If errors remain: read each error and fix the offending file. The most likely remaining error sites are unimported atom names in files we didn't touch — handle them as encountered.

- [ ] **Step 5: Verify backend still builds**

Run: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head`

Expected: empty. (Sanity check — Task 8 is TS-only but a clean Rust build at the end of Phase 1 is the contract.)

- [ ] **Step 6: Run all tests**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -15
```

Expected: all tests pass, including the 9 new ones (2 from Task 1 + 7 from Task 3).

```bash
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: pre-existing pass count, no new failures from this PR. (The `message.test.tsx` failure noted in CLAUDE.md is pre-existing and unrelated — leave it alone.)

- [ ] **Step 7: Commit**

```bash
git add ui/src/lib/agent-types.ts ui/src/atoms/agent-atoms.ts ui/src/hooks/useCloseTab.ts ui/src/components/app-shell/LeftSidebar.tsx
# (add only the files actually modified — `git status` shows the truth)
git commit -m "$(cat <<'EOF'
chore(types): drop AgentWorkspace.slug + dead attached-dirs atoms

Final Phase 1 commit — restores green build. Removes:

- AgentWorkspace.slug (never populated by backend; consumers all switched
  to id in earlier commits)
- agentAttachedDirectoriesMapAtom (session-level attached dirs)
- workspaceAttachedDirectoriesMapAtom (workspace-level attached dirs)
- All remaining importers/clearers in useCloseTab and LeftSidebar

Phase 1 complete. The build is green, 9 new Rust tests pass, no UI
regressions in features that previously worked.

Phase 1 spec §4.4 steps 5-6.
EOF
)"
```

---

## After all 8 commits

- [ ] **Sanity smoke (manual, on a copy of your dev DB)**

The migration runs against `~/.uclaw/uclaw.db` on next launch. Before merging:

1. Make a backup: `cp ~/.uclaw/uclaw.db ~/.uclaw/uclaw.db.pre-phase1.bak`
2. Run `cargo tauri dev` (or `cargo build --release && open target/release/bundle/macos/uClaw.app`)
3. Verify:
   - WorkspaceSelector shows `默认工作区` with 📁 icon at the bottom (or wherever your existing workspaces sort)
   - Create a new workspace; restart app; verify it persists
   - Select an existing workspace, create an agent session in it
   - Delete that workspace from WorkspaceSelector
   - Re-open the agent session — it should appear in `默认工作区`'s session list (via re-home)
   - Try to delete `默认工作区` — should fail; UI should toast the error
   - Open right panel Files tab — only `会话文件` and `工作区文件` headers; no `附加目录`; no Finder-open / rename / move buttons
   - The previous "drag a folder onto the agent input to attach it" gesture now toasts "Folder drag is disabled in Phase 1"

If any item fails: it's a real bug in this PR. Fix before merging.

- [ ] **Push the branch**

```bash
git push -u origin claude/workspace-phase1
```

- [ ] **Open a PR with the bisectable commits table**

Per CLAUDE.md PR pattern (PRs #29/31/33/35/36):

```bash
gh pr create --title "feat(workspace): Phase 1 — stop the bleeding" --body "$(cat <<'EOF'
## Summary

Phase 1 of the workspace remediation series. Spec:
[`docs/superpowers/specs/2026-05-11-workspace-phase1-design.md`](docs/superpowers/specs/2026-05-11-workspace-phase1-design.md)

Workspace was half-implemented: 17 frontend IPC wrappers invoked Tauri
commands that don't exist in the backend; some swallowed failures via
`.catch(...)` returning fake stubs. This PR makes the surface honest:

- V16 migration persists `default` workspace + heals existing orphan
  agent_sessions
- Application-layer integrity in `create_agent_session` (tolerant fallback),
  `move_agent_session_to_workspace` (strict err), `delete_workspace`
  (re-home agent_sessions before delete; refuse 'default')
- Frontend deletes 17 phantom wrappers + their dependent UI:
  WorkspaceSelector loses rename + drag-reorder, WorkspaceFilesView loses
  attached-dirs subtree, AgentView loses folder-attach flow

Phase 2 restores these features behind real backends.

## Commits (bisectable)

| # | Commit | Layer |
|---|---|---|
| 1 | `chore(db): V16 migration — persist 'default' workspace + heal orphans` | Rust + 2 tests |
| 2 | `refactor(workspace): drop synthetic-default branch from list_spaces` | Rust |
| 3 | `feat(workspace): IPC validation + delete_workspace re-homes sessions` | Rust + 7 tests |
| 4 | `chore(ui): delete 17 phantom IPC wrappers from tauri-bridge` | TS — intentionally red |
| 5 | `feat(ui): WorkspaceSelector uses real create/delete IPC, drops rename+reorder` | TS |
| 6 | `feat(ui): WorkspaceFilesView drops attached-dirs subtree + slug + in-Finder` | TS |
| 7 | `feat(ui): AgentView drops getWorkspaceFilesPath + attachDirectory flows` | TS |
| 8 | `chore(types): drop AgentWorkspace.slug + dead attached-dirs atoms` | TS — restores green |

Build is intentionally red between commits 4-7 (front-end wrappers gone but
call sites not yet repaired); commit 8 closes the loop.

## What users will lose (until Phase 2)

| Feature | Restored in |
|---|---|
| Workspace rename | Phase 2 |
| Workspace drag-reorder | Phase 2 |
| Workspace-level attached directories | Phase 2 |
| Session-level attached directories | Phase 2 |
| File-tree "open in Finder / rename / move" | Phase 2 |
| Image preview thumbnail when adding a file from Files panel to chat | Phase 2 |

These features did not actually work before this PR (silent failures, dead
paths). User-perceived regression is small.

## Test plan

- [ ] `cargo test --lib` (9 new tests pass; full suite green)
- [ ] `cd ui && npx tsc --noEmit` (clean)
- [ ] `cd ui && npm test -- --run` (no new failures vs main)
- [ ] Manual smoke per spec §5.3 on a copy of `~/.uclaw/uclaw.db` (see plan)
EOF
)"
```

---

## Spec coverage

For self-review at the end of plan writing:

- §4.1 (V16 migration): Task 1
- §4.2 (remove synthetic default): Task 2
- §4.3 (validation + delete-cascade): Task 3
- §4.4 step 1 (delete phantom wrappers): Task 4
- §4.4 step 2 (WorkspaceSelector): Task 5
- §4.4 step 3 (WorkspaceFilesView/SidePanel): Task 6
- §4.4 step 4 (AgentView): Task 7
- §4.4 step 5 (atoms): Task 8 step 2
- §4.4 step 6 (slug from AgentWorkspace): Task 8 step 1
- §5.1 tests 1-2 (V16 migration tests): Task 1
- §5.1 tests 3-6 (helpers): Task 3 (covers the spec's listed 4 tests + 3 additional helper-level tests)
- §5.1 — note: spec §5.1 lists 6 tests including `delete_workspace_reassigns_agent_sessions_to_default` and `delete_workspace_refuses_default`. Plan Task 3 tests are at the helper level (not the Tauri command level). The `rehome_agent_sessions_moves_them_to_default` test covers the cascade behavior; the spec's `delete_workspace_refuses_default` would require an AppState — defer to manual smoke (Task 5.3 in this plan covers it).
- §5.2 test 7 (TS test): NOT in plan — optional per spec, skip in Phase 1 to keep scope tight.
- §5.3 (manual smoke): "After all 8 commits" section.
- §6 (risks): captured in PR description "What users will lose" table.
- §7 (PR shape, 8 commits): exactly mirrored as Tasks 1-8.
