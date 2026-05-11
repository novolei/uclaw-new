# Workspace Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [`docs/superpowers/specs/2026-05-11-workspace-phase2-design.md`](../specs/2026-05-11-workspace-phase2-design.md)

**Goal:** Restore six features Phase 1 deleted (workspace rename, drag-reorder, workspace + session attached directories, file actions, image previews) behind real persisting backends; close the 3 architectural findings from Phase 1 smoke testing (dead `WorkspaceSelector`, orphan `MoveSessionDialog`, missing `WorkspaceRail`/`WorkspaceGroup` affordances).

**Architecture:** V17 migration adds two columns to `spaces` (`sort_order INTEGER`, `attached_dirs TEXT JSON`) and one to `agent_sessions` (`attached_dirs TEXT JSON`). Workspace mutations (rename / reorder / attach-dir / detach-dir) become real Tauri commands; helpers from Phase 1 (`require_workspace_exists`) are reused. File system actions (open in Finder, folder picker) go through `tauri-plugin-dialog` and `tauri-plugin-shell` v2. Image previews bypass base64 via Tauri's `convertFileSrc` asset protocol. Build is green at every commit — unlike Phase 1's intentionally-red commits 6-9.

**Tech Stack:** Rust + rusqlite + Tauri 2 + React 18 + Jotai + `tauri-plugin-dialog`/`-shell` v2 (new). No other new deps.

---

## File Structure

**Modify (Rust):**
- `src-tauri/src/db/migrations.rs` — add V17 const + run-block + 4 tests (Task 1)
- `src-tauri/src/tauri_commands.rs` — add 9 new commands across Tasks 2-8; add module-private helpers; extend `list_agent_sessions` / `list_spaces` to surface new columns
- `src-tauri/src/ipc.rs` — extend `SpaceResponse` struct with `path`, `attached_dirs`, `sort_order` fields (Task 1 — required for V17 reads)
- `src-tauri/src/main.rs` — register new IPC commands in `invoke_handler!`; init two new plugins (Task 7)
- `src-tauri/Cargo.toml` — add `tauri-plugin-dialog` + `tauri-plugin-shell` (Task 7)
- `src-tauri/capabilities/default.json` — grant dialog + shell permissions (Task 7)

**Modify (TS):**
- `ui/src/lib/agent-types.ts` — extend `AgentWorkspace` with `icon`, `path`, `attachedDirs?`, `sortOrder?` (Task 1)
- `ui/src/atoms/workspace.ts` — extend `WorkspaceInfo` with `attachedDirs?`, `sortOrder?`; add session attached-dirs map atom (Task 6)
- `ui/src/atoms/agent-atoms.ts` — add new attached-dirs atoms `workspaceAttachedDirsMapAtom` / `agentSessionAttachedDirsMapAtom` (Task 5/6)
- `ui/src/lib/tauri-bridge.ts` — add 9 new IPC wrappers (Tasks 2-8); replace the `openFolderDialog` / `openExternal` / `openFile` wrappers to use plugins (Task 7)
- `ui/src/components/workspace/WorkspaceGroup.tsx` — hover-buttons (Pencil + Trash), inline rename, drag-reorder, drop indicators, session three-dot menu (Tasks 9 + 10)
- `ui/src/components/workspace/SessionItem.tsx` — replace the simple `×` with a three-dot DropdownMenu (delete + move-to-workspace + archive) (Task 10)
- `ui/src/components/workspace/WorkspaceRail.tsx` — hoist MoveSessionDialog state + render the dialog once (Task 10)
- `ui/src/components/agent/MoveSessionDialog.tsx` — wire to real call sites; cosmetic adjustments to props if needed (Task 10)
- `ui/src/components/agent/SidePanel.tsx` (`WorkspaceFilesView`) — restore attached-dirs sections, add open-in-Finder buttons, convertFileSrc previews (Task 11)
- `ui/src/components/agent/AgentView.tsx` — replace Phase 1 stubs (`attachedDirs: string[] = []`) with real atom subscriptions; folder drop calls `attach_session_directory` (Task 12)
- `ui/src/components/app-shell/LeftSidebar.tsx` — delete `<WorkspaceSelector />` mount at line ~660 (Task 13)

**Delete:**
- `ui/src/components/agent/WorkspaceSelector.tsx` — dead UI, atom never populated (Task 13)

**No file creations** (all new logic lives in existing modules).

---

## Conventions for this plan

- Run from repo root `/Users/ryanliu/Documents/uclaw` unless noted.
- Branch is already `claude/workspace-phase2` (created off main `2f9008e`; spec committed at `8ac4f6b`).
- Each task ends with a commit. Commit messages are pre-written — copy verbatim.
- After each commit: `cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head` should be empty. For TS-only tasks: `cd ui && npx tsc --noEmit 2>&1 | head -10` clean.
- Build stays green at every commit (unlike Phase 1). If a task can't complete without breaking the build, **stop and escalate** rather than push a broken state.

---

### Task 1: V17 migration — sort_order + attached_dirs columns + backfill

Spec §4.1. Adds two columns to `spaces` and one to `agent_sessions`. Backfills `sort_order` from `created_at DESC` so newest workspace = 0. Idempotent via per-statement `tracing::warn!` skip (SQLite returns "duplicate column" on re-run).

Also extends `SpaceResponse` (`ipc.rs`) and `AgentWorkspace` (`agent-types.ts`) types with the new fields, and updates `list_spaces` to read them (returns NULL/`'[]'`/`0` for fresh DBs and real values post-V17).

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (append V17 const around line 635; append run-block around line 740; extend the test mod at end of file)
- Modify: `src-tauri/src/ipc.rs:162-168` (extend `SpaceResponse`)
- Modify: `src-tauri/src/tauri_commands.rs::list_spaces` (read new columns)
- Modify: `ui/src/lib/agent-types.ts:91-98` (extend `AgentWorkspace`)
- Modify: `ui/src/atoms/workspace.ts:4-11` (extend `WorkspaceInfo`)

- [ ] **Step 1: Write the failing migration tests**

Append to the existing `#[cfg(test)] mod tests` block in `src-tauri/src/db/migrations.rs` (at end of file). The helpers `db_pre_v16` and `run_v16` already exist from Phase 1 — extend with V17 equivalents:

```rust
    /// Apply migrations through V16 so V17 has a populated schema to extend.
    fn db_pre_v17() -> Connection {
        let conn = db_pre_v16();
        // V16 needs to run first so 'default' exists, otherwise the
        // V17 backfill counts can be confused by data that V16 would touch.
        run_v16(&conn);
        conn
    }

    fn run_v17(conn: &Connection) {
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v17_adds_sort_order_column_idempotent() {
        let conn = db_pre_v17();
        run_v17(&conn);
        // Column exists and default is 0 for backfilled rows.
        let mut stmt = conn.prepare("SELECT sort_order FROM spaces WHERE id = 'default'").unwrap();
        let val: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        // 'default' is the only workspace at this point (V16 inserted it), so sort_order = 0.
        assert_eq!(val, 0, "default workspace should be at sort_order 0 (only workspace)");

        // Second run must not error or change values.
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            // Some ALTERs fail with "duplicate column" — that's expected, swallow.
            let _ = conn.execute(stmt, []);
        }
        let val2: i64 = conn.query_row("SELECT sort_order FROM spaces WHERE id = 'default'", [], |r| r.get(0)).unwrap();
        assert_eq!(val2, 0, "sort_order must remain 0 after re-run");
    }

    #[test]
    fn v17_adds_workspace_attached_dirs_column() {
        let conn = db_pre_v17();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM spaces WHERE id = 'default'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh workspace should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_adds_session_attached_dirs_column() {
        let conn = db_pre_v17();
        // Insert a session so we have something to query.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-1', 'default', 'test', 0, 0)",
            [],
        ).unwrap();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = 's-1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh session should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_backfills_sort_order_from_created_at() {
        let conn = db_pre_v17();
        // Insert 2 more workspaces with controlled created_at timestamps.
        // Order intent: newest (B) should get sort_order=0, then 'default', then oldest (A).
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-a', 'A', '📁', NULL, '2026-05-01 00:00:00', '2026-05-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-b', 'B', '📁', NULL, '2026-05-11 00:00:00', '2026-05-11 00:00:00')",
            [],
        ).unwrap();
        run_v17(&conn);

        let mut stmt = conn.prepare(
            "SELECT id, sort_order FROM spaces ORDER BY sort_order ASC"
        ).unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        // ws-b is newest (2026-05-11), then default (datetime('now') ~ 2026-05-11), then ws-a (2026-05-01).
        // sort_order = COUNT(rows with created_at > THIS.created_at).
        // ws-b: 0 rows newer → 0; default: 0-1 rows newer (depending on if datetime('now') > ws-b) → 0 or 1; ws-a: 2 rows newer → 2.
        // Assertions tolerate the default-row tie-with-ws-b:
        let ws_b_order = rows.iter().find(|(id, _)| id == "ws-b").map(|(_, o)| *o).unwrap();
        let ws_a_order = rows.iter().find(|(id, _)| id == "ws-a").map(|(_, o)| *o).unwrap();
        assert_eq!(ws_b_order, 0, "newest workspace ws-b should have sort_order 0");
        assert_eq!(ws_a_order, 2, "oldest workspace ws-a should have sort_order 2");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib v17 2>&1 | tail -15`

Expected: compile error — `V17_WORKSPACE_PATH_SORT_ATTACHED` not defined.

- [ ] **Step 3: Add the V17 const**

In `src-tauri/src/db/migrations.rs`, insert immediately after the `V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL` declaration (around line 632):

```rust

/// V17: per-workspace + per-session attached directory lists (JSON columns),
/// workspace sort ordering (integer column), and a one-time backfill that
/// derives sort_order from created_at descending so the existing newest-first
/// order is preserved after the schema change.
///
/// All three ALTERs may fail on re-run with "duplicate column" — handled by
/// the per-statement tracing::warn! skip in run(), matching V9/V10 idiom.
pub const V17_WORKSPACE_PATH_SORT_ATTACHED: &str = "
ALTER TABLE spaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE spaces ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent_sessions ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';

UPDATE spaces SET sort_order = (
    SELECT COUNT(*) FROM spaces s2 WHERE s2.created_at > spaces.created_at
);
";
```

- [ ] **Step 4: Wire V17 into `run()`**

Inside `pub fn run(...)`, AFTER the V16 block (just before `tracing::info!("Database migrations complete")` around line 745), add:

```rust
    // V17: workspace sort + attached directories columns + backfill.
    tracing::debug!("Running migration V17: workspace path/sort/attached_dirs");
    for stmt in V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V17 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib v17 2>&1 | tail -10`

Expected:
```
running 4 tests
test db::migrations::tests::v17_adds_session_attached_dirs_column ... ok
test db::migrations::tests::v17_adds_sort_order_column_idempotent ... ok
test db::migrations::tests::v17_adds_workspace_attached_dirs_column ... ok
test db::migrations::tests::v17_backfills_sort_order_from_created_at ... ok

test result: ok. 4 passed; 0 failed; ...
```

- [ ] **Step 6: Extend `SpaceResponse` struct**

In `src-tauri/src/ipc.rs`, find the `SpaceResponse` struct (around line 162):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub created_at: String,
    pub updated_at: String,
}
```

Replace with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub path: Option<String>,
    pub attached_dirs: Vec<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 7: Update `list_spaces` to read new columns**

In `src-tauri/src/tauri_commands.rs`, replace the `list_spaces` function body (currently lines 933-953) with:

```rust
pub async fn list_spaces(state: State<'_, AppState>) -> Result<Vec<SpaceResponse>, Error> {
    let db = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

    let mut stmt = db.prepare(
        "SELECT id, name, icon, path, attached_dirs, sort_order, created_at, updated_at
         FROM spaces ORDER BY sort_order ASC"
    ).map_err(Error::Database)?;

    let spaces: Vec<SpaceResponse> = stmt.query_map([], |row| {
        let attached_dirs_json: String = row.get::<_, String>(4).unwrap_or_else(|_| "[]".into());
        let attached_dirs: Vec<String> = serde_json::from_str(&attached_dirs_json).unwrap_or_default();
        Ok(SpaceResponse {
            id: row.get(0)?,
            name: row.get(1)?,
            icon: row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
            path: row.get(3).ok(),
            attached_dirs,
            sort_order: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }).map_err(Error::Database)?
    .filter_map(|r| r.ok())
    .collect();

    Ok(spaces)
}
```

(ORDER BY changes from `created_at DESC` to `sort_order ASC`.)

- [ ] **Step 8: Extend frontend types**

In `ui/src/lib/agent-types.ts`, find the `AgentWorkspace` interface (around line 91):

```typescript
export interface AgentWorkspace {
  id: string
  name: string
  createdAt: number
  updatedAt: number
}
```

Replace with:

```typescript
export interface AgentWorkspace {
  id: string
  name: string
  icon: string
  path: string | null
  attachedDirs?: string[]
  sortOrder?: number
  createdAt: number
  updatedAt: number
}
```

In `ui/src/atoms/workspace.ts`, find `WorkspaceInfo` (around line 4):

```typescript
export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
  createdAt: string
  updatedAt: string
}
```

Replace with:

```typescript
export interface WorkspaceInfo {
  id: string
  name: string
  icon: string
  path: string | null
  attachedDirs: string[]
  sortOrder: number
  createdAt: string
  updatedAt: string
}
```

- [ ] **Step 9: Verify TS + cargo build green**

Run:
```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Both should be empty (no errors).

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs ui/src/lib/agent-types.ts ui/src/atoms/workspace.ts
git commit -m "$(cat <<'EOF'
chore(db): V17 migration — sort_order + attached_dirs + backfill

Adds two columns to spaces (sort_order INTEGER, attached_dirs TEXT JSON
'[]') and one to agent_sessions (attached_dirs TEXT JSON '[]'), plus a
one-time UPDATE that backfills sort_order from created_at descending so
the existing newest-first list order is preserved after switching
list_spaces from ORDER BY created_at DESC to ORDER BY sort_order ASC.

Type extensions:
- SpaceResponse (ipc.rs) gains path, attached_dirs, sort_order
- AgentWorkspace (agent-types.ts) gains icon, path, attachedDirs?, sortOrder?
- WorkspaceInfo (atoms/workspace.ts) gains attachedDirs, sortOrder

Inline tests (4):
- v17_adds_sort_order_column_idempotent
- v17_adds_workspace_attached_dirs_column
- v17_adds_session_attached_dirs_column
- v17_backfills_sort_order_from_created_at

No FK additions — Phase 1 §3 non-goal stands; application-layer cascade
in delete_workspace (Phase 1 §4.3) remains the only defense.

Phase 2 spec §4.1.
EOF
)"
```

---

### Task 2: `update_workspace` IPC + tests

Spec §4.2. New command for workspace rename + icon change. Refuses to rename `'default'` (sentinel protection) but allows icon change.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add command near other workspace commands, around line 4438 just before `create_workspace`; extend `mod workspace_integrity_tests`)
- Modify: `src-tauri/src/main.rs` (register `update_workspace` in `invoke_handler!`)
- Modify: `ui/src/lib/tauri-bridge.ts` (add wrapper)

- [ ] **Step 1: Write failing tests**

Append to `#[cfg(test)] mod workspace_integrity_tests` in `tauri_commands.rs` (at the end, after existing tests):

