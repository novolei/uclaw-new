# Workspace Phase 1 — 止血 (Stop the Bleeding)

**Status**: Spec
**Date**: 2026-05-11
**Author**: Ryan + Claude (uClaw repo)
**Phase**: 1 of 4 (Phase 2 = backend 补齐 / Phase 3 = 真沙箱 / Phase 4 = UX 重塑)

---

## 1. Background

A workspace architecture survey found that uClaw's workspace feature is half-implemented:

- The frontend has full UI for create / rename / delete / drag-reorder / attached-directories.
- The backend implements only a subset of the corresponding IPC commands. **17 IPC commands invoked from `tauri-bridge.ts` do not exist in `tauri_commands.rs`.**
- Several frontend wrappers swallow failures via `.catch(...)` returning fake stubs (e.g. `createAgentWorkspace` returns a generated UUID + name on failure), so users see "create / rename succeeded" but nothing persists across restart.
- `agent_sessions.space_id` has no FK to `spaces`. Sessions are silently orphaned when their workspace is deleted; `LEFT JOIN ... COALESCE(..., 'default')` masks the orphan in queries.
- `AgentWorkspace.slug` exists in the frontend type but the backend never returns it. `SidePanel.tsx` derives `workspaceSlug` from `workspaces.find(...).slug` which is always `undefined`, so the entire "attached directories" feature is gated behind `if (!workspaceSlug) return` and never executes.
- `list_spaces()` returns a synthetic in-memory `default` row when the `spaces` table is empty. It is never persisted, which causes confusing transitions when the user creates the first real workspace.

Phase 1's goal is **integrity, not features**. Stop the runtime errors and silent failures, heal data, hide what doesn't work, and leave a clean foundation for Phase 2 to add the missing backends.

## 2. Goals

1. **No phantom IPC calls reachable from any UI surface.** Every `invoke(...)` in `tauri-bridge.ts` resolves to a real `#[tauri::command]` registered in `main.rs`.
2. **No orphaned `agent_sessions`.** Existing orphans heal to `default`; future calls validate workspace existence at the IPC boundary.
3. **`default` workspace is a real row in `spaces`**, not a synthetic in-memory return.
4. **`AgentWorkspace.slug` removed.** All callers use `id`.
5. **Compile cleanly** (`cargo build`, `npx tsc --noEmit`) and existing tests pass. Add 4 new Rust tests covering the migration and validation.

## 3. Non-Goals (deferred to Phase 2+)

These are **explicitly out of scope** and must not be added during Phase 1 implementation:

- Adding a real `FOREIGN KEY` constraint between `agent_sessions.space_id` and `spaces.id` (would require SQLite table-recreate dance — risky, Phase 2 work).
- Implementing the missing IPC commands (`update_workspace`, `reorder_workspaces`, `attach_workspace_directory`, etc.). The cut UI features stay cut until Phase 2.
- Implementing `open_file` / `open_folder_dialog` / `read_attached_file` via `tauri-plugin-dialog` and `tauri-plugin-shell`.
- Workspace path-isolation / sandbox enforcement at the agent tool layer (Phase 3, C5 in survey).
- Workspace-creation modal redesign with folder picker / icon picker (Phase 4, UX).
- ⌘⌃-digit workspace switching, breadcrumb visual anchor (Phase 4, UX).

If implementation discovers a bug in non-goal territory, surface it as a separate ticket per CLAUDE.md "spin off real bugs".

## 4. Design

### 4.1 Database migration (V15)

Append a new migration to `src-tauri/src/db/migrations.rs`. **Idempotent, no schema changes.**

```sql
-- V15_workspace_default_and_orphan_heal

-- (1) Persist 'default' workspace row so list_spaces() never has to synthesize it.
INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('default', '默认工作区', '📁', NULL, datetime('now'), datetime('now'));

-- (2) Re-home agent_sessions whose space_id points at a non-existent workspace.
UPDATE agent_sessions
SET space_id = 'default'
WHERE space_id NOT IN (SELECT id FROM spaces);
```

**Rationale for `path = NULL`**: matches the synthetic default's prior behavior; `active_workspace_root()` already falls back to `state.workspace_root` when path is empty/NULL. Setting an explicit path here would create a divergence from current behavior that some downstream code may depend on.

**Rationale for `OR IGNORE`**: if the user has previously created a workspace with `id='default'` (unlikely but possible — `default` is a reserved sentinel that current code treats as a placeholder), we don't clobber it.

**Rationale for `WHERE … NOT IN (SELECT id FROM spaces)`**: subquery is evaluated after the `INSERT OR IGNORE` above, so 'default' is guaranteed to exist when this UPDATE runs. Orphaned sessions land somewhere known.

