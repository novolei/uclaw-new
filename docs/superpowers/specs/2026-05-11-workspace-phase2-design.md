# Workspace Phase 2 — 后端补齐 + UI 恢复

**Status**: Spec
**Date**: 2026-05-11
**Author**: Ryan + Claude (uClaw repo)
**Phase**: 2 of 4
**Prerequisite**: [Phase 1 spec](./2026-05-11-workspace-phase1-design.md) merged as 2f9008e on 2026-05-11

---

## 1. Background

Phase 1 surgically removed 18 phantom IPC wrappers and the UI features that depended on them: workspace rename, drag-reorder, workspace-level + session-level attached directories, file-tree "open in Finder / rename / move", and image preview when adding files to chat. Phase 1's goal was integrity-not-features.

Phase 2 restores those features by implementing the real backends. It also closes three architectural gaps surfaced by Phase 1's smoke testing:

1. **Dead `WorkspaceSelector` mount** at `LeftSidebar.tsx:660`. Its atom (`agentWorkspacesAtom`) is never populated; the actual workspace UI is `WorkspaceRail`, which reads from a separate atom file (`@/atoms/workspace`). The mount is invisible to users but adds confusion when navigating code.
2. **Orphan `MoveSessionDialog`**. The component file exists but no JSX renders it — `grep -rn "<MoveSessionDialog"` returns zero hits. Users can't move sessions between workspaces.
3. **No workspace-level affordances on `WorkspaceRail` / `WorkspaceGroup`**. Sessions have `onDelete`; workspaces have nothing. No rename, no delete, no reorder UI in the live UI.

## 2. Goals

1. **Restore 6 Phase-1-deleted features** with real persisting backends:
   - Workspace rename
   - Workspace drag-reorder
   - Workspace-level attached directories
   - Session-level attached directories
   - File-tree actions (open in Finder, rename, move file, open external app, image preview)
   - "Drag folder onto agent input → attach as directory"
2. **Each workspace gets a real on-disk directory** (`~/Documents/workground/<slug>/`), automatically created when the workspace is created. Existing NULL-path workspaces fall back to global root (unchanged behavior from Phase 1).
3. **Close 3 smoke-test findings**: delete dead `WorkspaceSelector`, wire `MoveSessionDialog`, add hover affordances on `WorkspaceGroup`.
4. **Build is green at every commit** in the PR. No intentionally-red intermediate states (unlike Phase 1 commits 6-9).
5. **Tests**: ~15 new Rust unit tests + 3-5 Vitest tests.

## 3. Non-Goals (deferred to Phase 3+)

These are **explicitly out of scope**:

- **Real FK constraint on `agent_sessions.space_id`** (SQLite table-recreate dance). Application-layer cascade from Phase 1 §4.3 stays as the only defense.
- **`WorkspaceService` Rust module abstraction**. Workspace logic continues to live in `tauri_commands.rs`. Phase 3 will extract it when the sandbox layer needs a clean interface.
- **Agent tool workspace path whitelist / sandbox enforcement**. Phase 3.
- **Type unification** between `AgentWorkspace` (`@/lib/agent-types`) and `Workspace` (`@/atoms/workspace`). Both shapes get the same new fields (`attached_dirs`, `sort_order`, `path`, `icon`) but stay as separate types. Phase 3 collapses them.
- **WorkspaceCreateDialog folder picker** (let user override the auto-derived slug directory). Phase 4 UX.
- **WorkspaceCreateDialog icon picker**. Phase 4.
- **`⌘ ⌃ 1..9` workspace switching shortcuts**. Phase 4.
- **Breadcrumb visual anchor** in LeftSidebar header. Phase 4.
- **Apple Mail style source-list layout**. Phase 4.
- **⌘P / ⌘K palette cross-workspace fuzzy file search**. Phase 4.

**Pre-existing phantoms surfaced by Phase 1 smoke testing — also out of scope for Phase 2:**
- `open_file_dialog` (file picker, distinct from `open_folder_dialog`). Opportunistically picked up if trivial during Phase 2, otherwise a separate issue.
- `get_workspace_capabilities` (MCP servers + skills query). Separate issue.

## 4. Design

### 4.1 Database migration (V17)