```rust

    // ─── update_workspace ──────────────────────────────────────────────

    fn read_workspace_name(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT name FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        ).unwrap()
    }

    fn read_workspace_icon(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT icon FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn update_workspace_changes_name() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-real", "Original");
        super::do_update_workspace(&conn, "ws-real", Some("Renamed".into()), None).unwrap();
        assert_eq!(read_workspace_name(&conn, "ws-real"), "Renamed");
    }

    #[test]
    fn update_workspace_refuses_to_rename_default() {
        let conn = fresh_db();
        let r = super::do_update_workspace(&conn, "default", Some("NotDefault".into()), None);
        assert!(r.is_err(), "renaming 'default' must return Err");
        // Name unchanged.
        assert_eq!(read_workspace_name(&conn, "default"), "默认工作区");
    }

    #[test]
    fn update_workspace_allows_icon_change_on_default() {
        let conn = fresh_db();
        super::do_update_workspace(&conn, "default", None, Some("🌟".into())).unwrap();
        assert_eq!(read_workspace_icon(&conn, "default"), "🌟");
    }
```

Note: tests call `do_update_workspace` — a `pub(crate)` helper that wraps the SQL so the Tauri command itself stays thin. The Tauri command just acquires the lock and delegates.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib workspace_integrity 2>&1 | tail -15`

Expected: compile error — `do_update_workspace` not found.

- [ ] **Step 3: Add helper + Tauri command**

In `src-tauri/src/tauri_commands.rs`, insert immediately AFTER the existing `rehome_agent_sessions_to_default` function (the last Phase 1 helper, around line 4475 — search for `pub(crate) fn rehome_agent_sessions_to_default`):

```rust

/// Apply name and/or icon updates to a workspace. Refuses to rename
/// 'default' (sentinel protection) but allows icon changes on it.
/// Extracted from `update_workspace` so it's unit-testable without AppState.
pub(crate) fn do_update_workspace(
    conn: &rusqlite::Connection,
    id: &str,
    name: Option<String>,
    icon: Option<String>,
) -> Result<(), Error> {
    if id == "default" && name.is_some() {
        return Err(Error::Internal(
            "cannot rename the 'default' workspace".into(),
        ));
    }
    require_workspace_exists(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(n) = name.as_ref() {
        conn.execute(
            "UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![n, &now, id],
        ).map_err(Error::Database)?;
    }
    if let Some(i) = icon.as_ref() {
        conn.execute(
            "UPDATE spaces SET icon = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![i, &now, id],
        ).map_err(Error::Database)?;
    }
    Ok(())
}
```

Then add the Tauri command (insert AFTER the existing `create_workspace` function, around line 4520 — search for `pub async fn create_workspace`):

```rust

#[tauri::command]
pub async fn update_workspace(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    icon: Option<String>,
) -> Result<serde_json::Value, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    do_update_workspace(&conn, &id, name, icon)?;
    // Return the updated row.
    let (id, name, icon, path, sort_order, created_at, updated_at): (String, String, String, Option<String>, i64, String, String) =
        conn.query_row(
            "SELECT id, name, icon, path, sort_order, created_at, updated_at FROM spaces WHERE id = ?1",
            rusqlite::params![&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        ).map_err(Error::Database)?;
    Ok(serde_json::json!({
        "id": id,
        "name": name,
        "icon": icon,
        "path": path,
        "sortOrder": sort_order,
        "createdAt": created_at,
        "updatedAt": updated_at,
    }))
}
```

- [ ] **Step 4: Register in `invoke_handler!`**

In `src-tauri/src/main.rs`, find the existing line `uclaw_core::tauri_commands::create_workspace,` (around line 501). Insert immediately after it:

```rust
            uclaw_core::tauri_commands::update_workspace,
```

- [ ] **Step 5: Add frontend wrapper**

In `ui/src/lib/tauri-bridge.ts`, find the `deleteWorkspace` wrapper (around line 232). Insert immediately after it:

```typescript

export const updateWorkspace = (input: { id: string; name?: string; icon?: string }): Promise<{
  id: string; name: string; icon: string; path: string | null; sortOrder: number; createdAt: string; updatedAt: string
}> => invoke('update_workspace', input)
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib workspace_integrity 2>&1 | tail -10`

Expected: 10 tests pass (7 from Phase 1 + 3 new).

- [ ] **Step 7: Verify build clean**

Run:
```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Both empty.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(workspace): update_workspace IPC for rename + icon change

New Tauri command + extracted `do_update_workspace` helper. The helper
refuses to rename 'default' (sentinel protection per spec §4.2) but
allows icon changes on it — both because there's no good reason to lock
the icon and to align with the rest of the codebase's "default is
deletable-protected, otherwise just like any workspace" stance.

Inline tests (3):
- update_workspace_changes_name
- update_workspace_refuses_to_rename_default
- update_workspace_allows_icon_change_on_default

Phase 2 spec §4.2.
EOF
)"
```

---

### Task 3: `reorder_workspaces` IPC + list_spaces ORDER BY sort_order

Spec §4.2. New command that writes `sort_order = idx` for each id in the supplied array. Uses a transaction so partial reorders don't leave the DB inconsistent. List_spaces was already changed to ORDER BY sort_order ASC in Task 1.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (add command + helper + tests)
- Modify: `src-tauri/src/main.rs` (register)
- Modify: `ui/src/lib/tauri-bridge.ts` (add wrapper)

- [ ] **Step 1: Write failing tests**

Append to `mod workspace_integrity_tests`:

```rust

    // ─── reorder_workspaces ──────────────────────────────────────────────

    fn read_sort_order(conn: &Connection, id: &str) -> i64 {
        conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        ).unwrap()
    }

    #[test]
    fn reorder_workspaces_sets_sort_order_by_array_index() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-a", "A");
        insert_workspace(&conn, "ws-b", "B");
        insert_workspace(&conn, "ws-c", "C");
        // Reorder: c first, then a, then b (default kept at 'default' but not in this list).
        super::do_reorder_workspaces(&conn, &["ws-c".into(), "ws-a".into(), "ws-b".into()]).unwrap();
        assert_eq!(read_sort_order(&conn, "ws-c"), 0);
        assert_eq!(read_sort_order(&conn, "ws-a"), 1);
        assert_eq!(read_sort_order(&conn, "ws-b"), 2);
    }

    #[test]
    fn reorder_workspaces_errors_on_unknown_id_no_partial_writes() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-a", "A");
        let before = read_sort_order(&conn, "ws-a");
        let result = super::do_reorder_workspaces(&conn, &["ws-a".into(), "ghost".into()]);
        assert!(result.is_err(), "unknown id must error");
        // ws-a should be unchanged because the tx rolled back.
        assert_eq!(read_sort_order(&conn, "ws-a"), before);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

`cd src-tauri && cargo test --lib reorder 2>&1 | tail -10` — expect "not found".

- [ ] **Step 3: Add helper + Tauri command**

In `tauri_commands.rs`, insert immediately AFTER `do_update_workspace` (added in Task 2):

```rust

/// Apply `sort_order = idx` for each workspace id in the supplied ordered
/// list. Wraps in a transaction so partial reorders don't leave the DB
/// inconsistent if a later id is invalid. Validates each id exists first.
pub(crate) fn do_reorder_workspaces(
    conn: &rusqlite::Connection,
    ordered_ids: &[String],
) -> Result<(), Error> {
    // Validate everything up front so we error before any write.
    for id in ordered_ids {
        require_workspace_exists(conn, id)?;
    }
    let tx = conn.unchecked_transaction().map_err(Error::Database)?;
    for (idx, id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE spaces SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![idx as i64, id],
        ).map_err(Error::Database)?;
    }
    tx.commit().map_err(Error::Database)?;
    Ok(())
}
```

Add the Tauri command near `update_workspace` (after the closing brace of `update_workspace`):

```rust

#[tauri::command]
pub async fn reorder_workspaces(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    do_reorder_workspaces(&conn, &ordered_ids)
}
```

- [ ] **Step 4: Register in `invoke_handler!`**

In `main.rs`, insert immediately after `update_workspace,`:

```rust
            uclaw_core::tauri_commands::reorder_workspaces,
```

- [ ] **Step 5: Add frontend wrapper**

In `tauri-bridge.ts`, immediately after `updateWorkspace` (added in Task 2):

```typescript

export const reorderWorkspaces = (orderedIds: string[]): Promise<void> =>
  invoke('reorder_workspaces', { orderedIds })
```

- [ ] **Step 6: Run tests to verify they pass**

`cd src-tauri && cargo test --lib reorder 2>&1 | tail -10` — expect 2 pass.

- [ ] **Step 7: Build clean**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Both empty.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(workspace): reorder_workspaces IPC + list_spaces sort by sort_order

New Tauri command writes sort_order = idx for each id in the ordered
array, wrapped in a transaction so partial reorders can't leave the DB
inconsistent if a later id is invalid. Validates all ids exist first.

list_spaces (Task 1) already switched to ORDER BY sort_order ASC.

Inline tests (2):
- reorder_workspaces_sets_sort_order_by_array_index
- reorder_workspaces_errors_on_unknown_id_no_partial_writes

Phase 2 spec §4.2.
EOF
)"
```

---

### Task 4: `create_workspace` auto-creates slug directory

Spec §4.3. When `create_workspace` is called without an explicit path, derive a slug from name, mkdir `~/Documents/workground/<slug>/`, and store the resulting path. Existing NULL-path workspaces continue working via `active_workspace_root` fallback.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (extend `create_workspace`; add `slugify` helper; add `compute_workspace_dir` helper for testability; extend tests)

- [ ] **Step 1: Write failing tests**

Append to `mod workspace_integrity_tests`:

```rust

    // ─── create_workspace auto-mkdir + slugify ──────────────────────────

    #[test]
    fn slugify_basic_ascii() {
        assert_eq!(super::slugify("My Project"), "my-project");
        assert_eq!(super::slugify("test"), "test");
    }

    #[test]
    fn slugify_collapses_special_chars() {
        assert_eq!(super::slugify("foo!!bar"), "foo-bar");
        assert_eq!(super::slugify("---weird---"), "weird");
    }

    #[test]
    fn slugify_chinese_only_falls_back_to_empty() {
        // Pure CJK strips to "" — caller is responsible for the workspace-<uuid8> fallback.
        assert_eq!(super::slugify("我的项目"), "");
    }

    #[test]
    fn slugify_truncates_long_input() {
        let long = "a".repeat(100);
        assert_eq!(super::slugify(&long).len(), 32);
    }

    #[test]
    fn compute_workspace_dir_uses_slug_when_no_path() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = super::compute_workspace_dir(tmp.path(), "My Project", None, "id-1234567890ab").unwrap();
        assert_eq!(dir, tmp.path().join("my-project"));
    }

    #[test]
    fn compute_workspace_dir_uses_uuid_fallback_when_slug_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = super::compute_workspace_dir(tmp.path(), "我的项目", None, "id-1234567890ab").unwrap();
        // "我的项目" → empty slug → fallback workspace-<first8 of id>
        assert_eq!(dir, tmp.path().join("workspace-id-12345"));
    }

    #[test]
    fn compute_workspace_dir_respects_explicit_path() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().join("custom");
        let dir = super::compute_workspace_dir(
            tmp.path(),
            "ignored",
            Some(custom.to_string_lossy().into_owned()),
            "id-anything",
        ).unwrap();
        assert_eq!(dir, custom);
    }
```

(These tests need the `tempfile` crate. Already a dev-dependency? Verify with `grep "tempfile" src-tauri/Cargo.toml`. If not present, add `tempfile = "3"` under `[dev-dependencies]`.)

- [ ] **Step 2: Run tests to verify they fail**

`cd src-tauri && cargo test --lib slugify 2>&1 | tail -15` — expect "not found".

- [ ] **Step 3: Verify `tempfile` dev-dep is present**

```bash
grep -nE "^tempfile|tempfile = " src-tauri/Cargo.toml
```

If empty, add to `src-tauri/Cargo.toml` under `[dev-dependencies]` (or create the section if missing):

```toml
[dev-dependencies]
tempfile = "3"
```

If `[dev-dependencies]` already exists, append `tempfile = "3"` to it instead of duplicating the header.

- [ ] **Step 4: Add helpers**

In `tauri_commands.rs`, insert immediately after `do_reorder_workspaces` (added in Task 3):

```rust

