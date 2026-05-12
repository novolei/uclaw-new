# Files-Rail Proma Alignment Design

**Branch:** `claude/files-rail-proma-alignment` (off `main` at `38fb5c5`)
**Reference:** [`ErlichLiu/Proma`](https://github.com/ErlichLiu/Proma) — `apps/electron/src/renderer/components/agent/SidePanel.tsx:370-470` (panel composition) and `apps/electron/src/renderer/components/file-browser/FileBrowser.tsx:600-750` (3-dot menu).

## Goal

Restructure the files-rail UI to mirror Proma's "工作区文件" panel: one header, two grouped subsections (附加目录 + 工作文件), footer drop-zone with explicit attach affordances, and a per-file hover 3-dot menu surfacing five actions. The current per-mount section pattern collapses into one cohesive panel.

## Architecture

Today's composition (post-W3):

```
FilesRail
└── WorkspaceFilesPanel
    └── MountSection × N           ← per-mount header (label + Lock + RefreshCw) + tree
        └── FileTreeNode (recursive)
```

Target composition:

```
FilesRail
└── WorkspaceFilesPanel
    ├── WorkspacePanelHeader        ← title + ⓘ tooltip + ↻ refresh-all + ↗ reveal-in-Finder
    ├── AttachedDirsSection         ← only renders if attached dirs exist
    │   ├── AttachedDirsSubtitle    ← "附加目录（Agent 可以读取并操作此外部文件夹）"
    │   └── AttachedDirRow × N      ← collapsible row, Lock badge, hosts 3-dot menu
    │       └── FileTreeNode (recursive, on expand)
    ├── WorkspaceFilesSection       ← always renders
    │   ├── WorkspaceFilesSubtitle  ← "工作文件（存储于该工作区目录）" — only shown when 附加目录 non-empty
    │   └── FileTreeNode × N        ← flat list at workspace root
    └── WorkspacePanelFooter        ← two side-by-side buttons: 添加文件 / 附加文件夹
```

The existing `MountSection` component is **removed**. Its responsibilities split between `AttachedDirRow` (collapsible row + Lock + hover menu host) and the flat `FileTreeNode` rendering under `WorkspaceFilesSection`. Refresh moves to the header. The watcher (`useFilesRailWatcher`) wiring per mount is preserved — each `AttachedDirRow` and the workspace root each call `filesRailWatchStart` for their mount.

## File Structure

**Create:**
- `ui/src/components/files-rail/workspace/WorkspacePanelHeader.tsx` — title row.
- `ui/src/components/files-rail/workspace/AttachedDirsSection.tsx` — wrapper for attached dirs.
- `ui/src/components/files-rail/workspace/AttachedDirRow.tsx` — single attached-dir row + nested tree.
- `ui/src/components/files-rail/workspace/WorkspaceFilesSection.tsx` — wrapper for workspace root files.
- `ui/src/components/files-rail/workspace/WorkspacePanelFooter.tsx` — footer buttons.
- `ui/src/components/files-rail/workspace/FileRowMenu.tsx` — 3-dot DropdownMenu host (5 items).
- `ui/src/components/files-rail/workspace/RenameInput.tsx` — inline rename input with validation.
- `ui/src/components/files-rail/workspace/MoveToDialog.tsx` — folder-picker dialog for 移动到…
- `ui/src/components/files-rail/workspace/DeleteConfirmDialog.tsx` — shadcn AlertDialog wrapper.
- `ui/src/lib/files-rail-helpers.ts` — `spaceIdForMount` and friends.
- `ui/src/atoms/files-rail-row-atoms.ts` — atoms scoped to row-level UI state (rename target, move target, delete target).
- `ui/src/components/files-rail/workspace/FileRowMenu.test.tsx` — menu rendering + read-only gating.
- `ui/src/components/files-rail/workspace/RenameInput.test.tsx` — Enter/Esc + duplicate-name guard.
- `ui/src/lib/files-rail-helpers.test.ts` — `spaceIdForMount` parsing for all mount ID forms.

**Modify:**
- `ui/src/components/files-rail/workspace/WorkspaceFilesPanel.tsx` — new composition; no longer renders `MountSection`.
- `ui/src/components/files-rail/workspace/FileTreeNode.tsx` — host `FileRowMenu` on hover, accept "in-rename" flag.
- `ui/src/components/agent/SidePanel.tsx` — drop the existing top-level "添加目录" handler/button (moves to footer); keep workspace cwd plumbing.
- `ui/src/components/chat/AttachmentPreviewItem.tsx` — replace the ext-badge stop-gap with `<FileTypeIcon>` so chat-composer chips share the rail's icon vocabulary (see Section 8a).

**Delete:**
- `ui/src/components/files-rail/workspace/MountSection.tsx` — superseded by `AttachedDirRow` + `WorkspaceFilesSection`.

## Section 1: WorkspacePanelHeader

A single horizontal row at the top of the panel.

**Layout:** `FolderHeart` icon | `工作区文件` label | `Info` (ⓘ) tooltip trigger | flex-spacer | `RotateCw` button | `ExternalLink` button.

**Behaviors:**
- ⓘ tooltip body: `工作区内所有会话可访问的文件和文件夹，每个新对话都可以自动读取` (verbatim from Proma).
- ↻ refresh: bumps `filesRailRefreshTickAtom` (already exists at `files-rail-atoms.ts:47`). `useFileTree` doesn't subscribe today — add a 1-line `useAtomValue(filesRailRefreshTickAtom)` + include it as a useEffect dep so every visible mount refetches when the tick bumps. Spinning animation while any mount in `mountRootsAtomFamily(sessionId)` has `fileTreeAtomFamily(m.id).status === 'loading'`.
- ↗ Finder: calls `invoke('reveal_path_in_file_manager', { path: workspaceRoot })`. Disabled when workspace root is unresolved.

**Styling tokens:** `h-[36px] px-3 border-b border-border bg-popover`. Title `text-[12px] font-medium text-foreground/85`. Icons `size-3.5`. No hardcoded hex — CLAUDE.md theme-tokens rule applies throughout.

## Section 2: AttachedDirsSection + AttachedDirRow

`AttachedDirsSection` is rendered only when at least one mount has `kind === 'attached_dir'`. It renders:

```
附加目录（Agent 可以读取并操作此外部文件夹）   ← muted-foreground subtitle, 11px, px-3 pt-2
▶ folder-1                                    ← AttachedDirRow (collapsed)
▼ folder-2  🔒  …                             ← AttachedDirRow (expanded, hovered)
    ├── nested-tree (FileTreeNode children)
▶ folder-3
```

`AttachedDirRow` is a single attached-dir item:

- **Chevron**: `ChevronRight`, rotates 90° when expanded. Toggles via `expandedPathsAtomFamily(mount.id)` with the sentinel `''` key (top-level expand state for the mount; child folder expansion uses the existing relPath-keyed mechanism).
- **Folder icon**: `<FileTypeIcon name={mount.label} isDirectory />` — picks up `@react-symbols/icons` folder glyph by name.
- **Label**: `mount.label`, truncate with title=full path.
- **Lock badge**: tiny `Lock` icon (size-2.5, muted) shown when `mount.editable === false`. Tooltip: `只读 — 编辑此挂载点需要批准`.
- **3-dot menu on the row itself** (hover-only, `invisible group-hover:visible`): only 在文件夹中显示 is offered. The attached-dir row label is the *mount label* — renaming it would mean renaming the on-disk folder while it's still registered as a mount, which the backend doesn't support. Out of scope.
- **Children**: when expanded, render `<FileTreeNode>` recursively under this row. Each child file/dir gets its own full 3-dot menu (gated by `mount.editable` per the read-only rules table below — for `kind === 'attached_dir'` this means items 3/4/6 are disabled, but items 1/2 work).

The watcher (`useFilesRailWatcher` + `filesRailWatchStart`) is mounted **inside** `AttachedDirRow` so it only watches expanded dirs. Collapsing unregisters the watcher to save FS resources.

## Section 3: WorkspaceFilesSection

Always rendered (the workspace mount always exists for an active workspace).

**Subtitle:** `工作文件（存储于该工作区目录）`, shown only when `AttachedDirsSection` rendered above it. Same typography as 附加目录 subtitle.

**Body:** flat `FileTreeNode` list at the workspace mount root. Re-uses the existing `useFileTree(workspaceMount.id, sessionId)` hook for the tree state. The watcher (`useFilesRailWatcher` + `filesRailWatchStart` for the workspace mount) is mounted at this section's root — the workspace tree is always visible when the panel is open, so unlike `AttachedDirRow` there's no collapse gating.

**Empty state** (workspace ready + zero nodes + no attached dirs above):

```
工作区还没有文件 — 用下方的「添加文件」或「附加文件夹」开始
```

Single line, `text-[12px] text-muted-foreground` at `px-3 py-3`. If attached dirs exist above, suppress this empty-state copy (the panel isn't actually empty).

## Section 4: WorkspacePanelFooter

Two side-by-side buttons at the bottom of the panel. Equal width (50/50 split via `grid grid-cols-2 gap-2`). Tall touch targets, dashed border, muted background — matches Proma screenshot.

| Button | Icon | Action |
|---|---|---|
| 添加文件 | `Paperclip` | `openFileDialog()` → `copyFileIntoWorkspace(workspaceId, src)` per picked file |
| 附加文件夹 | `FolderPlus` | `openFolderDialog()` → `attachWorkspaceDirectory(workspaceId, picked.path)` |

Both call existing helpers. Errors surface via sonner toasts. After success, push the updated `attached_dirs` list to `workspaceAttachedDirsMapAtom` so the rail refetches mounts (fingerprint dep already added in commit `ed0ff02`).

The existing AppShell drop handler (`AppShell.tsx:113-150`) stays untouched — it serves the same flows for drag-drop. The footer buttons are **explicit click affordances** for users who don't drag.

## Section 5: FileRowMenu

Per-file hover popover. Built on shadcn `DropdownMenu`. The trigger `<button><MoreHorizontal /></button>` is:

- Slotted into `FileTreeNode`'s right-edge container, which is **always present** (zero-width when invisible) so list rows don't reflow on hover.
- `invisible group-hover:visible focus-visible:visible data-[state=open]:visible` — appears on hover OR keyboard focus OR while the menu is open.
- `size-6 rounded text-muted-foreground hover:text-foreground hover:bg-accent/70`.

**Menu items (in order):**

| # | Label | Icon | Enabled when | Handler |
|---|---|---|---|---|
| 1 | 添加到聊天 | `MessageSquarePlus` | `!isDirectory` | `addPendingAttachmentAction({ mountId, relPath, name, sessionId, absolutePath })` (existing) |
| 2 | 在文件夹中显示 | `FolderSearch` | always | `invoke('reveal_path_in_file_manager', { path: absolutePath })` |
| 3 | 移动到… | `FolderInput` | `mount.kind === 'workspace'` | open `MoveToDialog` |
| 4 | 重命名 | `Pencil` | `mount.kind === 'workspace'` | set `renamingFilePathAtom` to this row's absolutePath |
| 5 | (separator) |  |  |  |
| 6 | 删除 | `Trash2` (text-destructive) | `mount.kind === 'workspace'` | open `DeleteConfirmDialog` |

**Disabled-item UX:** When `mount.kind !== 'workspace'`, items 3/4/6 render in the menu but with `data-disabled` + tooltip `只读 — 编辑此挂载点需要批准`. They remain visible so users can discover the actions exist, with a clear explanation of why they're unavailable on the current mount.

**Why workspace-mount-only for 3/4/6:** `rename_artifact / move_artifact / delete_artifact_recursive` resolve paths under `<data_dir>/spaces/<space_id>/workspace/<path>` ([tauri_commands.rs:1302-1356](../../src-tauri/src/tauri_commands.rs#L1302-L1356)). They literally can't operate on paths outside the workspace dir. Session-mount and attached-dir files therefore can't be mutated through these IPCs. Out-of-scope for this wave; documented in the "Out of Scope" section.

## Section 6: RenameInput (inline)

When `renamingFilePathAtom === row.absolutePath`, `FileTreeNode` swaps its `<span>{name}</span>` for a controlled `<input>`:

- Auto-focused, selects the basename (preserves extension selection convention).
- `onKeyDown`: `Enter` commits, `Escape` cancels.
- `onBlur`: commits if no validation error, cancels otherwise.
- **Validation (synchronous, on each keystroke):**
  - Non-empty after trim.
  - Doesn't contain `/`, `\`, or `:`.
  - Doesn't already exist as a sibling (lookup via `useFileTree`'s `nodes` for the parent dir).
  - Error message shown inline below the input, `text-[10px] text-destructive`.
- **Commit:** call `renameArtifact({ spaceId, oldPath: workspaceRelPath, newPath: newWorkspaceRelPath })`. On success, clear `renamingFilePathAtom` and trigger a refetch of the parent dir's children. On error, surface as sonner toast and leave the input open with error styling.

## Section 7: MoveToDialog

A focused folder-picker dialog. Reuses the existing `openFolderDialog()` from `@tauri-apps/plugin-dialog`:

- When triggered from the menu, opens the OS native folder picker rooted at the workspace dir.
- The picked folder must be **inside the same workspace dir** (validate: `picked.startsWith(workspaceDir)`). If not, sonner error: `只能移动到当前工作区内的文件夹`.
- On valid pick, compute the new workspace-relative path: `picked + '/' + originalBasename`. Call `moveArtifact({ spaceId, srcPath, destPath })`. On success, refetch both source and destination parent dirs.

No custom modal UI — defer to the OS picker. A modal browser inside the rail is YAGNI for this wave.

## Section 8a: Chat Composer Chip Icons

The chat composer's attached-file chip (`AttachmentPreviewItem.tsx`) currently renders a hand-rolled uppercase ext-badge (`JS`, `MD`, `PNG`, …) inside a `bg-primary/12` rounded square. This was a stop-gap from PR #127 polish. Replace with the same `@react-symbols/icons` glyph used in the files rail so a file dragged from rail to chat keeps a visually identical icon.

**Change site:** `ui/src/components/chat/AttachmentPreviewItem.tsx` lines 78–98 (the non-image branch).

**Before** (ext-badge):
```tsx
const ext = fileExtBadge(filename)
// ...
<span className="bg-primary/12 text-primary text-[9.5px] font-semibold ...">
  {ext || <FileText className="size-3" />}
</span>
```

**After** (auto-assigned glyph):
```tsx
import { FileTypeIcon } from '@/components/file-browser/FileTypeIcon'
// ...
<FileTypeIcon name={filename} isDirectory={false} size={14} />
```

The image-branch (`previewUrl` thumbnail) is unchanged.

Drop the unused `fileExtBadge` helper and the unused `FileText` import.

**Files touched:** 1.
**LOC delta:** −15 / +4.
**Test impact:** no new tests; existing `AttachmentPreviewItem` snapshot/render tests (if any) re-pass because rendered text doesn't change — only the icon node does.

## Section 8: DeleteConfirmDialog

shadcn `AlertDialog` with:

- Title: `确认删除`
- Body: `确定要删除 <strong>{name}</strong> 吗？此操作不可撤销。`
  - If `isDirectory`: append `（包含其下全部内容）`.
- Cancel button: `取消` (default styling).
- Confirm button: `删除` (`bg-destructive text-destructive-foreground hover:bg-destructive/90`).
- ESC and outside-click close the dialog (cancel).
- Confirm calls `deleteArtifactRecursive(spaceId, workspaceRelPath)`. On success, refetch parent dir + sonner success toast. On error, leave dialog open with error in body.

## State Model

New atoms in `ui/src/atoms/files-rail-row-atoms.ts`:

```ts
export const renamingFilePathAtom = atom<string | null>(null)
export const moveTargetAtom = atom<{ mountId: string; absolutePath: string; name: string } | null>(null)
export const deleteTargetAtom = atom<{ mountId: string; absolutePath: string; name: string; isDirectory: boolean } | null>(null)
```

Each is a single-target atom (no Map keyed by row). Only one rename/move/delete operation can be in flight at a time from the UI — this is a deliberate simplification that mirrors Proma's behavior and avoids multi-target confusion.

Existing atoms re-used (no changes):
- `mountRootsAtomFamily(sessionId)` — mount list (`files-rail-atoms.ts:35`)
- `fileTreeAtomFamily(mountId)` — per-mount tree state
- `expandedPathsAtomFamily(mountId)` — per-mount expanded paths Set; the AttachedDirRow top-level expand uses the sentinel `''` key
- `filesRailRefreshTickAtom` — bumped by header ↻ button
- `workspaceAttachedDirsMapAtom` — updated by footer 附加文件夹 button so panel refetches mounts
- `currentAgentWorkspaceIdAtom` — fallback when mount.id parsing yields no space_id

## Helper: spaceIdForMount

```ts
// ui/src/lib/files-rail-helpers.ts

export function spaceIdForMount(
  mount: { id: string; kind: MountKind },
  currentWorkspaceId: string | null,
): string | null {
  // workspace:<sid>  or  workspace-attached:<sid>:<hash>
  if (mount.id.startsWith('workspace:')) {
    return mount.id.slice('workspace:'.length) || null
  }
  if (mount.id.startsWith('workspace-attached:')) {
    const rest = mount.id.slice('workspace-attached:'.length)
    const colon = rest.indexOf(':')
    return colon >= 0 ? rest.slice(0, colon) : null
  }
  // session:<sid>  or  attached:<sid>:<hash>  — the *session's* workspace is the
  // currently-active workspace (sessions don't migrate workspaces mid-life).
  if (mount.id.startsWith('session:') || mount.id.startsWith('attached:')) {
    return currentWorkspaceId
  }
  return null
}
```

Backed by unit tests in `files-rail-helpers.test.ts` covering each mount kind + malformed IDs.

## Backend IPC Reference

| Frontend wrapper | Tauri command | Required only for |
|---|---|---|
| `renameArtifact` ([tauri-bridge.ts:367](../../ui/src/lib/tauri-bridge.ts#L367)) | `rename_artifact` | Menu item 4 |
| `moveArtifact` ([tauri-bridge.ts:370](../../ui/src/lib/tauri-bridge.ts#L370)) | `move_artifact` | Menu item 3 |
| `deleteArtifactRecursive` ([tauri-bridge.ts:373](../../ui/src/lib/tauri-bridge.ts#L373)) | `delete_artifact_recursive` | Menu item 6 |
| `attachWorkspaceDirectory` ([tauri-bridge.ts:251](../../ui/src/lib/tauri-bridge.ts#L251)) | `attach_workspace_directory` | Footer 附加文件夹 |
| `copyFileIntoWorkspace` ([tauri-bridge.ts:284](../../ui/src/lib/tauri-bridge.ts#L284)) | `copy_file_into_workspace` | Footer 添加文件 |
| `invoke('reveal_path_in_file_manager', ...)` | `reveal_path_in_file_manager` (added in PR #127) | Header ↗ + menu item 2 |
| `addPendingAttachmentAction` ([preview-chip-atoms.ts:89](../../ui/src/atoms/preview-chip-atoms.ts#L89)) | (atom action; calls `preview_read_bytes`) | Menu item 1 |

**Zero new Rust commands.** All backend wiring exists.

## Read-Only Gating Rules

| Action | Workspace mount | Attached dir mount | Session mount |
|---|---|---|---|
| 添加到聊天 (files only) | ✅ | ✅ | ✅ |
| 在文件夹中显示 | ✅ | ✅ | ✅ |
| 移动到… | ✅ | ❌ disabled + tooltip | ❌ disabled + tooltip |
| 重命名 | ✅ | ❌ disabled + tooltip | ❌ disabled + tooltip |
| 删除 | ✅ | ❌ disabled + tooltip | ❌ disabled + tooltip |

Disabled tooltip copy: `只读 — 编辑此挂载点需要批准` (consistent with the Lock affordance from PR #127 polish).

## Error Handling

- **Backend IPC error**: sonner toast with `err instanceof Error ? err.message : String(err)`. Dialog/input stays open so the user can retry.
- **Validation error** (rename): inline error below the input. No toast (would double-fire on every keystroke).
- **Workspace not active** (no `currentWorkspaceId`): footer buttons disabled with tooltip `请先选择工作区`. Menu actions on workspace-mount files unreachable because the mount itself wouldn't render.
- **Watcher events during rename / move / delete**: tree-patch handles the `Renamed` + `Removed` events idempotently — even if the user-initiated mutation lands first via refetch and the watcher event arrives second, applying it to a tree that no longer contains the old node is a no-op.

## Testing Strategy

**Unit:**
- `spaceIdForMount` — all four mount kinds + malformed IDs (8 cases).
- `RenameInput` validation — empty, separator chars, duplicate sibling (3 cases).
- `FileRowMenu` rendering — workspace mount enables 3/4/6, attached-dir mount disables them with tooltip (2 cases).

**Integration (Vitest + jsdom):**
- `WorkspaceFilesPanel` empty state copy switches between "0 attached + 0 workspace files" and "N attached + 0 workspace files" cases.
- `AttachedDirRow` collapse/expand toggles the nested `FileTreeNode` tree.
- Footer 附加文件夹 click calls `attachWorkspaceDirectory` and the panel refetches mounts (mocked invoke).

**Visual regression: out of scope** — uClaw doesn't run snapshot tests today; visual fidelity is verified manually against the Proma reference.

**Target test count:** UI 328 baseline (post-PR #127) → ~340 (+12 for the new components and helper).

## Out of Scope (Punted)

1. **Multi-select** — Proma supports Cmd-click row multi-select with multi-row delete/move. The 3-dot menu acts on single rows in this wave. Adding multi-select later requires only changes inside `FileRowMenu` and `WorkspaceFilesPanel`'s selection state; no contract break.
2. **Rename / move / delete for session-mount and attached-dir files** — the backend artifact commands resolve paths under the workspace dir only. Lifting this needs new IPCs (`rename_session_file`, etc.) or generalizing the existing commands to accept absolute paths.
3. **Custom in-app folder browser for 移动到…** — defer to the OS native picker for now.
4. **Bulk drag-drop reordering** — the rail is a tree, not a sortable list.
5. **Per-row refresh icon** — single header-level ↻ covers all mounts. Per-mount refresh discoverability is preserved via the watcher and keyboard `Cmd-R` (browser refresh) which the Tauri window passes through.
6. **Trash / undo for delete** — deletion is permanent (matches existing `delete_artifact_recursive` semantics). A trash folder + undo toast is a separate, larger workstream.

## Open Risks

- `@react-symbols/icons` folder glyphs depend on directory name conventions. An attached dir named `截图` won't get a special icon — falls back to default folder. Acceptable; matches VS Code's own behavior.
- Path validation in `RenameInput` runs synchronously per keystroke. With ~1000 sibling files (unlikely in workspace dirs but possible) the sibling-existence check is O(n). Use a `Set` lookup over the parent's `nodes` array, not `.find()` — sub-millisecond at any realistic scale.