Append a new migration to `src-tauri/src/db/migrations.rs`. Three `ALTER TABLE` statements + one backfill `UPDATE`. Idempotent — re-running V17 against a populated DB is a no-op except for the backfill (which is also idempotent since `sort_order` is overwritten with the same value).

```sql
-- V17_workspace_path_sort_attached

-- (1) Workspace ordering. NOT NULL DEFAULT 0 means existing rows get 0;
-- the backfill below assigns unique ordering values from created_at.
ALTER TABLE spaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

-- (2) Per-workspace attached directories. JSON array of absolute paths.
ALTER TABLE spaces ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';

-- (3) Per-session attached directories. Same shape, but per agent_session.
ALTER TABLE agent_sessions ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';

-- Backfill sort_order from created_at descending (newest first = 0). Idempotent:
-- always recomputed from the timestamp, never from the existing sort_order.
UPDATE spaces SET sort_order = (
    SELECT COUNT(*) FROM spaces s2 WHERE s2.created_at > spaces.created_at
);
```

**Rationale:**
- **`ALTER ADD COLUMN` is the only schema change.** SQLite supports adding columns with defaults idempotently as long as we wrap in `IF NOT EXISTS` guards — actually SQLite's `ALTER TABLE ADD COLUMN` doesn't support `IF NOT EXISTS`. The migration uses `tracing::warn!` skip-on-error per existing repo pattern (V9, V10), so a re-run silently warns "duplicate column" and continues.
- **NOT NULL DEFAULT** means existing rows get sensible values immediately. No two-step migrate-then-populate.
- **`sort_order` backfill** correlates with `created_at DESC` (matching the original `ORDER BY created_at DESC` semantics in `list_spaces`). Newest workspace = sort_order 0.
- **JSON columns** match `agent_sessions.metadata_json` convention from V8. Stored as plain TEXT; parsed/written from Rust via `serde_json`.

**Rerun safety:** all three ALTERs may fail on second run with "duplicate column" — handled by the per-statement `tracing::warn!` skip in the migration runner.

**No FK additions.** Phase 1 §3 non-goal. Application layer continues to defend (Phase 1's `rehome_agent_sessions_to_default` in `delete_workspace`).

### 4.2 Workspace mutation commands (F1, F2)

**`update_workspace(id, name?, icon?)`** in `tauri_commands.rs`:

```rust
#[tauri::command]
pub async fn update_workspace(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    icon: Option<String>,
) -> Result<serde_json::Value, Error> {
    // Refuse to rename 'default' (sentinel protection). Icon changes are OK.
    if id == "default" && name.is_some() {
        return Err(Error::Internal("cannot rename the 'default' workspace".into()));
    }

    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    require_workspace_exists(&conn, &id)?;  // Phase 1 helper

    if let Some(n) = name.as_ref() {
        conn.execute("UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![n, chrono::Utc::now().to_rfc3339(), id])
            .map_err(Error::Database)?;
    }
    if let Some(i) = icon.as_ref() {
        conn.execute("UPDATE spaces SET icon = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![i, chrono::Utc::now().to_rfc3339(), id])
            .map_err(Error::Database)?;
    }
    // Return the updated row
    let row: (String, String, String, Option<String>, String, String) = conn.query_row(
        "SELECT id, name, icon, path, created_at, updated_at FROM spaces WHERE id = ?1",
        rusqlite::params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
    ).map_err(Error::Database)?;
    Ok(serde_json::json!({
        "id": row.0, "name": row.1, "icon": row.2, "path": row.3,
        "createdAt": row.4, "updatedAt": row.5
    }))
}
```

**`reorder_workspaces(ordered_ids)`**:

```rust
#[tauri::command]
pub async fn reorder_workspaces(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let tx = conn.unchecked_transaction().map_err(Error::Database)?;
    for (idx, id) in ordered_ids.iter().enumerate() {
        require_workspace_exists(&conn, id)?;
        tx.execute("UPDATE spaces SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![idx as i64, id])
            .map_err(Error::Database)?;
    }
    tx.commit().map_err(Error::Database)?;
    Ok(())
}
```

**`list_spaces` updated** to `ORDER BY sort_order ASC` (replaces `created_at DESC` from Phase 1).

### 4.3 Auto-derive workspace path on creation (`create_workspace`)

Update `create_workspace` body to mkdir under workground root if `path` arg is None:

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

    let resolved_path: String = match path {
        Some(p) if !p.trim().is_empty() => p,
        _ => {
            let slug = slugify(&name);
            let dir = state.workspace_root.join(&slug);
            std::fs::create_dir_all(&dir).map_err(|e| Error::Internal(format!("mkdir failed: {}", e)))?;
            dir.to_string_lossy().into_owned()
        }
    };

    // ... existing INSERT, but include `path = ?` with resolved_path ...
    // Compute new sort_order as MAX(sort_order)+1 so the new workspace lands at the end.
}