/// Simple ASCII slug: lowercase, non-alphanumeric → '-', collapse repeats,
/// trim leading/trailing '-', truncate to 32 chars. CJK and other non-ASCII
/// chars become '-' and get collapsed away, so a pure-Chinese name produces
/// an empty string — caller's responsibility to fall back.
pub(crate) fn slugify(name: &str) -> String {
    let lowered: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    // Collapse repeated '-'.
    let mut out = String::with_capacity(lowered.len());
    let mut prev_dash = false;
    for c in lowered.chars() {
        if c == '-' {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    // Trim and truncate.
    let trimmed = out.trim_matches('-');
    trimmed.chars().take(32).collect::<String>()
}

/// Pure function: given the workground root, workspace name, optional
/// explicit path, and a workspace id, produce the directory the workspace
/// should live in. Does NOT mkdir — caller does that. Extracted from
/// `create_workspace` so it's unit-testable without `state.workspace_root`.
pub(crate) fn compute_workspace_dir(
    workground_root: &std::path::Path,
    name: &str,
    explicit_path: Option<String>,
    id: &str,
) -> Result<std::path::PathBuf, Error> {
    if let Some(p) = explicit_path {
        if !p.trim().is_empty() {
            return Ok(std::path::PathBuf::from(p));
        }
    }
    let slug = slugify(name);
    let dir_name = if slug.is_empty() {
        // Take first 8 chars of id for uniqueness when name doesn't slugify.
        format!("workspace-{}", &id.chars().take(8).collect::<String>())
    } else {
        slug
    };
    Ok(workground_root.join(dir_name))
}
```

- [ ] **Step 5: Update `create_workspace` to use the helpers**

In `tauri_commands.rs`, find `pub async fn create_workspace` (around line 4505). Replace the entire function body with:

```rust
pub async fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    path: Option<String>,
    icon: Option<String>,
) -> Result<serde_json::Value, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let icon = icon.unwrap_or_else(|| "📁".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    // Compute target dir (auto-derived from name if no path supplied) and
    // mkdir it. create_dir_all is idempotent: existing dir is a no-op.
    let dir = compute_workspace_dir(&state.workspace_root, &name, path, &id)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::Internal(format!("mkdir failed for {:?}: {}", &dir, e)))?;
    let resolved_path = dir.to_string_lossy().into_owned();

    // Compute sort_order = MAX(sort_order) + 1 so the new workspace sorts last.
    // Subquery: COALESCE(MAX, -1) + 1 → 0 when table empty, N+1 when non-empty.
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let sort_order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM spaces", [],
        |r| r.get(0),
    ).unwrap_or(0);

    conn.execute(
        "INSERT INTO spaces (id, name, icon, path, sort_order, attached_dirs, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '[]', ?6, ?6)",
        rusqlite::params![id, name, icon, &resolved_path, sort_order, now],
    ).map_err(Error::Database)?;

    Ok(serde_json::json!({
        "id": id,
        "name": name,
        "icon": icon,
        "path": resolved_path,
        "sortOrder": sort_order,
        "attachedDirs": Vec::<String>::new(),
        "createdAt": now,
        "updatedAt": now,
    }))
}
```

- [ ] **Step 6: Run tests to verify they pass**

`cd src-tauri && cargo test --lib 2>&1 | tail -5` — expect all (existing + 7 new) pass.

- [ ] **Step 7: Build clean**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
```

Empty.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/Cargo.toml
git commit -m "$(cat <<'EOF'
feat(workspace): auto-create slug dir on create_workspace

create_workspace now resolves the workspace's on-disk path:
- explicit `path` arg → use as-is
- otherwise → derive slug from name; if slug empty (e.g. pure-CJK), fall
  back to `workspace-<first 8 chars of UUID>`; join under
  state.workspace_root (~/Documents/workground/) and `mkdir -p`

Also sets `sort_order = MAX(existing)+1` so the new workspace lands at the
end of the list (won't disrupt existing reorderings). Idempotent for
re-runs because `create_dir_all` is no-op on existing dirs and the slug is
deterministic for the same name.

Two helpers extracted for testability without AppState:
- slugify(&str) -> String
- compute_workspace_dir(&Path, &str, Option<String>, &str) -> Result<PathBuf>

Inline tests (7):
- slugify_basic_ascii / slugify_collapses_special_chars
- slugify_chinese_only_falls_back_to_empty / slugify_truncates_long_input
- compute_workspace_dir_uses_slug_when_no_path
- compute_workspace_dir_uses_uuid_fallback_when_slug_empty
- compute_workspace_dir_respects_explicit_path

Adds `tempfile = "3"` to [dev-dependencies] for tempdir support.

Phase 2 spec §4.3.
EOF
)"
```

---

### Task 5: Workspace-level attached directory IPCs (3 commands + helper)

Spec §4.4. Three commands operate on `spaces.attached_dirs` JSON column: get, attach (append-dedup), detach (remove). Shared `modify_attached_dirs` helper since session-level (Task 6) reuses the same shape.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (helper + 3 commands + tests)
- Modify: `src-tauri/src/main.rs` (register 3 commands)
- Modify: `ui/src/lib/tauri-bridge.ts` (3 wrappers)

- [ ] **Step 1: Write failing tests**

Append to `mod workspace_integrity_tests`:

```rust

    // ─── workspace attached directories ─────────────────────────────────

    fn read_workspace_dirs(conn: &Connection, id: &str) -> Vec<String> {
        let json: String = conn.query_row(
            "SELECT attached_dirs FROM spaces WHERE id = ?1",
            rusqlite::params![id], |r| r.get(0),
        ).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn attach_workspace_directory_appends_to_json() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        let after = super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |mut dirs| {
            dirs.push("/tmp/foo".into());
            dirs
        }).unwrap();
        assert_eq!(after, vec!["/tmp/foo".to_string()]);
        assert_eq!(read_workspace_dirs(&conn, "ws-x"), vec!["/tmp/foo".to_string()]);
    }

    #[test]
    fn attach_workspace_directory_dedupes() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        // First attach.
        super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |mut dirs| {
            if !dirs.contains(&"/tmp/foo".to_string()) { dirs.push("/tmp/foo".into()); }
            dirs
        }).unwrap();
        // Second attach of same path.
        let after = super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |mut dirs| {
            if !dirs.contains(&"/tmp/foo".to_string()) { dirs.push("/tmp/foo".into()); }
            dirs
        }).unwrap();
        assert_eq!(after, vec!["/tmp/foo".to_string()], "duplicate path not appended");
    }

    #[test]
    fn detach_workspace_directory_removes_existing() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |_| {
            vec!["/tmp/foo".into(), "/tmp/bar".into()]
        }).unwrap();
        let after = super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |dirs| {
            dirs.into_iter().filter(|d| d != "/tmp/foo").collect()
        }).unwrap();
        assert_eq!(after, vec!["/tmp/bar".to_string()]);
    }

    #[test]
    fn detach_workspace_directory_noop_when_missing() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        let after = super::do_modify_attached_dirs(&conn, "spaces", "ws-x", |dirs| {
            dirs.into_iter().filter(|d| d != "/tmp/notthere").collect()
        }).unwrap();
        assert_eq!(after, Vec::<String>::new());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

`cd src-tauri && cargo test --lib attach_workspace 2>&1 | tail -10` — expect "not found".

- [ ] **Step 3: Add the helper**

In `tauri_commands.rs`, insert immediately after `compute_workspace_dir` (added in Task 4):

```rust

/// Generic read-modify-write of an `attached_dirs` JSON column. Works for
/// `spaces` (workspace level) and `agent_sessions` (session level). The
/// caller's closure receives the current list and returns the new list;
/// we serialize back to JSON and write. `id_col` is always "id" for both
/// tables. Returns the new list.
///
/// Note: `spaces.updated_at` is RFC3339 TEXT; `agent_sessions.updated_at`
/// is INTEGER milliseconds. We branch on the table name.
pub(crate) fn do_modify_attached_dirs<F>(
    conn: &rusqlite::Connection,
    table: &str,
    id: &str,
    f: F,
) -> Result<Vec<String>, Error>
where
    F: FnOnce(Vec<String>) -> Vec<String>,
{
    let json: String = conn
        .query_row(
            &format!("SELECT attached_dirs FROM {} WHERE id = ?1", table),
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                Error::NotFound(format!("{} '{}'", table, id))
            }
            other => Error::Database(other),
        })?;
    let dirs: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
    let new_dirs = f(dirs);
    let new_json = serde_json::to_string(&new_dirs)
        .map_err(|e| Error::Internal(format!("JSON encode: {}", e)))?;
    let updated_at_sql = match table {
        "agent_sessions" => "?2".to_string(),
        _ => "?2".to_string(),
    };
    let updated_at_value: Box<dyn rusqlite::ToSql> = match table {
        "agent_sessions" => Box::new(chrono::Utc::now().timestamp_millis()),
        _ => Box::new(chrono::Utc::now().to_rfc3339()),
    };
    conn.execute(
        &format!(
            "UPDATE {} SET attached_dirs = ?1, updated_at = {} WHERE id = ?3",
            table, updated_at_sql
        ),
        rusqlite::params![&new_json, &*updated_at_value, id],
    ).map_err(Error::Database)?;
    Ok(new_dirs)
}
```

- [ ] **Step 4: Add the 3 Tauri commands**

In `tauri_commands.rs`, insert immediately after `reorder_workspaces` (added in Task 3):

```rust

#[tauri::command]
pub async fn get_workspace_directories(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    require_workspace_exists(&conn, &workspace_id)?;
    let json: String = conn.query_row(
        "SELECT attached_dirs FROM spaces WHERE id = ?1",
        rusqlite::params![&workspace_id], |r| r.get(0),
    ).map_err(Error::Database)?;
    serde_json::from_str(&json)
        .map_err(|e| Error::Internal(format!("JSON parse: {}", e)))
}

#[tauri::command]
pub async fn attach_workspace_directory(
    state: State<'_, AppState>,
    workspace_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    require_workspace_exists(&conn, &workspace_id)?;
    do_modify_attached_dirs(&conn, "spaces", &workspace_id, |mut dirs| {
        if !dirs.contains(&dir_path) { dirs.push(dir_path.clone()); }
        dirs
    })
}

#[tauri::command]
pub async fn detach_workspace_directory(
    state: State<'_, AppState>,
    workspace_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    require_workspace_exists(&conn, &workspace_id)?;
    do_modify_attached_dirs(&conn, "spaces", &workspace_id, |dirs| {
        dirs.into_iter().filter(|d| d != &dir_path).collect()
    })
}
```

- [ ] **Step 5: Register 3 commands in `invoke_handler!`**

In `main.rs`, insert immediately after `reorder_workspaces,`:

```rust
            uclaw_core::tauri_commands::get_workspace_directories,
            uclaw_core::tauri_commands::attach_workspace_directory,
            uclaw_core::tauri_commands::detach_workspace_directory,
```

- [ ] **Step 6: Add frontend wrappers**

In `tauri-bridge.ts`, immediately after `reorderWorkspaces`:

```typescript

export const getWorkspaceDirectories = (workspaceId: string): Promise<string[]> =>
  invoke('get_workspace_directories', { workspaceId })

export const attachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_workspace_directory', { workspaceId, dirPath })

export const detachWorkspaceDirectory = (workspaceId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_workspace_directory', { workspaceId, dirPath })
```

- [ ] **Step 7: Run tests + verify build**

```bash
cd src-tauri && cargo test --lib attach_workspace 2>&1 | tail -10  # 4 pass
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head  # empty
cd ui && npx tsc --noEmit 2>&1 | head -10  # empty
```

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(workspace): attach/detach/get workspace_directory IPCs

Three new Tauri commands operate on the spaces.attached_dirs JSON column:
- get_workspace_directories(workspaceId)
- attach_workspace_directory(workspaceId, dirPath) — append-dedup
- detach_workspace_directory(workspaceId, dirPath) — filter-out, no-op if missing

Shared helper `do_modify_attached_dirs(conn, table, id, f)` does the
read-modify-write dance for both this and session-level (Task 6); branches
on table name for the updated_at column type difference (spaces is TEXT
RFC3339, agent_sessions is INTEGER milliseconds).

Inline tests (4):
- attach_workspace_directory_appends_to_json
- attach_workspace_directory_dedupes
- detach_workspace_directory_removes_existing
- detach_workspace_directory_noop_when_missing

Phase 2 spec §4.4.
EOF
)"
```

---

### Task 6: Session-level attached directory IPCs + extend list_agent_sessions

Spec §4.5. Three commands mirror Task 5 but operate on `agent_sessions.attached_dirs`. Reuses `do_modify_attached_dirs` helper. Also extends `list_agent_sessions` to surface `attachedDirs` for hydration.

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (3 commands + extend `list_agent_sessions` + tests)
- Modify: `src-tauri/src/main.rs` (register 3)
- Modify: `ui/src/lib/tauri-bridge.ts` (3 wrappers)

- [ ] **Step 1: Write failing tests**

Append to `mod workspace_integrity_tests`:

```rust

    // ─── session attached directories ───────────────────────────────────

    fn read_session_dirs(conn: &Connection, id: &str) -> Vec<String> {
        let json: String = conn.query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params![id], |r| r.get(0),
        ).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn attach_session_directory_appends() {
        let conn = fresh_db();
        insert_session(&conn, "s-1", "default");
        let after = super::do_modify_attached_dirs(&conn, "agent_sessions", "s-1", |mut dirs| {
            dirs.push("/tmp/sess-dir".into());
            dirs
        }).unwrap();
        assert_eq!(after, vec!["/tmp/sess-dir".to_string()]);
        assert_eq!(read_session_dirs(&conn, "s-1"), vec!["/tmp/sess-dir".to_string()]);
    }

    #[test]
    fn list_session_directories_returns_attached() {
        let conn = fresh_db();
        insert_session(&conn, "s-1", "default");
        super::do_modify_attached_dirs(&conn, "agent_sessions", "s-1", |_| {
            vec!["/tmp/a".into(), "/tmp/b".into()]
        }).unwrap();
        // Direct DB read (mimics what the Tauri command would do under the lock).
        let json: String = conn.query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params!["s-1"], |r| r.get(0),
        ).unwrap();
        let dirs: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(dirs, vec!["/tmp/a".to_string(), "/tmp/b".to_string()]);
    }
```

- [ ] **Step 2: Run tests to verify they fail (or pass — helper is shared)**

`cd src-tauri && cargo test --lib session_dir 2>&1 | tail -10` — these tests should pass IF Task 5's helper is in place. They're regression tests confirming the helper works for the `agent_sessions` table too.

- [ ] **Step 3: Add the 3 Tauri commands**

In `tauri_commands.rs`, insert immediately after `detach_workspace_directory` (added in Task 5):

```rust

#[tauri::command]
pub async fn list_session_directories(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let json: String = conn.query_row(
        "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
        rusqlite::params![&session_id], |r| r.get(0),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("agent_session '{}'", session_id)),
        other => Error::Database(other),
    })?;
    serde_json::from_str(&json)
        .map_err(|e| Error::Internal(format!("JSON parse: {}", e)))
}

#[tauri::command]
pub async fn attach_session_directory(
    state: State<'_, AppState>,
    session_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    do_modify_attached_dirs(&conn, "agent_sessions", &session_id, |mut dirs| {
        if !dirs.contains(&dir_path) { dirs.push(dir_path.clone()); }
        dirs
    })
}