**Rerun safety**: both statements are idempotent; running V15 twice is a no-op.

### 4.2 Remove the synthetic-default fallback

In `tauri_commands.rs::list_spaces`, the branch that returns an in-memory `vec![SpaceResponse { id: "default", … }]` when the table is empty becomes dead after V15. Delete it. `list_spaces` now always reads from DB. Saves ~10 lines and one source of truth.

### 4.3 Application-layer integrity (validation + delete-cascade)

Three endpoints in `src-tauri/src/tauri_commands.rs` get edits. Behavior diverges intentionally — **automatic flows tolerate, explicit flows fail loud, destructive flows clean up first.**

**`create_agent_session(title?, channel_id?, workspace_id?)`** (`tauri_commands.rs:3763`):
- If `workspace_id` is `None`: behave as today (default to `'default'`).
- If `workspace_id` is `Some(id)` and `id` exists in `spaces`: accept.
- If `workspace_id` is `Some(id)` and `id` does NOT exist: **fall back to `'default'`**, log a warning. Reason: this is called automatically when the user clicks "新会话". A stale workspace ID in the frontend (e.g. user deleted the workspace from another device, sync race) shouldn't block session creation.

**`move_agent_session_to_workspace(session_id, target_workspace_id)`** (`tauri_commands.rs:4265`):
- If `target_workspace_id` does not exist in `spaces`: **return `Err(...)`**. Reason: this is an explicit user drag-drop or menu action; silently rerouting would surprise the user.

**`delete_workspace(id)`** (`tauri_commands.rs:4459`):
- Currently relies on `ON DELETE CASCADE` on `conversations.space_id` (FK exists). `agent_sessions.space_id` has no FK, so deleted-workspace sessions become orphans.
- Phase 1 fix: **before** the `DELETE FROM spaces`, run `UPDATE agent_sessions SET space_id='default' WHERE space_id = ?`. This is the application-layer equivalent of `ON DELETE SET DEFAULT` and matches the user choice in Q2 ("只治愈 + 应用层防守").
- Refuse deletion of `'default'` itself: return `Err`.

The session-existence check (does `session_id` correspond to a real session in `move_agent_session_to_workspace`) is out of scope — separate hardening pass.

### 4.4 Frontend cuts

The TypeScript compiler is the cut-detector: removing `tauri-bridge.ts` wrappers makes every dependent file fail compilation, surfacing every site that needs editing.

**Step 1 — `ui/src/lib/tauri-bridge.ts`: delete 17 phantom wrappers.**

| Wrapper to delete | Phantom IPC name |
|---|---|
| `createAgentWorkspace` | `create_agent_workspace` |
| `updateAgentWorkspace` | `update_agent_workspace` |
| `deleteAgentWorkspace` | `delete_agent_workspace` |
| `reorderAgentWorkspaces` | `reorder_agent_workspaces` |
| `attachWorkspaceDirectory` | `attach_workspace_directory` |
| `detachWorkspaceDirectory` | `detach_workspace_directory` |
| `getWorkspaceDirectories` | `get_workspace_directories` |
| `getWorkspaceFilesPath` | `get_workspace_files_path` |
| `attachDirectory` | `attach_directory` |
| `detachDirectory` | `detach_directory` |
| `listAttachedDirectory` | `list_attached_directory` |
| `readAttachedFile` | `read_attached_file` |
| `openAttachedFile` | `open_attached_file` |
| `showAttachedInFolder` | `show_attached_in_folder` |
| `renameAttachedFile` | `rename_attached_file` |
| `moveAttachedFile` | `move_attached_file` |
| `openFolderDialog` | `open_folder_dialog` |

`tauri-bridge.ts` should be the single source of truth for what the backend exposes. After this cut, `grep "phantom" survey list` returns nothing.