fn slugify(name: &str) -> String {
    // Simple ASCII slug. For Chinese names, transliterate or take first 8 chars + UUID prefix.
    // Truncate at 32 chars. Replace non-alphanumeric with '-'. Collapse repeats.
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars().take(32).collect::<String>()
}
```

**Edge cases:**
- Empty slug after slugification (e.g., name is all Chinese chars stripped to `""` then trimmed): fall back to `format!("workspace-{}", &id[..8])`.
- `create_dir_all` already-exists is a no-op (Rust returns Ok), so re-runs are safe.

**`update_workspace` does NOT rename the directory.** Users can rename via UI but the underlying folder keeps its slug forever. Trade-off: simpler implementation, no broken file references in agent context. Phase 4 may add a "rename folder too" affordance behind a confirm dialog.

### 4.4 Attached directories — workspace level (F3)

Three commands, all operating on `spaces.attached_dirs` JSON column:

```rust
#[tauri::command]
pub async fn get_workspace_directories(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let json: String = conn.query_row(
        "SELECT attached_dirs FROM spaces WHERE id = ?1",
        rusqlite::params![workspace_id],
        |r| r.get(0),
    ).map_err(Error::Database)?;
    serde_json::from_str::<Vec<String>>(&json)
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
    let mut dirs: Vec<String> = serde_json::from_str(&conn.query_row(
        "SELECT attached_dirs FROM spaces WHERE id = ?1",
        rusqlite::params![&workspace_id],
        |r| r.get::<_, String>(0),
    ).map_err(Error::Database)?).unwrap_or_default();

    if !dirs.contains(&dir_path) {
        dirs.push(dir_path);
        conn.execute(
            "UPDATE spaces SET attached_dirs = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![
                serde_json::to_string(&dirs).unwrap(),
                chrono::Utc::now().to_rfc3339(),
                workspace_id,
            ],
        ).map_err(Error::Database)?;
    }
    Ok(dirs)
}

#[tauri::command]
pub async fn detach_workspace_directory(
    state: State<'_, AppState>,
    workspace_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    // Same shape: read JSON, filter out dir_path, write back, return new list.
    // No-op when dir_path isn't in the list (returns the current list unchanged).
}
```

**Helper extraction:** the JSON-load/modify/store pattern repeats. Extract once:

```rust
fn modify_attached_dirs<F>(
    conn: &rusqlite::Connection,
    table: &str,        // "spaces" or "agent_sessions"
    id_col: &str,       // "id" for both
    id: &str,
    f: F,
) -> Result<Vec<String>, Error>
where F: FnOnce(Vec<String>) -> Vec<String>
{
    let json: String = conn.query_row(
        &format!("SELECT attached_dirs FROM {} WHERE {} = ?1", table, id_col),
        rusqlite::params![id], |r| r.get(0),
    ).map_err(Error::Database)?;
    let dirs: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
    let new_dirs = f(dirs);
    conn.execute(
        &format!("UPDATE {} SET attached_dirs = ?1, updated_at = ?2 WHERE {} = ?3", table, id_col),
        rusqlite::params![
            serde_json::to_string(&new_dirs).unwrap(),
            chrono::Utc::now().to_rfc3339(),
            id,
        ],
    ).map_err(Error::Database)?;
    Ok(new_dirs)
}
```

(Note: `updated_at` column types differ — `spaces.updated_at` is TEXT/RFC3339; `agent_sessions.updated_at` is INTEGER timestamp_millis. The helper takes a `&str` and the caller picks the right format. Adjust function signature accordingly.)

### 4.5 Attached directories — session level (F4)

Mirror of §4.4 with `agent_sessions` table instead of `spaces`. Three commands:

- `attach_session_directory(session_id, dir_path)`
- `detach_session_directory(session_id, dir_path)`
- `list_session_directories(session_id)`

Shape identical to §4.4. The helper from §4.4 handles both tables via the `table` / `id_col` parameters.

**`list_agent_sessions`** is updated to include `attached_dirs` in its returned JSON for each session, so the frontend can hydrate `agentSessionAttachedDirsMapAtom` from a single startup call.

### 4.6 File action commands (F5)

Adopt two Tauri 2 plugins:

**`src-tauri/Cargo.toml`:**
```toml
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
```

**`src-tauri/src/main.rs`:**
```rust
.plugin(tauri_plugin_dialog::init())
.plugin(tauri_plugin_shell::init())
```

**`src-tauri/capabilities/migrated.json`** (or wherever the capability file lives — check `tauri.conf.json`):
```json
{
  "permissions": [
    "dialog:default",
    "shell:allow-open"
  ]
}
```

**Frontend usage** in `tauri-bridge.ts`:
```typescript
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { open as openShell, openPath } from '@tauri-apps/plugin-shell'