#[tauri::command]
pub async fn detach_session_directory(
    state: State<'_, AppState>,
    session_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    do_modify_attached_dirs(&conn, "agent_sessions", &session_id, |dirs| {
        dirs.into_iter().filter(|d| d != &dir_path).collect()
    })
}
```

- [ ] **Step 4: Extend `list_agent_sessions` to surface attached_dirs**

In `tauri_commands.rs`, find `pub async fn list_agent_sessions` (around line 3710). Update the SELECT to include `attached_dirs`, and add the field to the returned JSON:

Replace the function body with:

```rust
pub async fn list_agent_sessions(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let mut stmt = conn.prepare(
        "SELECT id, space_id, title, metadata_json, message_count, pinned, archived,
                attached_dirs, created_at, updated_at
         FROM agent_sessions ORDER BY updated_at DESC"
    ).map_err(|e| Error::Database(e))?;
    let rows = stmt.query_map([], |row| {
        let meta_str: String = row.get(3)?;
        let attached_dirs_json: String = row.get::<_, String>(7).unwrap_or_else(|_| "[]".into());
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            meta_str,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            attached_dirs_json,
            row.get::<_, i64>(8)?,
            row.get::<_, i64>(9)?,
        ))
    }).map_err(|e| Error::Database(e))?;
    let sessions: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).map(|(id, space_id, title, meta_str, msg_count, pinned, archived, attached_dirs_json, created_at, updated_at)| {
        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Object(Default::default()));
        let title_from_meta = meta.get("title").and_then(|v| v.as_str()).unwrap_or(&title).to_string();
        let title_emoji = meta.get("emoji").and_then(|v| v.as_str()).unwrap_or("💬").to_string();
        let title_pending = meta.get("title_pending").and_then(|v| v.as_bool()).unwrap_or(false);
        let attached_dirs: Vec<String> = serde_json::from_str(&attached_dirs_json).unwrap_or_default();
        serde_json::json!({
            "id": id,
            "workspaceId": space_id,
            "title": title_from_meta,
            "titleEmoji": title_emoji,
            "titlePending": title_pending,
            "metadataJson": meta_str,
            "messageCount": msg_count,
            "pinned": pinned != 0,
            "archived": archived != 0,
            "attachedDirs": attached_dirs,
            "createdAt": created_at,
            "updatedAt": updated_at,
        })
    }).collect();
    Ok(sessions)
}
```

- [ ] **Step 5: Register 3 commands in `invoke_handler!`**

In `main.rs`, insert after `detach_workspace_directory,`:

```rust
            uclaw_core::tauri_commands::list_session_directories,
            uclaw_core::tauri_commands::attach_session_directory,
            uclaw_core::tauri_commands::detach_session_directory,
```

- [ ] **Step 6: Add frontend wrappers**

In `tauri-bridge.ts`, immediately after `detachWorkspaceDirectory`:

```typescript

export const listSessionDirectories = (sessionId: string): Promise<string[]> =>
  invoke('list_session_directories', { sessionId })

export const attachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('attach_session_directory', { sessionId, dirPath })

export const detachSessionDirectory = (sessionId: string, dirPath: string): Promise<string[]> =>
  invoke('detach_session_directory', { sessionId, dirPath })
```

- [ ] **Step 7: Add session attached-dirs atoms**

In `ui/src/atoms/agent-atoms.ts`, find a reasonable place near the other agent-related atoms (e.g., after `currentAgentWorkspaceIdAtom`). Add:

```typescript

/** Map workspace.id → attached dir paths. Hydrated at startup from
 *  list_spaces (each WorkspaceInfo carries attachedDirs); kept in sync
 *  by attach/detach mutations.
 */
export const workspaceAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())

/** Map agent_session.id → attached dir paths. Hydrated at startup from
 *  list_agent_sessions (each session carries attachedDirs in its JSON).
 */
export const agentSessionAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())
```

- [ ] **Step 8: Tests + build**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

All clean.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts ui/src/atoms/agent-atoms.ts
git commit -m "$(cat <<'EOF'
feat(workspace): attach/detach/list session_directory IPCs

Three new Tauri commands mirror the workspace-level shape from Task 5
but operate on agent_sessions.attached_dirs. Reuses the shared
do_modify_attached_dirs helper. list_agent_sessions extended to
include attachedDirs in its returned JSON so the frontend can hydrate
a per-session atom map in one call.

Two new atoms (intentionally renamed vs the Phase 1 deleted ones to
make grep distinguish):
- workspaceAttachedDirsMapAtom: Map<workspaceId, string[]>
- agentSessionAttachedDirsMapAtom: Map<sessionId, string[]>

Inline tests (2):
- attach_session_directory_appends
- list_session_directories_returns_attached

Phase 2 spec §4.5.
EOF
)"
```

---

### Task 7: Add `tauri-plugin-dialog` + `tauri-plugin-shell` + capabilities

Spec §4.6. Adopt Tauri 2 plugins for folder picker, file open with default app, and reveal in Finder. Update the `openFolderDialog` / `openFile` / `openExternal` frontend wrappers to use plugin JS APIs directly (instead of invoke). This is config-heavy but small in code size.

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs` (plugin init in `tauri::Builder`)
- Modify: `src-tauri/capabilities/default.json` (grant permissions)
- Modify: `ui/src/lib/tauri-bridge.ts` (rewire `openFolderDialog`, `openFile`, `openExternal` to plugins)
- Modify: `ui/package.json` (add `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-shell`)

- [ ] **Step 1: Add Rust crate deps**

In `src-tauri/Cargo.toml`, find the `[dependencies]` section (look for the existing `tauri = { ... }` line around line 64). Add these entries near it (alphabetical):

```toml
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
```

- [ ] **Step 2: Add frontend npm deps**

```bash
cd ui && npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-shell
```

Verify they show up in `ui/package.json` dependencies.

- [ ] **Step 3: Initialize plugins in main.rs**

In `src-tauri/src/main.rs`, find the `tauri::Builder::default()` call (search for `.plugin(`). Add two `.plugin()` calls before the existing ones:

```rust
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
```

(Place them at the top of the plugin chain so they initialize before any handler is registered.)

- [ ] **Step 4: Grant capabilities**

In `src-tauri/capabilities/default.json`, append to the `"permissions"` array (after the existing entries, before the closing `]`):

```json
    "dialog:default",
    "shell:default",
    "shell:allow-open"
```

The whole array should now end like:

```json
    "core:window:allow-set-position",
    "core:event:default",
    "dialog:default",
    "shell:default",
    "shell:allow-open"
  ]
```

- [ ] **Step 5: Rewire frontend wrappers**

In `ui/src/lib/tauri-bridge.ts`, find the existing wrappers (these are placeholders left by Phase 1 — check the current file state with `grep -nE "openFolderDialog|openFile|openExternal" ui/src/lib/tauri-bridge.ts`).

If `openFolderDialog`, `openFile`, `openExternal` exist as `invoke('...')` wrappers, replace them. If they don't exist (Phase 1 deleted them), add fresh:

At the top of `tauri-bridge.ts`, near other imports:

```typescript
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { open as openShellUrl, openPath as openShellPath } from '@tauri-apps/plugin-shell'
```

Then near the bottom (in a new "File / dialog actions (plugin-backed)" section), add:

```typescript

// --- File / dialog actions (Tauri plugin-backed) ---

export const openFolderDialog = async (): Promise<{ path: string; name: string } | null> => {
  const selected = await openDialog({ directory: true, multiple: false })
  if (!selected || typeof selected !== 'string') return null
  const name = selected.split('/').pop() ?? selected
  return { path: selected, name }
}

export const openFile = (path: string): Promise<void> => openShellPath(path)

export const openExternal = (url: string): Promise<void> => openShellUrl(url)

export const showInFinder = (path: string): Promise<void> => {
  // tauri-plugin-shell v2 doesn't expose revealItemInDir in stable;
  // fall back to opening the containing folder.
  const parent = path.substring(0, path.lastIndexOf('/'))
  return openShellPath(parent || '/')
}
```

- [ ] **Step 6: Verify build (Rust + npm fetch + TS)**

```bash
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Both empty. (If `cargo build` complains about plugin version mismatch, pin to a specific minor version: `tauri-plugin-dialog = "2.0"` etc. Verify the npm packages installed.)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/main.rs src-tauri/capabilities/default.json ui/package.json ui/package-lock.json ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
chore(deps): add tauri-plugin-dialog + tauri-plugin-shell + capabilities

Rust deps:
- tauri-plugin-dialog = "2"  (folder picker)
- tauri-plugin-shell = "2"   (open file with default app, open external URL)

NPM deps:
- @tauri-apps/plugin-dialog
- @tauri-apps/plugin-shell

Initialized both plugins in main.rs Builder. Granted dialog:default,
shell:default, shell:allow-open in capabilities/default.json.

Frontend wrappers rewired to use plugin JS APIs directly (not invoke):
- openFolderDialog → plugin-dialog open({directory: true})
- openFile → plugin-shell openPath
- openExternal → plugin-shell open
- showInFinder → plugin-shell openPath of parent dir (v2 stable lacks
  revealItemInDir; opens the containing folder as the next-best)

Phase 2 spec §4.6.
EOF
)"
```

---

### Task 8: `rename_attached_file` + `move_attached_file` + `read_attached_file` IPCs

Spec §4.6 (latter half). Three self-rolled Rust commands using `std::fs`. Cross-volume rename falls back to copy-then-delete. `read_attached_file` retained for the few cases that need bytes (LLM ingestion).

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs` (3 commands + tests)
- Modify: `src-tauri/src/main.rs` (register 3)
- Modify: `ui/src/lib/tauri-bridge.ts` (3 wrappers)

- [ ] **Step 1: Write failing tests**

Append to `mod workspace_integrity_tests`:

```rust

    // ─── file action commands ───────────────────────────────────────────

    use std::fs;
    use std::io::Write;

    fn create_tmp_file(dir: &std::path::Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn rename_attached_file_renames_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let original = create_tmp_file(tmp.path(), "old.txt", b"hello");
        let new_path = super::do_rename_attached_file(
            original.to_string_lossy().as_ref(),
            "new.txt",
        ).unwrap();
        assert!(!original.exists(), "old path should no longer exist");
        let new_pb = std::path::PathBuf::from(&new_path);
        assert!(new_pb.exists(), "new path should exist");
        assert_eq!(fs::read(&new_pb).unwrap(), b"hello");
    }

    #[test]
    fn move_attached_file_moves_to_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        let dst_dir = tmp.path().join("dst");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();
        let original = create_tmp_file(&src_dir, "f.txt", b"data");
        let new_path = super::do_move_attached_file(
            original.to_string_lossy().as_ref(),
            dst_dir.to_string_lossy().as_ref(),
        ).unwrap();
        assert!(!original.exists());
        let new_pb = std::path::PathBuf::from(&new_path);
        assert!(new_pb.starts_with(&dst_dir));
        assert_eq!(fs::read(&new_pb).unwrap(), b"data");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

`cd src-tauri && cargo test --lib rename_attached 2>&1 | tail -10` — expect "not found".

- [ ] **Step 3: Add helpers + Tauri commands**

In `tauri_commands.rs`, insert near the other workspace helpers (after `do_modify_attached_dirs`):

```rust

/// Rename a file within its parent directory. Returns the new absolute path.
pub(crate) fn do_rename_attached_file(path: &str, new_name: &str) -> Result<String, Error> {
    let p = std::path::Path::new(path);
    let parent = p.parent()
        .ok_or_else(|| Error::Internal(format!("no parent for {}", path)))?;
    let new_path = parent.join(new_name);
    std::fs::rename(p, &new_path)
        .map_err(|e| Error::Internal(format!("rename {} → {}: {}", path, new_path.display(), e)))?;
    Ok(new_path.to_string_lossy().into_owned())
}

/// Move a file into `dest_dir`, keeping the filename. Returns the new path.
/// Falls back to copy+delete on cross-volume errors.
pub(crate) fn do_move_attached_file(path: &str, dest_dir: &str) -> Result<String, Error> {
    let p = std::path::Path::new(path);
    let fname = p.file_name()
        .ok_or_else(|| Error::Internal(format!("no filename in {}", path)))?;
    let new_path = std::path::Path::new(dest_dir).join(fname);
    match std::fs::rename(p, &new_path) {
        Ok(()) => Ok(new_path.to_string_lossy().into_owned()),
        Err(e) if e.raw_os_error() == Some(18) /* EXDEV */ => {
            std::fs::copy(p, &new_path)
                .map_err(|e2| Error::Internal(format!("cross-volume copy: {}", e2)))?;
            std::fs::remove_file(p)
                .map_err(|e2| Error::Internal(format!("cross-volume remove: {}", e2)))?;
            Ok(new_path.to_string_lossy().into_owned())
        }
        Err(e) => Err(Error::Internal(format!("move: {}", e))),
    }
}
```

Add the Tauri commands (insert after `detach_session_directory` from Task 6):

```rust

#[tauri::command]
pub async fn rename_attached_file(path: String, new_name: String) -> Result<String, Error> {
    do_rename_attached_file(&path, &new_name)
}

#[tauri::command]
pub async fn move_attached_file(path: String, dest_dir: String) -> Result<String, Error> {
    do_move_attached_file(&path, &dest_dir)
}

#[tauri::command]
pub async fn read_attached_file(path: String) -> Result<Vec<u8>, Error> {
    std::fs::read(&path).map_err(|e| Error::Internal(format!("read {}: {}", path, e)))
}
```

- [ ] **Step 4: Register 3 commands in `invoke_handler!`**

In `main.rs`, insert after `detach_session_directory,`:

```rust
            uclaw_core::tauri_commands::rename_attached_file,
            uclaw_core::tauri_commands::move_attached_file,
            uclaw_core::tauri_commands::read_attached_file,
```

- [ ] **Step 5: Add frontend wrappers**

In `tauri-bridge.ts`, immediately after `detachSessionDirectory`:

```typescript

export const renameAttachedFile = (path: string, newName: string): Promise<string> =>
  invoke('rename_attached_file', { path, newName })

export const moveAttachedFile = (path: string, destDir: string): Promise<string> =>
  invoke('move_attached_file', { path, destDir })

export const readAttachedFile = (path: string): Promise<number[]> =>
  invoke('read_attached_file', { path })
```

- [ ] **Step 6: Run tests + verify build**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
cd src-tauri && cargo build --quiet 2>&1 | grep -E "^error" | head
cd ui && npx tsc --noEmit 2>&1 | head -10
```