**Step 2 — `WorkspaceSelector.tsx`:**
- Replace `createAgentWorkspace(name)` with `createWorkspace(name)` (the existing wrapper for the real `create_workspace` command). The return shape differs (`{id, name, icon, path, createdAt}` vs the phantom's `{id, name, slug, createdAt, updatedAt}`); adapt the local atom update to the real shape.
- Replace `deleteAgentWorkspace(id)` with `deleteWorkspace(id)`.
- Delete the inline-rename UI (the `<input>` that appears on click; lines around handleRename / startRename).
- Delete the drag-to-reorder logic (`handleDrop`, `handleDragOver`, drop-zone visuals, the `reorderAgentWorkspaces` call site).
- Hover-state edit-icon (which triggered rename) and reorder-grip-icon: removed with their handlers.

**Step 3 — `SidePanel.tsx` (`WorkspaceFilesView`):**
- Delete the entire "Attached directories" subtree: `AttachedDirsSection`, `AttachedDirTree`, `AttachedDirItem` components and their helpers. They live in the same file (~400+ lines).
- Delete `workspaceSlug` derivation and every `if (!workspaceSlug || !currentWorkspaceId) return` guard.
- Delete `attachSessionDir`, `handleAttachFolder`, `handleSessionFoldersDropped`, `handleDetachDirectory`, `attachWorkspaceDir`, `handleAttachWorkspaceFolder`, `handleWorkspaceFoldersDropped`, `handleDetachWorkspaceDirectory` — all attached-dirs handlers.
- Delete the "在 Finder 中打开" button in the 会话文件 header (calls `openFile`, phantom).
- Delete the workspace-files header's "在 Finder 中打开工作区文件目录" button (also `openFile`).
- `handleAddToChat` (image preview path): drop the `readAttachedFile` + base64 conversion. The pending file just references the path (`sourcePath`). Image thumbnails will be unavailable until Phase 2 implements `read_attached_file` properly.
- Remove `getWorkspaceFilesPath` call. Workspace files path will be derived purely from the session's workspace via `agentSessions.find(...).workspaceId` → `workspaces.find(...).path` (or NULL → fallback to global root, which the backend handles).

**Step 4 — `AgentView.tsx`:**
- Delete the `getWorkspaceFilesPath` call (line ~390) and any state that holds its result (the displayed path may render as just the workspace name or the global-default fallback).
- Delete the two `attachDirectory` flows (lines ~649, ~729) and the `openFolderDialog` invocations they depend on.

**Step 5 — `agent-atoms.ts`:**
- Delete `agentAttachedDirectoriesMapAtom` and `workspaceAttachedDirectoriesMapAtom`. Both have zero readers/writers after steps 3-4.

**Step 6 — `agent-types.ts`:**
- Remove `slug` from `AgentWorkspace`. Compile errors will identify any remaining readers (there should be none after step 3).

### 4.5 What users will lose (transitional UX gap)

Surface this in the PR description so the team and any users on `main` know:

| Lost feature | Restored in |
|---|---|
| Workspace 重命名 | Phase 2 |
| Workspace 拖拽排序 | Phase 2 |
| Workspace 级附加目录("Agent 可读取此外部文件夹") | Phase 2 |
| 会话级附加目录 | Phase 2 |
| 文件树"在 Finder 中打开 / 重命名 / 移动 / 打开外部 / 添加到聊天的图像预览" | Phase 2 |
| Workspace files path 显式 breadcrumb (line 390 of AgentView) | Phase 2 / 4 |

These features did not actually work before Phase 1 (silent failures or dead paths), so user-perceived regression is small — the affordances disappear, but they were never functional.

## 5. Testing

### 5.1 Rust unit tests (in `src-tauri/`)

All inline `#[cfg(test)]` per repo convention. Use the existing in-memory SQLite test harness pattern (search the repo for `Connection::open_in_memory` or `setup_test_db()` to copy).

1. **`migrations::tests::v15_inserts_default_idempotent`**
   - Open in-memory DB, run all migrations through V15.
   - Assert `SELECT COUNT(*) FROM spaces WHERE id='default'` = 1.
   - Run V15 again manually. Assert count is still 1.

2. **`migrations::tests::v15_heals_orphan_agent_sessions`**
   - Open in-memory DB, run migrations through V14.
   - Insert an `agent_sessions` row with `space_id='ghost-workspace-that-does-not-exist'`.
   - Run V15.
   - Assert that session's `space_id` is now `'default'`.

3. **`tauri_commands::tests::create_agent_session_falls_back_to_default_on_unknown_workspace`**
   - Set up `AppState` with test DB at V15.
   - Call `create_agent_session(title=Some("test"), channel_id=None, workspace_id=Some("ghost"))`.
   - Assert: returns `Ok`, the new session's `space_id` is `'default'`.

4. **`tauri_commands::tests::move_agent_session_errors_on_unknown_target`**
   - Set up `AppState` at V15. Create a real session in `'default'`.
   - Call `move_agent_session_to_workspace(session_id, target_workspace_id="ghost")`.
   - Assert: returns `Err`. Session's `space_id` is still `'default'`.

5. **`tauri_commands::tests::delete_workspace_reassigns_agent_sessions_to_default`**
   - Set up `AppState` at V15. Create workspace `ws-x`. Create a session bound to `ws-x`.
   - Call `delete_workspace("ws-x")`.
   - Assert: returns `Ok`. The session's `space_id` is now `'default'`. `ws-x` no longer in `spaces`.

6. **`tauri_commands::tests::delete_workspace_refuses_default`**
   - Set up `AppState` at V15.
   - Call `delete_workspace("default")`.
   - Assert: returns `Err`. `'default'` still in `spaces`.

### 5.2 TypeScript test (optional, encouraged)

7. **`WorkspaceSelector.test.tsx`** (extend if exists, create if not):
   - Render `<WorkspaceSelector>`, click "+", type a name, submit.
   - Mock `invoke` to capture the IPC name. Assert it's called with `'create_workspace'` (real), not `'create_agent_workspace'` (phantom).

### 5.3 Manual smoke

After implementation, manual smoke per the PR description checklist:

- Boot app fresh (or `rm ~/.uclaw/uclaw.db` to force migrations from scratch).
- Confirm WorkspaceSelector lists `默认工作区` with 📁 icon.
- Create a new workspace; restart app; confirm it persists.
- In that workspace, create an agent session.
- Delete that workspace from WorkspaceSelector; confirm the agent session reappears under `'默认工作区'` (re-homed by §4.3 application-layer cascade).
- Try to delete `'默认工作区'` itself; confirm refusal (UI may not even expose this; backend should refuse regardless).
- Open Files tab in right panel; confirm only "会话文件" and "工作区文件" sections render — no "附加目录" section, no extra "X 关闭" button, no "在 Finder 中打开" buttons.

## 6. Risks & Mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| User has stale data: workspace deleted at runtime, sessions become orphaned post-Phase-1 | Low | §4.3's `delete_workspace` UPDATE handles user-initiated deletes within Phase 1. V15 heals existing orphans on first boot. The only remaining gap is direct DB tampering, which Phase 1 explicitly accepts. |
| Removing image preview from `handleAddToChat` regresses chat UX (no thumbnails) | Low | Document in PR. Phase 2 restores `read_attached_file`. |
| `createWorkspace` return shape (`{id,name,icon,path,createdAt}`) doesn't match what `agentWorkspacesAtom` expects (`{id,name,slug,createdAt,updatedAt}`) | High | Step 6 (slug removal) fixes this. Adapt the atom shape and any consumers. TS compiler enforces. |
| Tests for migrations may not exist; need to bootstrap test infrastructure | Low | Search for `setup_test_db`, `Connection::open_in_memory` in repo first. CLAUDE.md says tests are inline `#[cfg(test)]`; pattern should exist. |
| Migration runs against production DB at next launch — cannot easily roll back if something goes wrong | Medium | (1) Migration is idempotent and additive (no DROP, no ALTER COLUMN). (2) Test on a copy of `~/.uclaw/uclaw.db` before merging. (3) The V15 changes are limited to one INSERT and one UPDATE — the blast radius is contained. |

## 7. Implementation Order (PR shape)

CLAUDE.md prefers one logical change per commit. Suggested commit sequence (single PR, bisectable):

1. `chore(db): add V15 migration — persist default workspace + heal agent_session orphans` (migration + 2 unit tests)
2. `refactor(workspace): remove synthetic-default branch from list_spaces` (10-line cleanup)
3. `feat(workspace): IPC validation + delete_workspace re-homes agent_sessions to default` (validation + cascade + 4 unit tests)
4. `chore(ui): remove 17 phantom IPC wrappers from tauri-bridge` (deletion only — TS will fail to compile after this; that's intentional)
5. `feat(ui): WorkspaceSelector uses real create_workspace/delete_workspace, drop rename + reorder UI` (re-stabilizes WorkspaceSelector)
6. `feat(ui): WorkspaceFilesView drops attached-dirs + slug, keeps workspace/session file browsers` (re-stabilizes SidePanel)
7. `feat(ui): AgentView drops getWorkspaceFilesPath + attachDirectory call sites` (re-stabilizes AgentView)
8. `chore(types): remove AgentWorkspace.slug + dead atoms` (final TS cleanup; build should be green again)

Total: ~8 commits, ~600-900 LOC delta (mostly deletions). Per CLAUDE.md PR pattern (PRs #29/31/33/35/36), open as a single PR with a `## Commits (bisectable)` table referencing each commit.

## 8. Open Questions

None at design time. If implementation surfaces ambiguity (e.g. `setup_test_db` doesn't exist, `move_agent_session_to_workspace` already validates somewhere), surface inline before patching.

## 9. References

- Survey report (in chat history, 2026-05-11): catalog of phantom IPCs, schema state, and dual-identity issues.
- `CLAUDE.md` working-style guidelines, especially "Active migration registry" (V15 is the next free V-number after V14 `tool_permission_rules`).
- Prior Phase 1-style PRs in repo: #33, #41 for migration shape; #36 for TS cleanup style.