export const openFolderDialog = async (): Promise<{ path: string; name: string } | null> => {
  const selected = await openDialog({ directory: true, multiple: false })
  if (!selected || typeof selected !== 'string') return null
  return { path: selected, name: selected.split('/').pop() ?? selected }
}

export const openExternal = (url: string) => openShell(url)  // URLs

export const openFile = (path: string) => openPath(path)      // OS-default app

export const showInFinder = (path: string) => openPath(path.substring(0, path.lastIndexOf('/')))
// (macOS will reveal containing folder when opening with no leaf file)
// Alternative: use tauri::path::BaseDirectory and a custom Rust command for true reveal.
```

(`tauri-plugin-shell`'s `revealItemInDir` is not in v2 stable as of this writing — verify version. Fallback: a tiny Rust command that calls `std::process::Command::new("open").arg("-R").arg(path)` on macOS.)

**Self-rolled IPCs** for `attached_file` operations (not in plugins):

```rust
#[tauri::command]
pub async fn rename_attached_file(path: String, new_name: String) -> Result<String, Error> {
    let p = std::path::Path::new(&path);
    let parent = p.parent().ok_or_else(|| Error::Internal("no parent dir".into()))?;
    let new_path = parent.join(&new_name);
    std::fs::rename(&p, &new_path).map_err(|e| Error::Internal(format!("rename: {}", e)))?;
    Ok(new_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn move_attached_file(path: String, dest_dir: String) -> Result<String, Error> {
    let p = std::path::Path::new(&path);
    let fname = p.file_name().ok_or_else(|| Error::Internal("no filename".into()))?;
    let new_path = std::path::Path::new(&dest_dir).join(fname);
    std::fs::rename(&p, &new_path).map_err(|e| {
        // Cross-volume? Fall back to copy + delete.
        if e.kind() == std::io::ErrorKind::CrossesDevices {
            std::fs::copy(&p, &new_path).and_then(|_| std::fs::remove_file(&p))
                .map_err(|e2| Error::Internal(format!("cross-volume move: {}", e2)))
                .unwrap_or_else(|e| panic!("{}", e));
        }
        Error::Internal(format!("move: {}", e))
    })?;
    Ok(new_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn read_attached_file(path: String) -> Result<Vec<u8>, Error> {
    std::fs::read(&path).map_err(|e| Error::Internal(format!("read: {}", e)))
}
```

**Image preview** in the frontend uses `convertFileSrc` from `@tauri-apps/api/core` instead of base64:
```typescript
import { convertFileSrc } from '@tauri-apps/api/core'
// In WorkspaceFilesView when adding image to chat:
const previewUrl = convertFileSrc(entry.path)  // tauri://localhost/... protocol
```

This bypasses base64 entirely. Tauri's asset protocol serves the file directly to the WebView. Massive speedup vs the Phase 1-era base64 round-trip.

### 4.7 Frontend changes

**`ui/src/lib/agent-types.ts`** — `AgentWorkspace` regains `path` and `icon` (Phase 1 only dropped `slug`; path and icon were never present here even though backend returns them):

```typescript
export interface AgentWorkspace {
  id: string
  name: string
  icon: string                            // new
  path: string | null                     // new
  attachedDirs?: string[]                 // new — from spaces.attached_dirs
  sortOrder?: number                      // new — from spaces.sort_order
  createdAt: number
  updatedAt: number
}
```

**`ui/src/atoms/workspace.ts`** (the live atom file consumed by `WorkspaceRail`) — its `Workspace` type gets the same new fields. Atom unification deferred to Phase 3.

**`ui/src/atoms/agent-atoms.ts`** — reintroduce the two attached-dirs atoms with **new names** so grep can tell Phase 2's from Phase 1's deleted ones:

```typescript
export const workspaceAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())
export const agentSessionAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())
```

Populated at startup from `list_spaces` + `list_agent_sessions` returns. Updated optimistically by `attach_*`/`detach_*` callers.

**`ui/src/components/workspace/WorkspaceGroup.tsx`** — three additions:
1. Hover buttons on the workspace-row right side: `<Pencil>` (rename) + `<Trash2>` (delete, gated by `id !== 'default'`).
2. Inline rename mode: click pencil → label replaced by `<input>` → Enter saves, Escape cancels (matches Phase 1's deleted WorkspaceSelector pattern).
3. Drag-and-drop reorder: wrap each row in `draggable`. On drop, compute new order, optimistic update, call `reorder_workspaces` with full ordered ID array. Drop indicators (horizontal line above/below) visible during drag.

**Session-row affordances inside `WorkspaceGroup`** — three-dot menu:
1. `MoreHorizontal` icon hover-visible on the right of each session row.
2. Click opens a `DropdownMenu` with items: "重命名" (already has via existing flow, double-check), "移动到...", "删除", "归档".
3. "移动到..." sets a target session ID + opens `MoveSessionDialog`.

**`ui/src/components/agent/MoveSessionDialog.tsx`** — wire it up. The component already exists (orphan). Verify its props interface, then:
1. From `WorkspaceGroup`, lift `moveTargetId` state (or use a context/atom).
2. Render `<MoveSessionDialog open={moveTargetId !== null} sessionId={moveTargetId} currentWorkspaceId={...} workspaces={...} onMoved={...} onOpenChange={...} />` near top of `WorkspaceRail` (so it doesn't unmount when individual rows re-render).
3. Inside, the dialog lists workspaces except the current one. On click, call `move_agent_session_to_workspace` (real IPC, has Phase 1 strict validation).

**`ui/src/components/agent/WorkspaceFilesView.tsx`** (formerly SidePanel) — three additions:
1. **`AttachedDirsSection`** for the workspace level (when `workspaceFilesPath` is non-null) and for the session level (when `sessionPath` is non-null). Component is recovered in spirit from Phase 1-era code (commit `03bdfe2`'s parent) but rebuilt against the new atom names and real IPC wrappers. Tree depth control, hover detach button, drag-handle for re-attaching elsewhere — keep functional minimum, defer Phase 1's right-click-menu fancy bits.
2. **"在 Finder 打开"** button on file rows: `<Button onClick={() => showInFinder(entry.path)}>`.
3. **Image previews**: when entry's extension is in `imageExts`, set `previewUrl = convertFileSrc(entry.path)` and render `<img src={previewUrl}>`. No fetch/encode.

**`ui/src/components/agent/AgentView.tsx`** — undo Phase 1's stub:
1. `attachedDirs: string[] = []` stub → real atom subscription `useAtomValue(agentSessionAttachedDirsMapAtom).get(sessionId) ?? []`.
2. Similarly `wsAttachedDirs` → `useAtomValue(workspaceAttachedDirsMapAtom).get(currentWorkspaceId) ?? []`.
3. `workspaceFilesPath` stub → `workspaces.find(w => w.id === currentWorkspaceId)?.path ?? null`.
4. `handleDrop` directory branch toast → real call `attach_session_directory(sessionId, dirPath)`.
5. agent message payload — restore `...(attachedDirs.length > 0 && { additionalDirectories: attachedDirs })`. **Verify the backend `send_agent_message` actually reads this field**; if it doesn't, open a separate issue (don't block Phase 2 on it).

**`ui/src/components/app-shell/LeftSidebar.tsx`** — delete L660 `<WorkspaceSelector />` mount + its surrounding `<div>`. The remaining JSX still renders `WorkspaceRail` correctly.

**Delete `ui/src/components/agent/WorkspaceSelector.tsx`** entirely (Phase 1 cleaned its phantom IPCs but the component is now dead UI per smoke testing).

### 4.8 PR shape

13 commits in one PR. Each commit leaves the build green. No intentionally-red intermediate states.

| # | Subject | Layer |
|---|---|---|
| 1 | `chore(db): V17 migration — sort_order + attached_dirs + backfill` | Rust + 4 tests |
| 2 | `feat(workspace): update_workspace IPC + tests` | Rust + 3 tests |
| 3 | `feat(workspace): reorder_workspaces + list_spaces sort + tests` | Rust + 2 tests |
| 4 | `feat(workspace): auto-create slug dir on create_workspace + tests` | Rust + 2 tests |
| 5 | `feat(workspace): attach/detach/get workspace directory IPCs + tests` | Rust + 3 tests |
| 6 | `feat(workspace): attach/detach/list session directory IPCs + tests` | Rust + 2 tests |
| 7 | `chore(deps): add tauri-plugin-dialog + shell + capabilities` | Build/config |
| 8 | `feat(workspace): rename/move/read attached_file IPCs + tests` | Rust + 2 tests |
| 9 | `feat(ui): WorkspaceGroup hover affordances (rename + delete + drag)` | TS |
| 10 | `feat(ui): MoveSessionDialog wired to WorkspaceGroup session menu` | TS |
| 11 | `feat(ui): WorkspaceFilesView attached-dirs sections + open-in-Finder + convertFileSrc image previews` | TS |
| 12 | `feat(ui): AgentView re-subscribes to real attached-dirs atoms` | TS |
| 13 | `chore(ui): delete dead WorkspaceSelector mount and file` | TS |

**Total estimate:** ~1300 LOC delta. Rust ~600 (commits 1-8); TS ~700 (commits 9-13).

**Why not split:** the after-commit-8 state has all backends working but zero UI consuming them — main is in an awkward "feature complete in DB but invisible" state. One PR keeps the integration honest.

## 5. Testing

### 5.1 Rust unit tests (~18 tests)

Add `mod workspace_phase2_tests` near the existing `workspace_integrity_tests` in `tauri_commands.rs`. Reuse the `fresh_db()` helper but apply V17 too.

**V17 migration:**
1. `v17_adds_sort_order_column_idempotent`
2. `v17_adds_workspace_attached_dirs_column`
3. `v17_adds_session_attached_dirs_column`
4. `v17_backfills_sort_order_from_created_at` — 3 workspaces with descending created_at → sort_order 0/1/2

**`update_workspace`:**
5. `update_workspace_changes_name`
6. `update_workspace_refuses_to_rename_default` — Err returned, row unchanged
7. `update_workspace_allows_icon_change_on_default` — icon updates, name preserved

**`reorder_workspaces`:**
8. `reorder_workspaces_sets_sort_order_by_array_index`
9. `reorder_workspaces_errors_on_unknown_id_no_partial_writes` — transaction rolls back

**`create_workspace` auto-mkdir:**
10. `create_workspace_creates_directory_under_workground` — uses `tempfile::TempDir` to mock `state.workspace_root`
11. `create_workspace_slugifies_chinese_name_with_fallback` — name `"我的项目"` produces something usable (probably `workspace-<uuid8>` fallback)
12. `create_workspace_respects_explicit_path_arg` — when caller passes path, no mkdir

**Attached dirs (helper-level, shared between workspace + session):**
13. `attach_directory_appends_idempotent` — second attach of same path no-ops
14. `detach_directory_removes_existing`
15. `detach_directory_noop_when_missing`
16. `get_directories_returns_empty_array_for_fresh_row`

**File actions:**
17. `rename_attached_file_renames_in_place` — uses `tempfile::TempDir`
18. `move_attached_file_moves_to_destination`

Cross-volume move test deferred to manual smoke (mocking cross-volume is brittle).

### 5.2 Vitest tests (3-5)

19. `WorkspaceGroup.test.tsx` — hover pencil → click → renames via `updateWorkspace` mock; trash hidden when `id === 'default'`
20. `MoveSessionDialog.test.tsx` — lists non-current workspaces; clicking one calls `moveAgentSessionToWorkspace` with correct args
21. `WorkspaceFilesView.test.tsx` — attached-dirs section renders from atom; clicking attach → calls `attachWorkspaceDirectory`

(Drag-reorder TS test deferred — jsdom DnD is unreliable.)

### 5.3 Manual smoke (PR description checklist)

1. **Backup**: `cp ~/.uclaw/uclaw.db ~/.uclaw/uclaw.db.pre-phase2.bak`
2. **Boot** → V17 runs idempotently (no warnings about V17 stmt skipped beyond the expected "duplicate column" once)
3. **Create workspace "Test 项目"** → restart → `~/Documents/workground/test/` (or similar slugified path) exists, workspace persists with that path
4. **Rename workspace** "Test 项目" → "Test Renamed" → restart → name persists, underlying folder still `~/Documents/workground/test/` (slug doesn't follow rename)
5. **Drag-reorder workspaces** → restart → order preserved
6. **Attach external folder** `/Users/ryanliu/Desktop/foo` to a workspace → restart → attached dirs list shows it; Files tab right panel shows the dir
7. **Move agent session** from workspace A → workspace B via three-dot menu → session appears under B
8. **Files tab: "在 Finder 打开"** on a file → macOS Finder pops up showing the file
9. **Drag image into chat** from Files tab → thumbnail renders (via convertFileSrc, instant)
10. **Try to delete default workspace** via UI → no UI exists (canDelete protects); backend confirmed by Rust test 6
11. **Delete a workspace with sessions in it** → Phase 1 cascade still works (sessions re-home to default)

## 6. Risks & Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Slugify produces empty string for all-Chinese names | Medium | Fallback to `workspace-<uuid8>` (test 11 covers) |
| Cross-volume `std::fs::rename` fails in production | Low | Fallback to copy-then-delete in `move_attached_file` (manual smoke covers macOS APFS edge cases) |
| `tauri-plugin-dialog` / `tauri-plugin-shell` v2 API drift between minor versions | Low | Pin to specific minor version in Cargo.toml; verify on first build |
| `revealItemInDir` not available in v2 stable | Medium | Fallback to spawning `open -R <path>` via `std::process::Command` in a custom Rust command. If macOS-only support is OK, this is fine. |
| Atom name clash between Phase 1's deleted atoms and Phase 2's new ones | Low | New names have clarifying suffix (`workspaceAttachedDirsMapAtom` vs Phase 1's `workspaceAttachedDirectoriesMapAtom`) |
| `additionalDirectories` field on agent message payload not actually read by backend | Medium | Verify in commit 12. If backend ignores, open separate issue; don't block Phase 2. Document in PR description. |
| Two parallel workspace types (`AgentWorkspace` + `Workspace`) drift further in Phase 2 | Low | Both add the same fields. Phase 3 collapses. Add an inline TODO comment in `agent-types.ts` to track. |
| `create_workspace_creates_directory_under_workground` test needs to mock `state.workspace_root` which may require AppState restructuring | Medium | If non-trivial, extract `compute_workspace_path(workspace_root, name, explicit_path)` as a pure fn and test that directly. AppState mocking only for integration. |

## 7. Open Questions

None at design time. Implementation may surface:
- Exact API shape of `tauri-plugin-dialog` v2 (`open` returns `null | string | string[]`?)
- Whether `revealItemInDir` exists in v2 stable
- Whether `convertFileSrc` works for paths outside the app's allowed roots (likely needs Tauri capabilities config)

These get resolved during commit 7's plugin integration and surfaced via inline comments + test failures.

## 8. References

- Phase 1 spec: [`2026-05-11-workspace-phase1-design.md`](./2026-05-11-workspace-phase1-design.md)
- Phase 1 PR #75 (merged 2f9008e 2026-05-11): https://github.com/novolei/uclaw-new/pull/75
- Phase 1 smoke findings comment: https://github.com/novolei/uclaw-new/pull/75#issuecomment-4416539909
- CLAUDE.md "Active migration registry" — V16 is Phase 1; V17 is this Phase
- Tauri 2 plugin docs: https://v2.tauri.app/plugin/dialog/ and https://v2.tauri.app/plugin/shell/
- `convertFileSrc` Tauri asset protocol: https://v2.tauri.app/reference/javascript/api/namespacecore/#convertfilesrc