All clean. Cross-volume test deferred to manual smoke.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts
git commit -m "$(cat <<'EOF'
feat(workspace): rename_attached_file + move_attached_file + read_attached_file

Three self-rolled fs commands (no plugin needed):
- rename_attached_file(path, new_name) — std::fs::rename within parent
- move_attached_file(path, dest_dir) — std::fs::rename across dirs, falls
  back to copy + remove on EXDEV (cross-volume)
- read_attached_file(path) — std::fs::read for LLM ingestion path (image
  previews use convertFileSrc instead, much faster)

Inline tests (2, using tempfile):
- rename_attached_file_renames_in_place
- move_attached_file_moves_to_destination

Cross-volume move tested in manual smoke per spec §5.3.

Phase 2 spec §4.6.
EOF
)"
```

---

### Task 9: WorkspaceGroup hover affordances (rename + delete + drag)

Spec §4.7. Adds hover-revealed buttons on each workspace row: Pencil (rename via inline input) and Trash (delete via existing `deleteWorkspace`). Adds row-level drag-and-drop reorder with drop indicators.

**Files:**
- Modify: `ui/src/components/workspace/WorkspaceGroup.tsx` (add buttons, rename state, drag handlers)
- Modify: `ui/src/components/workspace/WorkspaceRail.tsx` (lift drag state for cross-row reorder; pass new props down)
- Modify: `ui/src/atoms/workspace.ts` (add `updateWorkspaceAtom`, `reorderWorkspacesAtom` action atoms)
- Test: `ui/src/components/workspace/WorkspaceGroup.test.tsx` (new file)

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/workspace/WorkspaceGroup.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'jotai'
import { WorkspaceGroup } from './WorkspaceGroup'

// Mock the bridge so tests don't hit a real backend.
vi.mock('@/lib/tauri-bridge', () => ({
  updateWorkspace: vi.fn().mockResolvedValue({}),
  deleteWorkspace: vi.fn().mockResolvedValue(undefined),
}))

describe('WorkspaceGroup', () => {
  it('shows hover Pencil + Trash buttons on non-default workspace', () => {
    const onSelectSession = vi.fn()
    const onSelectWorkspace = vi.fn()
    render(
      <Provider>
        <WorkspaceGroup
          id="ws-x"
          name="Test"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={onSelectSession}
          onSelectWorkspace={onSelectWorkspace}
        />
      </Provider>
    )
    // Pencil and Trash buttons exist (always present in DOM; opacity-0 by default,
    // hover-revealed via CSS; we just confirm they're rendered, not their visibility).
    expect(screen.getByTitle('重命名')).toBeTruthy()
    expect(screen.getByTitle('删除')).toBeTruthy()
  })

  it('hides Trash on default workspace', () => {
    render(
      <Provider>
        <WorkspaceGroup
          id="default"
          name="默认工作区"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={() => {}}
          onSelectWorkspace={() => {}}
        />
      </Provider>
    )
    expect(screen.queryByTitle('删除')).toBeNull()
    // Pencil also hidden on default (since name change refused server-side).
    expect(screen.queryByTitle('重命名')).toBeNull()
  })

  it('clicking Pencil enters inline rename mode', () => {
    render(
      <Provider>
        <WorkspaceGroup
          id="ws-x"
          name="Original"
          icon="📁"
          sessions={[]}
          isActive={false}
          activeSessionId={null}
          onSelectSession={() => {}}
          onSelectWorkspace={() => {}}
        />
      </Provider>
    )
    fireEvent.click(screen.getByTitle('重命名'))
    // Input should appear with the current name as value.
    const input = screen.getByDisplayValue('Original') as HTMLInputElement
    expect(input).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run test to verify failure**

`cd ui && npm test -- --run WorkspaceGroup 2>&1 | tail -20` — expect failures (no Pencil/Trash buttons in DOM yet).

- [ ] **Step 3: Add update + reorder action atoms**

In `ui/src/atoms/workspace.ts`, after `selectWorkspaceAtom` (around line 50), add:

```typescript

// Action: update a workspace's name or icon
export const updateWorkspaceAtom = atom(
  null,
  async (_get, set, input: { id: string; name?: string; icon?: string }) => {
    await bridge.updateWorkspace(input)
    // Re-sync from backend rather than maintain optimistic state.
    const spaces = await bridge.listSpaces()
    set(workspacesAtom, spaces as WorkspaceInfo[])
  }
)

// Action: persist a new workspace order
export const reorderWorkspacesAtom = atom(
  null,
  async (_get, set, orderedIds: string[]) => {
    await bridge.reorderWorkspaces(orderedIds)
    const spaces = await bridge.listSpaces()
    set(workspacesAtom, spaces as WorkspaceInfo[])
  }
)
```

- [ ] **Step 4: Rebuild WorkspaceGroup with hover affordances + rename + drag**

Replace `ui/src/components/workspace/WorkspaceGroup.tsx` entirely:

```typescript
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { ChevronRight, ChevronDown, Pencil, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { SessionItem } from './SessionItem'
import { agentSessionIndicatorMapAtom } from '@/atoms/agent-atoms'
import {
  updateWorkspaceAtom,
  refreshWorkspacesAtom,
  type WorkspaceSession,
} from '@/atoms/workspace'
import { deleteWorkspace } from '@/lib/tauri-bridge'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

interface WorkspaceGroupProps {
  id: string
  name: string
  icon: string
  sessions: WorkspaceSession[]
  isActive: boolean
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
  onSelectWorkspace: () => void
  /** Whether this row is currently being dragged. */
  isDragging?: boolean
  /** Drop indicator: 'before' | 'after' | null. */
  dropIndicator?: 'before' | 'after' | null
  /** DnD handlers from parent (WorkspaceRail). */
  onDragStart?: (e: React.DragEvent, id: string) => void
  onDragOver?: (e: React.DragEvent, id: string) => void
  onDragLeave?: (e: React.DragEvent) => void
  onDrop?: (e: React.DragEvent, id: string) => void
  onDragEnd?: () => void
}

export function WorkspaceGroup({
  id,
  name,
  icon,
  sessions,
  isActive,
  activeSessionId,
  onSelectSession,
  onDeleteSession,
  onSelectWorkspace,
  isDragging,
  dropIndicator,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
  onDragEnd,
}: WorkspaceGroupProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(isActive)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const updateWs = useSetAtom(updateWorkspaceAtom)
  const refreshWs = useSetAtom(refreshWorkspacesAtom)

  // Rename state
  const [renaming, setRenaming] = React.useState(false)
  const [renameValue, setRenameValue] = React.useState(name)
  const renameInputRef = React.useRef<HTMLInputElement>(null)

  // Delete confirm state
  const [confirmingDelete, setConfirmingDelete] = React.useState(false)

  React.useEffect(() => {
    if (isActive) setExpanded(true)
  }, [isActive])

  React.useEffect(() => {
    if (renaming) {
      requestAnimationFrame(() => {
        renameInputRef.current?.focus()
        renameInputRef.current?.select()
      })
    }
  }, [renaming])

  const canMutate = id !== 'default'

  const startRename = (e: React.MouseEvent): void => {
    e.stopPropagation()
    setRenameValue(name)
    setRenaming(true)
  }

  const commitRename = async (): Promise<void> => {
    const trimmed = renameValue.trim()
    if (!trimmed || trimmed === name) {
      setRenaming(false)
      return
    }
    try {
      await updateWs({ id, name: trimmed })
    } catch (err) {
      const msg = err instanceof Error ? err.message : '重命名失败'
      toast.error(msg)
    } finally {
      setRenaming(false)
    }
  }

  const cancelRename = (): void => {
    setRenaming(false)
    setRenameValue(name)
  }

  const handleRenameKeyDown = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter') {
      if (e.nativeEvent.isComposing) return
      e.preventDefault()
      commitRename()
    } else if (e.key === 'Escape') {
      cancelRename()
    }
  }

  const confirmDelete = async (): Promise<void> => {
    try {
      await deleteWorkspace(id)
      await refreshWs()
    } catch (err) {
      const msg = err instanceof Error ? err.message : '删除失败'
      toast.error(msg)
    } finally {
      setConfirmingDelete(false)
    }
  }

  return (
    <>
      <div className="mb-1 relative">
        {dropIndicator === 'before' && (
          <div className="absolute -top-0.5 left-1 right-1 h-0.5 bg-primary rounded-full z-10" />
        )}
        <div
          draggable={canMutate && !renaming}
          onDragStart={(e) => onDragStart?.(e, id)}
          onDragOver={(e) => onDragOver?.(e, id)}
          onDragLeave={onDragLeave}
          onDrop={(e) => onDrop?.(e, id)}
          onDragEnd={onDragEnd}
          className={cn(
            'group flex items-center gap-1.5 px-2 py-1 rounded-md cursor-pointer select-none',
            'text-[12px] font-semibold uppercase tracking-wide',
            isActive ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
            isDragging && 'opacity-40',
          )}
          onClick={() => {
            if (renaming) return
            onSelectWorkspace()
            setExpanded((v) => !v)
          }}
        >
          <span className="text-[13px]">{icon}</span>
          {renaming ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={commitRename}
              onClick={(e) => e.stopPropagation()}
              className="flex-1 min-w-0 bg-transparent text-[12px] uppercase tracking-wide border-b border-primary/50 outline-none px-0.5"
              maxLength={64}
            />
          ) : (
            <span className="flex-1 truncate">{name}</span>
          )}

          {canMutate && !renaming && (
            <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
              <button
                onClick={startRename}
                className="p-0.5 rounded hover:bg-foreground/[0.08] text-foreground/30 hover:text-foreground/60 transition-colors"
                title="重命名"
              >
                <Pencil className="size-3" />
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); setConfirmingDelete(true) }}
                className="p-0.5 rounded hover:bg-destructive/10 text-foreground/30 hover:text-destructive transition-colors"
                title="删除"
              >
                <Trash2 className="size-3" />
              </button>
            </div>
          )}

          {!renaming && (expanded ? (
            <ChevronDown className="h-3 w-3 shrink-0" />
          ) : (
            <ChevronRight className="h-3 w-3 shrink-0" />
          ))}
        </div>
        {dropIndicator === 'after' && (
          <div className="absolute -bottom-0.5 left-1 right-1 h-0.5 bg-primary rounded-full z-10" />
        )}
      </div>

      {expanded && (
        <div className="pl-3 flex flex-col gap-0.5 mt-0.5 mb-1">
          {sessions.length === 0 && (
            <p className="text-[11px] text-muted-foreground px-2 py-1">No sessions yet</p>
          )}
          {sessions.map((s) => (
            <SessionItem
              key={s.id}
              id={s.id}
              title={s.title}
              titleEmoji={s.titleEmoji}
              titlePending={s.titlePending}
              isActive={activeSessionId === s.id}
              running={indicatorMap.get(s.id) === 'running'}
              onClick={() => onSelectSession(s.id)}
              onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
            />
          ))}
        </div>
      )}

      <AlertDialog open={confirmingDelete} onOpenChange={(v) => { if (!v) setConfirmingDelete(false) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除工作区?</AlertDialogTitle>
            <AlertDialogDescription>
              删除「{name}」后,该工作区下的会话会被移动到「默认工作区」。文件夹本身不会被删除。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
```

- [ ] **Step 5: Add drag state + handlers in WorkspaceRail**

In `ui/src/components/workspace/WorkspaceRail.tsx`, replace its return statement and add state/handlers. Replace the entire file with:

```typescript
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Plus } from 'lucide-react'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  refreshWorkspacesAtom,
  selectWorkspaceAtom,
  reorderWorkspacesAtom,
} from '@/atoms/workspace'
import { WorkspaceGroup } from './WorkspaceGroup'
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog'
import { cn } from '@/lib/utils'

interface WorkspaceRailProps {
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
}

export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
}: WorkspaceRailProps): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const workspaceSessions = useAtomValue(workspaceSessionsAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const reorderWorkspaces = useSetAtom(reorderWorkspacesAtom)
  const [createOpen, setCreateOpen] = React.useState(false)

  // DnD state (workspace-level reorder)
  const [dragId, setDragId] = React.useState<string | null>(null)
  const [dropIndicator, setDropIndicator] = React.useState<{ id: string; position: 'before' | 'after' } | null>(null)

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const handleCreated = async (ws: { id: string; name: string; icon: string }) => {
    await refreshWorkspaces()
    await selectWorkspace(ws.id)
  }

  const handleDragStart = (e: React.DragEvent, id: string): void => {
    setDragId(id)
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', id)
  }

  const handleDragOver = (e: React.DragEvent, targetId: string): void => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    if (!dragId || dragId === targetId) {
      setDropIndicator(null)
      return
    }
    const rect = e.currentTarget.getBoundingClientRect()
    const ratio = (e.clientY - rect.top) / rect.height
    const position: 'before' | 'after' = ratio < 0.5 ? 'before' : 'after'
    if (dropIndicator?.id === targetId && dropIndicator.position === position) return
    setDropIndicator({ id: targetId, position })
  }

  const handleDragLeave = (e: React.DragEvent): void => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setDropIndicator(null)
    }
  }

  const handleDrop = async (_e: React.DragEvent, targetId: string): Promise<void> => {
    if (!dragId || dragId === targetId || !dropIndicator) {
      setDragId(null)
      setDropIndicator(null)
      return
    }
    const fromIdx = workspaces.findIndex((w) => w.id === dragId)
    const toIdx = workspaces.findIndex((w) => w.id === targetId)
    if (fromIdx === -1 || toIdx === -1) {
      setDragId(null)
      setDropIndicator(null)
      return
    }
    const reordered = [...workspaces]
    const [moved] = reordered.splice(fromIdx, 1)
    const adjustedToIdx = fromIdx < toIdx ? toIdx - 1 : toIdx
    const insertIdx = dropIndicator.position === 'after' ? adjustedToIdx + 1 : adjustedToIdx
    reordered.splice(insertIdx, 0, moved!)
    setDragId(null)
    setDropIndicator(null)
    try {
      await reorderWorkspaces(reordered.map((w) => w.id))
    } catch (err) {
      console.error('[workspace] reorder failed', err)
    }
  }

  const handleDragEnd = (): void => {
    setDragId(null)
    setDropIndicator(null)
  }

  return (
    <div className="flex flex-col h-full w-full">
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {workspaces.map((ws) => (
          <WorkspaceGroup
            key={ws.id}
            id={ws.id}
            name={ws.name}
            icon={ws.icon}
            sessions={workspaceSessions[ws.id] ?? []}
            isActive={activeWorkspaceId === ws.id}
            activeSessionId={activeSessionId}
            onSelectSession={onSelectSession}
            onDeleteSession={onDeleteSession}
            onSelectWorkspace={() => selectWorkspace(ws.id)}
            isDragging={dragId === ws.id}
            dropIndicator={dropIndicator?.id === ws.id ? dropIndicator.position : null}
            onDragStart={handleDragStart}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
            onDragEnd={handleDragEnd}
          />
        ))}
      </div>
      <div className="px-3 pb-2">
        <button
          onClick={() => setCreateOpen(true)}
          className={cn(
            'flex items-center gap-2 w-full px-3 py-1.5 rounded-[10px]',
            'text-[12px] text-foreground/40 hover:text-foreground/70 hover:bg-primary/5',
            'transition-colors duration-100 titlebar-no-drag'
          )}
        >
          <Plus className="h-3.5 w-3.5" />
          新建工作区
        </button>
      </div>
      <WorkspaceCreateDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={handleCreated}
      />
    </div>
  )
}
```

- [ ] **Step 6: Run test to verify passing**

`cd ui && npm test -- --run WorkspaceGroup 2>&1 | tail -15` — expect 3 tests pass.

- [ ] **Step 7: Verify build clean**

`cd ui && npx tsc --noEmit 2>&1 | head -10` — empty.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/workspace/WorkspaceGroup.tsx ui/src/components/workspace/WorkspaceGroup.test.tsx ui/src/components/workspace/WorkspaceRail.tsx ui/src/atoms/workspace.ts
git commit -m "$(cat <<'EOF'
feat(ui): WorkspaceGroup hover affordances + drag-reorder

Adds the long-missing workspace-level UI on the live UI path
(WorkspaceRail/Group), restoring Phase-1-deleted-from-WorkspaceSelector
functionality on the component users actually see (smoke finding #3).

WorkspaceGroup gets:
- Pencil button (hover-revealed, non-default only) → inline rename input;
  Enter commits, Esc cancels, blur commits. Backend update_workspace IPC
  (Task 2) refuses to rename 'default' so the button is also hidden
  client-side for ergonomics.
- Trash button (hover-revealed, non-default only) → AlertDialog confirm
  with explainer ("会话会被移动到默认工作区, 文件夹不会删除") → deleteWorkspace
  IPC (Phase 1 cascade). Default workspace's protection is enforced both
  client-side (no button) and server-side (delete_workspace returns Err).
- Drop indicators (horizontal line above/below) on drag-over.

WorkspaceRail centrally manages drag state and dispatches reorderWorkspaces
action atom on drop (Task 3 IPC).

Atom changes:
- updateWorkspaceAtom + reorderWorkspacesAtom action atoms

Inline TS test (3 assertions): hover buttons render on non-default, hidden
on default, click Pencil → inline input.

Phase 2 spec §4.7.
EOF
)"
```

---

### Task 10: Wire `MoveSessionDialog` to WorkspaceGroup session menu

Spec §4.7. `MoveSessionDialog` file exists but no JSX renders it. Add a three-dot DropdownMenu on each `SessionItem` with "移动到..." and "删除" actions. Render the dialog once at `WorkspaceRail` level (so it doesn't unmount when individual rows re-render).

**Files:**
- Modify: `ui/src/components/workspace/SessionItem.tsx` (replace `×` button with DropdownMenu)
- Modify: `ui/src/components/workspace/WorkspaceRail.tsx` (host MoveSessionDialog + state)
- Modify: `ui/src/components/agent/MoveSessionDialog.tsx` (verify props match; minor adjustments if needed)
- Test: `ui/src/components/agent/MoveSessionDialog.test.tsx` (new file)

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/agent/MoveSessionDialog.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { Provider } from 'jotai'
import { MoveSessionDialog } from './MoveSessionDialog'

const moveAgentSessionToWorkspace = vi.fn().mockResolvedValue({ id: 's-1', workspaceId: 'ws-b' })
vi.mock('@/lib/tauri-bridge', () => ({
  moveAgentSessionToWorkspace: (args: unknown) => moveAgentSessionToWorkspace(args),
}))

describe('MoveSessionDialog', () => {
  it('lists workspaces except the current one', () => {
    const workspaces = [
      { id: 'ws-a', name: 'A', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
      { id: 'ws-b', name: 'B', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
      { id: 'ws-c', name: 'C', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
    ]
    const onMoved = vi.fn()
    render(
      <Provider>
        <MoveSessionDialog
          open
          onOpenChange={() => {}}
          sessionId="s-1"
          currentWorkspaceId="ws-a"
          workspaces={workspaces}
          onMoved={onMoved}
        />
      </Provider>
    )
    // The select trigger is shown; opening it would require pointer-events polyfills
    // in jsdom which is brittle. Instead, assert the underlying filter logic by
    // confirming current workspace label "A" is not present as a selectable choice.
    // Available workspaces are still B and C (rendered inside the select component's
    // popover — we can't easily test the popover open state, but we can confirm the
    // component renders without error and the dialog is open.
    expect(screen.getByText(/移动会话到/)).toBeTruthy()
  })
})
```

(The test is light because Radix UI Select is notoriously hard to drive in jsdom — we just confirm the dialog renders with the right labels. Real coverage comes from manual smoke.)

- [ ] **Step 2: Run test to verify it fails (or compiles cleanly)**

`cd ui && npm test -- --run MoveSessionDialog 2>&1 | tail -10`

If `MoveSessionDialog` component throws on render (e.g., expects atom subscriptions that aren't mocked), fix in Step 4.

- [ ] **Step 3: Replace SessionItem's `×` with a three-dot menu**

Replace `ui/src/components/workspace/SessionItem.tsx` entirely:

```typescript
import * as React from 'react'
import { LoaderCircle, MoreHorizontal, FolderInput, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

interface SessionItemProps {
  id: string
  title: string
  titleEmoji: string
  titlePending: boolean
  isActive: boolean
  /** Whether the agent loop is currently running for this session. */
  running?: boolean
  onClick: () => void
  onDelete?: () => void
  onMove?: () => void
}

export function SessionItem({
  id,
  title,
  titleEmoji,
  titlePending,
  isActive,
  running,
  onClick,
  onDelete,
  onMove,
}: SessionItemProps): React.ReactElement {
  const hasMenu = Boolean(onDelete || onMove)
  return (
    <div
      onClick={onClick}
      className={cn(
        'group flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer',
        'text-[13px] transition-colors duration-100',
        isActive
          ? 'bg-sidebar-accent text-sidebar-primary font-medium'
          : 'text-muted-foreground hover:bg-muted hover:text-foreground'
      )}
    >
      <span className="shrink-0 inline-flex items-center justify-center text-primary" style={{ width: '18px' }}>
        {titlePending ? (
          <LoaderCircle size={14} strokeWidth={2} className="animate-spin" />
        ) : (
          <span className="text-[14px] leading-none" style={{ fontFamily: "'Noto Emoji', sans-serif" }}>
            {titleEmoji || '💬'}
          </span>
        )}
      </span>
      {titlePending ? (
        <span className="flex-1 h-3.5 rounded bg-muted-foreground/20 animate-pulse" />
      ) : (
        <span className="flex-1 truncate">{title || 'New session'}</span>
      )}
      {running && !titlePending && (
        <span
          className="shrink-0 size-1.5 rounded-full bg-primary animate-pulse shadow-[0_0_6px_hsl(var(--primary))] group-hover:opacity-0 transition-opacity"
          title="任务执行中"
        />
      )}
      {hasMenu && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              onClick={(e) => e.stopPropagation()}
              className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-foreground p-0.5 rounded"
              title="更多"
            >
              <MoreHorizontal className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-40 min-w-0 p-0.5">
            {onMove && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5"
                onSelect={onMove}
              >
                <FolderInput />
                移动到...
              </DropdownMenuItem>
            )}
            {onDelete && (
              <DropdownMenuItem
                className="text-xs py-1 [&>svg]:size-3.5 text-destructive focus:text-destructive"
                onSelect={onDelete}
              >
                <Trash2 />
                删除
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Verify MoveSessionDialog prop shape (read-only)**

Run `grep -nE "interface MoveSessionDialogProps|export function MoveSessionDialog" ui/src/components/agent/MoveSessionDialog.tsx` and read the props block to confirm:
- `open: boolean`
- `onOpenChange: (open: boolean) => void`
- `sessionId: string`
- `currentWorkspaceId: string | undefined`
- `workspaces: AgentWorkspace[]`
- `onMoved: (updatedSession: AgentSessionMeta, targetWorkspaceName: string) => void`

If the shape differs from what Task 10 expects, surface it as DONE_WITH_CONCERNS — but it likely matches per the ground-truth survey.

- [ ] **Step 5: Wire MoveSessionDialog into WorkspaceRail**

In `ui/src/components/workspace/WorkspaceRail.tsx`, add at the top:

```typescript
import { MoveSessionDialog } from '@/components/agent/MoveSessionDialog'
import { agentSessionsAtom, agentWorkspacesAtom } from '@/atoms/agent-atoms'
```

And inside the `WorkspaceRail` function, add state for the move dialog (after the dragId/dropIndicator state):

```typescript
  // Move-session dialog state
  const [moveTargetSessionId, setMoveTargetSessionId] = React.useState<string | null>(null)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const agentWorkspaces = useAtomValue(agentWorkspacesAtom)
  const moveTargetSession = moveTargetSessionId
    ? agentSessions.find((s) => s.id === moveTargetSessionId)
    : null
```

Then pass `onMove` to each `WorkspaceGroup`'s child sessions. **Key change**: `WorkspaceGroup` itself doesn't directly pass `onMove` to `SessionItem` — that's its own concern. Update `WorkspaceGroup` to accept an `onMoveSession` prop and forward it to each `SessionItem`. Modify `WorkspaceGroup.tsx`'s props:

Find the props interface in `WorkspaceGroup.tsx`:
```typescript
interface WorkspaceGroupProps {
  ...
  onDeleteSession?: (sessionId: string) => void
  ...
}
```

Add the new prop:
```typescript
  onDeleteSession?: (sessionId: string) => void
  onMoveSession?: (sessionId: string) => void
```

Forward it in the SessionItem render:
```typescript
            <SessionItem
              ...
              onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
              onMove={onMoveSession ? () => onMoveSession(s.id) : undefined}
            />
```

Don't forget to add `onMoveSession` to the destructured props at the top of the component.

Back in `WorkspaceRail.tsx`, pass it down:
```typescript
          <WorkspaceGroup
            ...
            onDeleteSession={onDeleteSession}
            onMoveSession={(sid) => setMoveTargetSessionId(sid)}
            ...
          />
```

And render the dialog at the bottom of WorkspaceRail's JSX (just before the closing `</div>`):

```typescript
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={agentWorkspaces}
          onMoved={() => {
            setMoveTargetSessionId(null)
            // Refresh workspaces + sessions; the dialog already moved the session.
            void refreshWorkspaces()
          }}
        />
      )}
```

- [ ] **Step 6: Make sure `agentWorkspacesAtom` is populated with the right shape**

`agentWorkspacesAtom` (`@/atoms/agent-atoms`) was previously empty in production (Phase 1 smoke finding). The atom shape needs `AgentWorkspace[]` for `MoveSessionDialog`. Two options:

A. Populate `agentWorkspacesAtom` at startup by listening to `workspacesAtom` and transforming `WorkspaceInfo` → `AgentWorkspace`.
B. Have `MoveSessionDialog` accept `WorkspaceInfo[]` instead (type unification deferred to Phase 3, but a one-shot adapter is fine).

**Choose A** (less invasive to MoveSessionDialog). In `ui/src/atoms/agent-atoms.ts`, the simplest population point is wherever `workspacesAtom` is set elsewhere. But since this is a Phase 2 detail and may sprawl, **the cleaner short-term fix** is to construct a transient `AgentWorkspace[]` in `WorkspaceRail` from `workspacesAtom`:

In `WorkspaceRail.tsx`, replace `const agentWorkspaces = useAtomValue(agentWorkspacesAtom)` with:

```typescript
  const agentWorkspaces: AgentWorkspace[] = React.useMemo(
    () => workspaces.map((w) => ({
      id: w.id,
      name: w.name,
      icon: w.icon,
      path: w.path,
      createdAt: Date.parse(w.createdAt) || Date.now(),
      updatedAt: Date.parse(w.updatedAt) || Date.now(),
    })),
    [workspaces],
  )
```

(Don't subscribe to `agentWorkspacesAtom`; derive from `workspacesAtom` directly.)

Add the type import at the top of WorkspaceRail.tsx:
```typescript
import type { AgentWorkspace } from '@/lib/agent-types'
```

Remove the `agentWorkspacesAtom` and `agentSessionsAtom` imports if only used here — but `agentSessionsAtom` is needed to find the session by id. Keep it.

- [ ] **Step 7: Run tests + build**

```bash
cd ui && npm test -- --run "MoveSession|WorkspaceGroup" 2>&1 | tail -15
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Tests pass, TS clean.

- [ ] **Step 8: Commit**

```bash
git add ui/src/components/workspace/SessionItem.tsx ui/src/components/workspace/WorkspaceRail.tsx ui/src/components/workspace/WorkspaceGroup.tsx ui/src/components/agent/MoveSessionDialog.test.tsx
git commit -m "$(cat <<'EOF'
feat(ui): MoveSessionDialog wired to WorkspaceGroup session menu

Closes Phase 1 smoke finding #2 (orphan dialog). SessionItem replaces
its bare × button with a three-dot DropdownMenu containing:
- "移动到..." → opens MoveSessionDialog
- "删除" → existing onDelete callback

WorkspaceRail centrally hosts the dialog state + render so it doesn't
unmount when individual rows re-render. Constructs a transient
AgentWorkspace[] from workspacesAtom rather than relying on the
deprecated agentWorkspacesAtom (type unification deferred to Phase 3).

MoveSessionDialog file unchanged — was already complete, just unwired.

Inline TS test (light — Radix Select is hard to drive in jsdom; manual
smoke covers the rest).

Phase 2 spec §4.7.
EOF
)"
```

---

### Task 11: WorkspaceFilesView — attached-dirs sections + Finder + image previews

Spec §4.7. Restore the attached-directory UI (workspace + session level) and the file actions in the right-panel Files tab. Image previews use `convertFileSrc` instead of base64.

**Files:**
- Modify: `ui/src/components/agent/SidePanel.tsx` (`WorkspaceFilesView`) — major additions
- Modify: `ui/src/components/file-browser/*` — verify `FileBrowser` supports `onAddToChat` with the new preview path (likely already does)
- Test: `ui/src/components/agent/WorkspaceFilesView.test.tsx` (new file)

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/agent/WorkspaceFilesView.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { WorkspaceFilesView } from './SidePanel'
import { workspaceAttachedDirsMapAtom, agentSessionAttachedDirsMapAtom } from '@/atoms/agent-atoms'

vi.mock('@/lib/tauri-bridge', () => ({
  attachWorkspaceDirectory: vi.fn(),
  detachWorkspaceDirectory: vi.fn(),
  attachSessionDirectory: vi.fn(),
  detachSessionDirectory: vi.fn(),
  openFolderDialog: vi.fn().mockResolvedValue(null),
  showInFinder: vi.fn(),
  openFile: vi.fn(),
}))

vi.mock('@/components/file-browser', () => ({
  FileBrowser: () => <div data-testid="file-browser" />,
  FileDropZone: () => <div data-testid="file-drop-zone" />,
}))

describe('WorkspaceFilesView', () => {
  it('renders attached-dirs section header when workspace has attached dirs', () => {
    const store = createStore()
    store.set(workspaceAttachedDirsMapAtom, new Map([['ws-x', ['/tmp/extra']]]))
    store.set(agentSessionAttachedDirsMapAtom, new Map())

    render(
      <Provider store={store}>
        <WorkspaceFilesView sessionId="s-1" sessionPath="/some/path" />
      </Provider>
    )
    // The "附加目录" section should render with /tmp/extra visible somewhere.
    expect(screen.getByText(/附加目录/)).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

`cd ui && npm test -- --run WorkspaceFilesView 2>&1 | tail -15` — expect failure (no `附加目录` text rendered yet).

- [ ] **Step 3: Update WorkspaceFilesView (SidePanel.tsx)**

Read current state of `ui/src/components/agent/SidePanel.tsx` then rewrite it. The Phase 1 cleanup left a thin shell at ~207 lines. Phase 2 restores:
- Attached-dirs sections (workspace + session)
- "在 Finder 打开" button on the file headers
- Image previews via `convertFileSrc` in `handleAddToChat`

The full rewrite is large; here is the new file content:

```typescript
/**
 * WorkspaceFilesView — RightSidePanel 的 Files tab 内容渲染
 *
 * 三个区:
 *   - 附加目录(workspace 级):用户主动 attach 的外部文件夹
 *   - 会话文件(sessionPath 存在时):当前 agent 会话的专属文件树
 *   - 工作区文件:当前工作区的共享文件树
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { FolderOpen, RefreshCw, Info, FolderHeart, Plus, X, ExternalLink, FolderPlus } from 'lucide-react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { FileBrowser, FileDropZone } from '@/components/file-browser'
import {
  agentSessionsAtom,
  agentSidePanelOpenMapAtom,
  workspaceFilesVersionAtom,
  currentAgentWorkspaceIdAtom,
  agentWorkspacesAtom,
  agentPendingFilesAtom,
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
} from '@/atoms/agent-atoms'
import { workspacesAtom } from '@/atoms/workspace'
import {
  attachWorkspaceDirectory,
  detachWorkspaceDirectory,
  attachSessionDirectory,
  detachSessionDirectory,
  openFolderDialog,
  showInFinder,
} from '@/lib/tauri-bridge'
import type { FileEntry } from '@/lib/chat-types'
import type { AgentPendingFile } from '@/lib/agent-types'

interface WorkspaceFilesViewProps {
  sessionId: string
  sessionPath: string | null
}

export function WorkspaceFilesView({ sessionId, sessionPath }: WorkspaceFilesViewProps): React.ReactElement {
  const setSidePanelOpenMap = useSetAtom(agentSidePanelOpenMapAtom)

  const filesVersion = useAtomValue(workspaceFilesVersionAtom)
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)

  const agentSessions = useAtomValue(agentSessionsAtom)
  const sessionWorkspaceId = agentSessions.find((s) => s.id === sessionId)?.workspaceId
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const currentWorkspaceId = sessionWorkspaceId ?? globalWorkspaceId

  // Workspace files path — derive from spaces.path (Task 1 added it to WorkspaceInfo).
  const workspaces = useAtomValue(workspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = workspaces.find((w) => w.id === currentWorkspaceId)
    return ws?.path ?? null
  }, [workspaces, currentWorkspaceId])

  // Attached dirs (workspace + session).
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedMap.get(currentWorkspaceId) ?? []) : []

  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const sessionAttachedDirs = sessionAttachedMap.get(sessionId) ?? []

  const handleAttachWorkspaceDir = React.useCallback(async () => {
    if (!currentWorkspaceId) return
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachWorkspaceDirectory(currentWorkspaceId, picked.path)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(currentWorkspaceId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] attach workspace dir failed', err)
    }
  }, [currentWorkspaceId, setWsAttachedMap])

  const handleDetachWorkspaceDir = React.useCallback(async (dirPath: string) => {
    if (!currentWorkspaceId) return
    try {
      const updated = await detachWorkspaceDirectory(currentWorkspaceId, dirPath)
      setWsAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(currentWorkspaceId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] detach workspace dir failed', err)
    }
  }, [currentWorkspaceId, setWsAttachedMap])

  const handleAttachSessionDir = React.useCallback(async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachSessionDirectory(sessionId, picked.path)
      setSessionAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(sessionId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] attach session dir failed', err)
    }
  }, [sessionId, setSessionAttachedMap])

  const handleDetachSessionDir = React.useCallback(async (dirPath: string) => {
    try {
      const updated = await detachSessionDirectory(sessionId, dirPath)
      setSessionAttachedMap((prev) => {
        const m = new Map(prev)
        m.set(sessionId, updated)
        return m
      })
    } catch (err) {
      console.error('[WorkspaceFilesView] detach session dir failed', err)
    }
  }, [sessionId, setSessionAttachedMap])

  // File upload version bump (triggers FileBrowser refresh).
  const handleFilesUploaded = React.useCallback(() => {
    setFilesVersion((prev) => prev + 1)
  }, [setFilesVersion])

  const handleRefresh = React.useCallback(() => {
    setFilesVersion((prev) => prev + 1)
  }, [setFilesVersion])

  // Add file to chat — image previews via convertFileSrc (Tauri asset protocol).
  const pendingFiles = useAtomValue(agentPendingFilesAtom)
  const setPendingFiles = useSetAtom(agentPendingFilesAtom)
  const handleAddToChat = React.useCallback((entry: FileEntry) => {
    if (pendingFiles.some((f) => f.sourcePath === entry.path)) return
    const ext = entry.name.split('.').pop()?.toLowerCase() ?? ''
    const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'])
    const mimeExt = ext === 'jpg' ? 'jpeg' : ext === 'svg' ? 'svg+xml' : ext
    const mediaType = imageExts.has(ext) ? `image/${mimeExt}` : 'application/octet-stream'
    const previewUrl = imageExts.has(ext) ? convertFileSrc(entry.path) : undefined

    const pending: AgentPendingFile = {
      id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      filename: entry.name,
      mediaType,
      size: 0,
      previewUrl,
      sourcePath: entry.path,
    }

    setPendingFiles((prev) => [...prev, pending])
  }, [pendingFiles, setPendingFiles])

  const breadcrumb = React.useMemo(() => {
    if (!sessionPath) return ''
    const parts = sessionPath.split('/').filter(Boolean)
    return parts.length > 2 ? `.../${parts.slice(-2).join('/')}` : sessionPath
  }, [sessionPath])

  // Auto-open right panel when files change (Phase 1 behavior preserved).
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

  // Combined attached-dirs list to render in a single section above the file browsers.
  const allAttachedDirs = React.useMemo(() => {
    const out: Array<{ path: string; scope: 'workspace' | 'session' }> = []
    for (const p of wsAttachedDirs) out.push({ path: p, scope: 'workspace' })
    for (const p of sessionAttachedDirs) out.push({ path: p, scope: 'session' })
    return out
  }, [wsAttachedDirs, sessionAttachedDirs])

  return (
    <div className="h-full flex flex-col">
      {currentWorkspaceId ? (
        <div className="flex-1 min-h-0 flex flex-col">

          {/* ===== Attached directories section ===== */}
          {(allAttachedDirs.length > 0 || currentWorkspaceId) && (
            <div className="flex-shrink-0 border-b border-border/40">
              <div className="flex items-center gap-1 px-3 pt-2 pb-1 h-[28px]">
                <FolderPlus className="size-3 text-muted-foreground" />
                <span className="text-[11px] font-medium text-muted-foreground">附加目录</span>
                <div className="flex-1" />
                {currentWorkspaceId && (
                  <button
                    type="button"
                    onClick={handleAttachWorkspaceDir}
                    className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                    title="附加目录到工作区"
                  >
                    <Plus className="size-3" />
                  </button>
                )}
                {sessionPath && (
                  <button
                    type="button"
                    onClick={handleAttachSessionDir}
                    className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                    title="附加目录到会话"
                  >
                    <Plus className="size-3" />
                  </button>
                )}
              </div>
              {allAttachedDirs.map((d) => (
                <div key={`${d.scope}:${d.path}`} className="group flex items-center gap-1 px-3 py-0.5 text-[11px]">
                  <span className="text-muted-foreground/60">{d.scope === 'workspace' ? '🌐' : '💬'}</span>
                  <span className="truncate flex-1" title={d.path}>{d.path}</span>
                  <button
                    type="button"
                    onClick={() => d.scope === 'workspace' ? handleDetachWorkspaceDir(d.path) : handleDetachSessionDir(d.path)}
                    className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive p-0.5 rounded"
                    title="移除"
                  >
                    <X className="size-3" />
                  </button>
                </div>
              ))}
            </div>
          )}

          {/* ===== Session files section ===== */}
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
                    <p>当前会话的专属文件,仅本次对话的 Agent 可以访问</p>
                  </TooltipContent>
                </Tooltip>
                <span className="text-[10px] text-muted-foreground/75 truncate flex-1" title={sessionPath}>
                  {breadcrumb}
                </span>
                <button
                  type="button"
                  onClick={() => showInFinder(sessionPath)}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="在 Finder 打开"
                >
                  <ExternalLink className="size-2.5" />
                </button>
                <button
                  type="button"
                  onClick={handleRefresh}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="刷新文件列表"
                >
                  <RefreshCw className="size-2.5" />
                </button>
              </div>
              <div className="flex-1 min-h-0 overflow-y-auto">
                <FileBrowser
                  rootPath={sessionPath}
                  hideToolbar
                  embedded
                  onAddToChat={handleAddToChat}
                />
                <FileDropZone
                  sessionId={sessionId}
                  target="session"
                  onFilesUploaded={handleFilesUploaded}
                />
              </div>
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
                  <p>工作区内所有会话可访问的文件和文件夹,每个新对话都可以自动读取</p>
                </TooltipContent>
              </Tooltip>
              <div className="flex-1" />
              {workspaceFilesPath && (
                <button
                  type="button"
                  onClick={() => showInFinder(workspaceFilesPath)}
                  className="h-5 w-5 inline-flex items-center justify-center rounded hover:bg-foreground/[0.06] text-foreground/40 hover:text-foreground/70 transition-colors"
                  title="在 Finder 打开"
                >
                  <ExternalLink className="size-2.5" />
                </button>
              )}
            </div>
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

- [ ] **Step 4: Run test + verify build**

```bash
cd ui && npm test -- --run WorkspaceFilesView 2>&1 | tail -10
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Test passes, TS clean.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/agent/SidePanel.tsx ui/src/components/agent/WorkspaceFilesView.test.tsx
git commit -m "$(cat <<'EOF'
feat(ui): WorkspaceFilesView attached-dirs sections + Finder + image previews

Restores the right-panel Files tab UI Phase 1 deleted. Now there are
three sections instead of two:

1. 附加目录 — workspace-level (globe icon) and session-level (chat icon)
   attached dir paths, with Plus button to attach via folder picker
   (Task 7 plugin) and X to detach. Shown whenever any dirs are attached
   OR workspace is selected (Plus is always available for ergonomics).
2. 会话文件 — agent session-private files. Now with "在 Finder 打开"
   button (Task 7 plugin) and refresh button.
3. 工作区文件 — workspace-shared files at workspaces.find(...).path
   (which is now populated post-Task 4 auto-mkdir). With "在 Finder 打开"
   button.

Image previews use Tauri's convertFileSrc asset protocol instead of
base64 round-trip (Phase 1 dropped the base64 path; Phase 2 uses the
right tool).

Inline TS test confirms 附加目录 header renders.

Phase 2 spec §4.7.
EOF
)"
```

---

### Task 12: AgentView re-subscribes to real attached-dirs atoms

Spec §4.7. Phase 1 stubbed `attachedDirs: string[] = []` and `workspaceFilesPath: string | null = null` in AgentView. Restore real atom subscriptions and IPC calls on folder drop / attach button.

**Files:**
- Modify: `ui/src/components/agent/AgentView.tsx`

- [ ] **Step 1: Read the current AgentView state**

`grep -nE "attachedDirs|wsAttachedDirs|workspaceFilesPath|attachSessionDirectory|attach_session|additionalDirectories|handleAttachFolder" /Users/ryanliu/Documents/uclaw/ui/src/components/agent/AgentView.tsx | head -30`

Identify the line numbers of:
- The Phase 1 stub `const attachedDirs: string[] = []` and `const wsAttachedDirs: string[] = []`
- The Phase 1 stub `const workspaceSlug: string | null = null` and `const workspaceFilesPath: string | null = null`
- The folder-drop branch (currently toast.message)
- The agent-message payload spread

- [ ] **Step 2: Add atom imports + bridge wrapper imports**

Near the top of `AgentView.tsx`, find the existing agent-atoms import block and add:

```typescript
import {
  ...
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
} from '@/atoms/agent-atoms'
```

And the bridge import block — add:
```typescript
import {
  ...
  attachSessionDirectory,
  openFolderDialog,
  ...
} from '@/lib/tauri-bridge'
```

Find the workspace atom import (probably `import { workspacesAtom } from '@/atoms/workspace'`). If not present, add it.

- [ ] **Step 3: Replace the Phase 1 stubs with real atom subscriptions**

Find lines like:
```typescript
  // Attached directories feature is gone in Phase 1 (phantom backend).
  // These constants keep prop-passing compiling without restructuring the
  // child component APIs. Phase 2 restores the data flow.
  const attachedDirs: string[] = []
  const wsAttachedDirs: string[] = []
```

Replace with:
```typescript
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const setWsAttachedMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const attachedDirs = sessionAttachedMap.get(sessionId) ?? []
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedMap.get(currentWorkspaceId) ?? []) : []
```

Find the Phase 1 stub block:
```typescript
  // Workspace shared files path: not reachable in Phase 1 (getWorkspaceFilesPath
  // is phantom; AgentWorkspace doesn't carry path). Stub to null; Phase 2 will
  // wire this up properly when AgentWorkspace gains a path field.
  const workspaceSlug: string | null = null
  const workspaceFilesPath: string | null = null
```

Replace with:
```typescript
  // Workspace shared files path: derived from spaces.path (Task 4 auto-mkdir).
  const wsList = useAtomValue(workspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = wsList.find((w) => w.id === currentWorkspaceId)
    return ws?.path ?? null
  }, [wsList, currentWorkspaceId])
  // workspaceSlug is no longer used in Phase 2 (slug field removed from
  // AgentWorkspace in Phase 1). Removed entirely.
```

Search for any remaining `workspaceSlug` usage in this file and remove those references (replace with `null` or remove the prop pass-through).

- [ ] **Step 4: Replace Phase 1 toast with real folder-drop attach**

Find the directory-handling block in `handleDrop` (look for "Folder drag is disabled in Phase 1"):

```typescript
        // Phase 1: dropping folders is a no-op until Phase 2 implements
        // attach_directory. Show a toast so users aren't confused.
        for (const dirPath of directories) {
          const dirName = dirPath.split('/').pop() || dirPath
          toast.message(`Folder drag is disabled in Phase 1: ${dirName}`)
        }
```

Replace with:
```typescript
        // Phase 2: real attach_session_directory.
        for (const dirPath of directories) {
          try {
            const updated = await attachSessionDirectory(sessionId, dirPath)
            setSessionAttachedMap((prev) => {
              const map = new Map(prev)
              map.set(sessionId, updated)
              return map
            })
            const dirName = dirPath.split('/').pop() || dirPath
            toast.success(`已附加目录: ${dirName}`)
          } catch (err) {
            console.error('[AgentView] attach directory failed', err)
          }
        }
```

- [ ] **Step 5: Restore `additionalDirectories` on agent message payload**

Find the `handleSend` (or equivalent function building the agent message payload). Look for where Phase 1 dropped the `additionalDirectories` spread. Add it back if removed:

```typescript
        ...(attachedDirs.length > 0 && { additionalDirectories: attachedDirs }),
```

(Search for `additionalDirectories` to find the right spot. If Phase 1 fully deleted it, restore the line near the other payload fields.)

- [ ] **Step 6: If a `handleAttachFolder` button-handler is needed**

Re-add the function if Phase 1 deleted it (and a JSX caller existed):

```typescript
  const handleAttachFolder = React.useCallback(async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      const updated = await attachSessionDirectory(sessionId, picked.path)
      setSessionAttachedMap((prev) => {
        const map = new Map(prev)
        map.set(sessionId, updated)
        return map
      })
      toast.success(`已附加目录: ${picked.name}`)
    } catch (err) {
      console.error('[AgentView] attach folder failed', err)
      toast.error('附加文件夹失败')
    }
  }, [sessionId, setSessionAttachedMap])
```

(If no JSX caller exists, skip — don't add dead code.)

- [ ] **Step 7: Verify build**

`cd ui && npx tsc --noEmit 2>&1 | head -10` — empty.

- [ ] **Step 8: Hydrate atoms at app startup**

This is a small but important detail. Find where `listAgentSessions()` and `listSpaces()` are called at startup (probably in `App.tsx` or a hook in `app-shell/`). After those calls succeed, populate the atom maps:

Find a startup hook (e.g., `useGlobalAgentListeners` or similar) — `grep -rn "listAgentSessions\(\)" ui/src/hooks ui/src/components/app-shell 2>/dev/null | head -5`.

In that hook, after `setAgentSessions(sessions)` (or similar), add:
```typescript
      const sessionAttachedMap = new Map<string, string[]>()
      for (const s of sessions) {
        if (Array.isArray((s as any).attachedDirs)) {
          sessionAttachedMap.set((s as any).id, (s as any).attachedDirs as string[])
        }
      }
      setAgentSessionAttachedDirsMap(sessionAttachedMap)
```

And after `setWorkspaces(spaces)`:
```typescript
      const wsAttachedMap = new Map<string, string[]>()
      for (const w of spaces) {
        if (Array.isArray((w as any).attachedDirs)) {
          wsAttachedMap.set((w as any).id, (w as any).attachedDirs as string[])
        }
      }
      setWorkspaceAttachedDirsMap(wsAttachedMap)
```

Import the atom setters from `@/atoms/agent-atoms` at the top of that file.

If you can't find the right startup hook, surface as DONE_WITH_CONCERNS — the atoms will simply remain empty until an attach action fires, which is acceptable degradation.

- [ ] **Step 9: Commit**

```bash
git add ui/src/components/agent/AgentView.tsx <other files modified for hydration>
git commit -m "$(cat <<'EOF'
feat(ui): AgentView re-subscribes to real attached-dirs atoms

Phase 1 stubbed `attachedDirs: string[] = []`, `wsAttachedDirs: ... = []`,
and `workspaceFilesPath: ... = null`. Phase 2 replaces those with real
atom subscriptions:
- attachedDirs from agentSessionAttachedDirsMapAtom (Task 6 atom)
- wsAttachedDirs from workspaceAttachedDirsMapAtom (Task 6 atom)
- workspaceFilesPath from workspacesAtom (Task 4 fills .path)

Folder-drop branch in handleDrop replaced the Phase 1 toast.message
with real attachSessionDirectory (Task 6) calls. agent-message payload
restored to include `additionalDirectories: attachedDirs` for the agent
to actually see attached paths.

Atom hydration happens at startup from list_agent_sessions (Task 6
extended return) and list_spaces (Task 1 extended SpaceResponse).

Phase 2 spec §4.7.
EOF
)"
```

---

### Task 13: Delete dead `WorkspaceSelector` mount + file

Spec §4.7. Closes Phase 1 smoke finding #1. The component was cleaned of phantom IPCs in Phase 1 Task 5 but the mount point still rendered an empty list (atom never populated). Remove it entirely.

**Files:**
- Modify: `ui/src/components/app-shell/LeftSidebar.tsx` (delete the mount around line 660)
- Delete: `ui/src/components/agent/WorkspaceSelector.tsx`

- [ ] **Step 1: Find the mount point**

```bash
grep -nE "<WorkspaceSelector|WorkspaceSelector " ui/src/components/app-shell/LeftSidebar.tsx
```

Locate the JSX line that renders `<WorkspaceSelector />` and the surrounding wrapper.

- [ ] **Step 2: Delete the JSX block**

Around line 660 of `LeftSidebar.tsx`, the JSX is:

```typescript
      {mode === 'agent' && <div className="px-3 pt-2"><WorkspaceSelector /></div>}
```

Delete this entire line.

- [ ] **Step 3: Remove the import**

In the same file, find the import line `import { WorkspaceSelector } from '@/components/agent/WorkspaceSelector'` and delete it.

- [ ] **Step 4: Delete the file**

```bash
rm ui/src/components/agent/WorkspaceSelector.tsx
```

- [ ] **Step 5: Verify no other importers**

```bash
grep -rn "WorkspaceSelector" ui/src 2>/dev/null
```

Should be empty (or only inside doc comments, which is fine).

- [ ] **Step 6: Build clean**

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
```

Empty.

- [ ] **Step 7: Commit**

```bash
git add ui/src/components/app-shell/LeftSidebar.tsx
git rm ui/src/components/agent/WorkspaceSelector.tsx
git commit -m "$(cat <<'EOF'
chore(ui): delete dead WorkspaceSelector mount + file

Closes Phase 1 smoke finding #1. The component was mounted at
LeftSidebar.tsx:660 always in agent mode but its source atom
(agentWorkspacesAtom) was never populated, so it always rendered an
empty list. The live workspace UI is WorkspaceRail (different atom
file @/atoms/workspace), which now has the rename + delete + reorder +
move-session affordances added in Tasks 9-10.

Deleted:
- LeftSidebar.tsx:660 mount (one JSX line + import)
- ui/src/components/agent/WorkspaceSelector.tsx (entire file)

No other importers exist.

Phase 2 spec §4.7.
EOF
)"
```

---

## After all 13 commits

- [ ] **Verify full test suite**

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -5
cd ui && npx tsc --noEmit 2>&1 | head -5
cd ui && npm test -- --run 2>&1 | tail -10
```

Expected: Rust 263 + ~18 new = 281 pass; TS clean; Vitest 102/103 + ~5 new = 107/108 pass (the 1 fail is pre-existing `message.test.tsx`).

- [ ] **Run manual smoke per spec §5.3**

11 manual smoke items in the spec. Highlights:

1. Backup DB: `cp ~/.uclaw/uclaw.db ~/.uclaw/uclaw.db.pre-phase2.bak`
2. Boot → V17 runs idempotently (no "V17 stmt skipped" warnings beyond expected "duplicate column" on re-runs)
3. Create workspace "Test 项目" → restart → `~/Documents/workground/test/` exists, workspace persists
4. Rename workspace → restart → name persists, slug folder unchanged
5. Drag-reorder workspaces → restart → order preserved
6. Attach external folder → restart → still attached
7. Move agent session via three-dot menu → session appears in target workspace
8. Files tab "在 Finder 打开" → Finder pops up
9. Drag image to chat → thumbnail renders via convertFileSrc
10. Delete default → no UI for it; backend would refuse (covered by Task 2 test)
11. Delete a workspace with sessions → re-home cascade still works (Phase 1 behavior)

- [ ] **Push the branch**

```bash
git push -u origin claude/workspace-phase2
```

- [ ] **Open PR with bisectable commits table**

```bash
gh pr create --title "feat(workspace): Phase 2 — backend completion + UI restoration" --body "$(cat <<'EOF'
## Summary

Phase 2 of the workspace remediation series. Spec: [`docs/superpowers/specs/2026-05-11-workspace-phase2-design.md`](docs/superpowers/specs/2026-05-11-workspace-phase2-design.md).

Restores 6 features Phase 1 deleted (workspace rename, drag-reorder, workspace+session attached directories, file actions, image previews) behind real persisting backends, and closes the 3 architectural findings from Phase 1 smoke testing.

V17 adds 2 columns to spaces and 1 to agent_sessions. New workspaces auto-mkdir under `~/Documents/workground/<slug>/`. File actions adopt `tauri-plugin-dialog` + `tauri-plugin-shell` v2. Image previews use Tauri's `convertFileSrc` asset protocol instead of base64.

Build is green at every commit (unlike Phase 1's intentionally-red middle).

## Commits (bisectable)

| # | Commit | Layer |
|---|---|---|
| 1 | `chore(db): V17 migration — sort_order + attached_dirs + backfill` | Rust + 4 tests |
| 2 | `feat(workspace): update_workspace IPC for rename + icon change` | Rust + 3 tests |
| 3 | `feat(workspace): reorder_workspaces IPC + list_spaces sort by sort_order` | Rust + 2 tests |
| 4 | `feat(workspace): auto-create slug dir on create_workspace` | Rust + 7 tests |
| 5 | `feat(workspace): attach/detach/get workspace_directory IPCs` | Rust + 4 tests |
| 6 | `feat(workspace): attach/detach/list session_directory IPCs` | Rust + 2 tests |
| 7 | `chore(deps): add tauri-plugin-dialog + tauri-plugin-shell + capabilities` | Build/deps |
| 8 | `feat(workspace): rename/move/read attached_file IPCs` | Rust + 2 tests |
| 9 | `feat(ui): WorkspaceGroup hover affordances + drag-reorder` | TS + test |
| 10 | `feat(ui): MoveSessionDialog wired to WorkspaceGroup session menu` | TS + test |
| 11 | `feat(ui): WorkspaceFilesView attached-dirs sections + Finder + image previews` | TS + test |
| 12 | `feat(ui): AgentView re-subscribes to real attached-dirs atoms` | TS |
| 13 | `chore(ui): delete dead WorkspaceSelector mount + file` | TS cleanup |

## Test plan

- [ ] `cd src-tauri && cargo test --lib` — 281 pass (was 263 + 18 new)
- [ ] `cd ui && npx tsc --noEmit` — clean
- [ ] `cd ui && npm test -- --run` — no new failures vs main
- [ ] Manual smoke per spec §5.3 (11 items on a copy of `~/.uclaw/uclaw.db`)
EOF
)"
```

---

## Spec coverage

For end-of-plan-writing self-check:

- §4.1 (V17 migration + SpaceResponse extension + frontend type extensions): Task 1
- §4.2 (update_workspace + reorder_workspaces + list_spaces ORDER BY): Tasks 2 + 3
- §4.3 (auto-derive workspace path on creation): Task 4
- §4.4 (workspace attached dirs IPCs): Task 5
- §4.5 (session attached dirs IPCs + list_agent_sessions extension): Task 6
- §4.6 (tauri-plugin-dialog + shell + file action commands): Tasks 7 + 8
- §4.7 (frontend changes — WorkspaceGroup, SessionItem, MoveSessionDialog wire, WorkspaceFilesView, AgentView, LeftSidebar mount removal): Tasks 9 + 10 + 11 + 12 + 13
- §4.8 (PR shape — 13 bisectable commits): mirrored exactly
- §5.1 (Rust tests): Tasks 1, 2, 3, 4, 5, 6, 8 each include their tests (~22 total)
- §5.2 (Vitest tests): Tasks 9 (WorkspaceGroup), 10 (MoveSessionDialog), 11 (WorkspaceFilesView) — 3 total per spec
- §5.3 (manual smoke): "After all 13 commits" section
- §3 (non-goals): respected — no FK addition, no WorkspaceService abstraction, no type unification, no Phase 4 UX
