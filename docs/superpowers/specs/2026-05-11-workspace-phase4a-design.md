# Workspace Phase 4a — Create-modal + Shortcuts + Workspace chip

**Status**: Spec
**Date**: 2026-05-11
**Author**: Ryan + Claude (uClaw repo)
**Phase**: 4a of 4 (4b deferred: Apple Mail source list rewrite of LeftSidebar)
**Prerequisite**: [Phase 3 spec](./2026-05-11-workspace-phase3-design.md) merged as `a5a24a4` on 2026-05-11

---

## 1. Background

Phases 1-3 fixed the workspace data model and the agent sandbox. The remaining gaps are pure UX:

- **`WorkspaceCreateDialog`** auto-derives the on-disk folder from a slug of the workspace name. There's no way to pick an explicit path, even though Phase 2's `createWorkspace(name, path?, icon?)` IPC already accepts one. CJK names produce `workspace-<id-prefix>/` (Phase 2 fallback) which is confusing on disk.
- **Switching workspaces requires clicking the sidebar tree**. No keyboard shortcut. With 5+ workspaces this becomes the dominant friction.
- **No top-level indicator of the active workspace**. Once focused into a tab, the only way to see which workspace you're in is to glance at the sidebar header — and there's no quick action on it.

Phase 4a closes these three gaps with isolated, low-risk UI additions. No backend changes; no migrations.

The fourth original Phase 4 item — Apple Mail source list rewrite of `LeftSidebar` — is a much bigger redesign with its own design loop. Deferred to **Phase 4b**.

## 2. Goals

1. **Folder picker in `WorkspaceCreateDialog`**: optional override; default behavior (auto-slug) unchanged so existing one-click "type name → Create" flow still works.
2. **`⌘ 1..9` / `Ctrl+1..9` workspace switching shortcuts**: position-based, follows `sort_order ASC` ordering. Out-of-range presses are silent no-ops.
3. **`TabBarWorkspaceChip`**: leftmost element in the TabBar, shows active workspace's emoji + (truncated) name with a chevron hint. Click opens a dropdown listing all workspaces with their shortcut hints and a "+ 新建工作区" footer entry.
4. **No backend changes, no migrations**.
5. **Build green at every commit** — ~3-4 commits, single PR.
6. **Tests**: ~10 Vitest cases. (No Rust tests because no Rust changes.)

## 3. Non-Goals (deferred)

- **Apple Mail source list rewrite** — Phase 4b.
- **Warn-on-non-empty-folder when user picks an override path** — backend currently writes alongside existing contents without checking; Phase 4a accepts this as "user knows what they're doing". A future patch could surface a warning toast.
- **Live slug-uniqueness validation in CreateDialog** — the backend already handles collisions silently via `compute_workspace_dir` falling back to `workspace-<id-prefix>` when a slug-derived name conflicts (Phase 2). Phase 4a doesn't re-implement this client-side; the preview is informational only.
- **Customizable shortcut chord** — `⌘ 1..9` is set in defaults; user can already remap via the Shortcut Settings panel which reads from `SHORTCUT_DEFINITIONS`. No per-feature override UI.
- **Filter / search input inside the workspace dropdown** — YAGNI for fewer than 20 workspaces. Revisit if users complain.
- **Visual breadcrumb showing session path** — the active session's path is already visible in the tab title and in the right panel's 工作区文件 header. Phase 4a doesn't duplicate this.

## 4. Detailed Design

### 4.1 `WorkspaceCreateDialog` folder picker

File: `ui/src/components/workspace/WorkspaceCreateDialog.tsx`. The dialog grows one new row below the existing name input:

```
┌─ New Workspace ─────────────────────────┐
│ [📁] [💼] [🚀] [🔬] [✍️] [🎯] [🏠] [⚙️]    │
│                                          │
│ Workspace name: [_________________]      │
│                                          │
│ 目录:                                    │
│ ~/Documents/workground/my-project        │  ← slug preview (muted)
│ [📂 选择其他位置...]  [✕ 清除]              │  ← clear only when overridden
│                                          │
│              [Cancel]  [Create]          │
└──────────────────────────────────────────┘
```

**State**:
```ts
const [overridePath, setOverridePath] = React.useState<string | null>(null)
const computedPath = React.useMemo(() => {
  if (overridePath) return overridePath
  const slug = name.toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32)
  return slug ? `~/Documents/workground/${slug}` : '~/Documents/workground/...'
}, [name, overridePath])
```

**Submit**:
```ts
await bridge.createWorkspace(name.trim(), overridePath ?? undefined, icon)
```

Passing `undefined` lets the backend's `compute_workspace_dir` (Phase 2) own the slugify rule — the client-side preview is purely informational. This prevents drift if the slug algorithm ever changes.

**Folder picker handler**:
```ts
const handlePickFolder = async () => {
  try {
    const picked = await openFolderDialog()
    if (picked) setOverridePath(picked.path)
  } catch (err) {
    toast.error(`选择文件夹失败: ${err}`)
  }
}
```

**Reset on close**: both `name` and `overridePath` clear when `onClose` fires (matches existing pattern).

**Edge cases**:
- Empty name → preview shows placeholder; Create button stays disabled by existing `!name.trim()` guard.
- CJK-only name → slug is empty; preview shows placeholder; backend falls back to `workspace-<id-prefix>/`. Hint text in the preview row clarifies: "若名称只含中文,将自动生成 workspace-xxx 目录".
- User selects a non-empty folder → no warning; backend accepts (acknowledged risk per §3).

### 4.2 Workspace switching shortcuts

Files:
- `ui/src/lib/shortcut-defaults.ts` — 9 new definitions appended to `SHORTCUT_DEFINITIONS`.
- `ui/src/components/shortcuts/GlobalShortcuts.tsx` — 9 new handler entries.

**Definitions**:
```ts
// ─── 工作区切换 ───
...Array.from({ length: 9 }, (_, i) => ({
  id: `switch-workspace-${i + 1}`,
  label: `切换到第 ${i + 1} 个工作区`,
  group: '导航',
  mac: `Cmd+${i + 1}`,
  win: `Ctrl+${i + 1}`,
}))
```

**Handler block**:
```ts
const workspaces = useAtomValue(workspacesAtom)
const selectWorkspace = useSetAtom(selectWorkspaceAtom)

const workspaceShortcuts = Array.from({ length: 9 }, (_, i) => ({
  id: `switch-workspace-${i + 1}`,
  handler: () => {
    const ws = workspaces[i]
    if (!ws) return
    void selectWorkspace(ws.id)
  },
}))

useShortcuts([
  // ...existing entries...
  ...workspaceShortcuts,
])
```

**Ordering**: `workspacesAtom` is populated by `listSpaces()` which returns rows `ORDER BY sort_order ASC` (Phase 2 V17 migration + IPC). Drag-reorder in `WorkspaceRail` (Phase 2) re-bumps `sort_order` via `reorder_workspaces`, so the keyboard shortcut binding follows the user's visual order automatically.

**Conflict audit** (against existing `SHORTCUT_DEFINITIONS`):

| Existing chord | Conflict with ⌘ N? |
|---|---|
| `Cmd+N` (new chat) — N is the letter | No (we use digits) |
| `Cmd+Shift+N` (new agent session) | No |
| `Cmd+W` (close tab), `Cmd+B` (sidebar), `Cmd+L` (focus input), `Cmd+F` (search), `Cmd+,` (settings), `Cmd+K` (palette) | All letter/punctuation keys |
| `Cmd+[` / `Cmd+]` (prev/next tab) | No |

No conflicts. `Cmd+1` … `Cmd+9` are free.

**Browser convention note**: in web browsers, `Cmd+1..9` switches between the first 9 tabs. uClaw uses `Cmd+[/]` for tab nav, so `Cmd+1..9` is repurposable here without breaking user muscle memory in this app.

### 4.3 `TabBarWorkspaceChip`

New file: `ui/src/components/tabs/TabBarWorkspaceChip.tsx`.

**Trigger**:
```tsx
<button
  className="titlebar-no-drag flex items-center gap-1 px-2 py-1 rounded-md
             text-[12px] text-foreground/80 hover:text-foreground
             hover:bg-foreground/[0.04] transition-colors shrink-0"
  title={`工作区: ${active.name}`}
>
  <span className="leading-none">{active.icon}</span>
  <span className="font-medium">{truncatedName}</span>
  <ChevronDown className="size-3 text-muted-foreground/60" />
</button>
```

Truncation: names > 12 chars → first 12 + `…`. The full name appears in the tooltip and in the dropdown.

**Dropdown content** (Radix `DropdownMenu`):
- One `DropdownMenuItem` per workspace, in `sort_order ASC` order:
  - `✓` mark on the active workspace; transparent otherwise (preserves alignment).
  - Emoji + full name (truncates with CSS, not JS).
  - `⌘ N` (or `Ctrl+N`) hint on the right for first 9 entries; 10+ shows no hint.
- `DropdownMenuSeparator`.
- "+ 新建工作区" item → opens the existing `WorkspaceCreateDialog`.

```tsx
<DropdownMenuContent align="start" sideOffset={4} className="w-56 z-[100]">
  {workspaces.map((w, i) => (
    <DropdownMenuItem
      key={w.id}
      onSelect={() => void selectWorkspace(w.id)}
      className="flex items-center gap-2 text-xs"
    >
      <Check className={cn('size-3.5 shrink-0', w.id === activeId ? 'opacity-100' : 'opacity-0')} />
      <span className="text-[13px] leading-none">{w.icon}</span>
      <span className="flex-1 truncate">{w.name}</span>
      {i < 9 && (
        <span className="text-[10px] text-muted-foreground/60 font-mono shrink-0">
          {modPrefix}{i + 1}
        </span>
      )}
    </DropdownMenuItem>
  ))}
  <DropdownMenuSeparator />
  <DropdownMenuItem
    onSelect={() => setCreateOpen(true)}
    className="flex items-center gap-2 text-xs text-primary"
  >
    <Plus className="size-3.5" />
    新建工作区
  </DropdownMenuItem>
</DropdownMenuContent>
```

**Platform-aware modifier**:
```ts
const isMac = /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)
const modPrefix = isMac ? '⌘' : 'Ctrl+'
```

**Mount point**: `ui/src/components/tabs/TabBar.tsx`. The chip becomes the first child of the existing horizontal flex container that holds the tab list — placed before tabs, after any leading drag region. Requires `titlebar-no-drag` (the TabBar wrapper is a Tauri drag region).

**Hide case**: if `workspaces.find(w => w.id === activeId)` returns undefined, the component renders `null` (no chip). In practice the V16 migration from Phase 1 always backfills `'default'` so this branch is defensive only.

**Reactivity**: `workspacesAtom` is the single source. Rename / reorder / delete via Phase 2's existing flows all re-render the chip and the dropdown automatically.

## 5. Error Handling

- **`openFolderDialog` cancelled**: returns `null` — `setOverridePath` not called; UI state unchanged.
- **`openFolderDialog` rejected** (rare — permission error, no Tauri runtime): toast error; preview stays as-is.
- **`createWorkspace` failure** (existing behavior): console error + dialog stays open. (Phase 4a doesn't add a toast since this is pre-existing UX; pile it onto Phase 4b polish.)
- **Shortcut handler with stale workspace list**: `useAtomValue(workspacesAtom)` re-subscribes on render — the handler closure captures the latest array each time `useShortcuts` is called. No stale-state risk.
- **DropdownMenu z-stacking** in some themes: explicit `z-[100]` on `DropdownMenuContent` to clear the sidebar's `z-[60]` stacking context (Phase 2-established pattern).

## 6. Testing

**Vitest** (UI tests, jsdom):

| File | Cases |
|---|---|
| `TabBarWorkspaceChip.test.tsx` | renders active workspace; truncates >12 chars; click trigger opens menu; menu item click calls selectWorkspace; "+ 新建工作区" opens CreateDialog; returns null when no active workspace |
| `WorkspaceCreateDialog.test.tsx` | preview follows slug; "选择其他位置" calls openFolderDialog and overrides; "清除" reverts to slug; submit passes overridePath when set, undefined when not |
| `GlobalShortcuts.test.tsx` (new) | `switch-workspace-3` calls `selectWorkspace(workspaces[2].id)`; out-of-range no-op; ordering follows atom value |

Total: ~10 cases. Mocked `@/lib/tauri-bridge` calls + a Jotai store seeded with 3-4 fixture workspaces.

**Manual smoke checklist** (recorded in PR description):

1. **Chip render**: launch app in workspace 2222 → see `[📁 2222 ▾]` at TabBar left edge.
2. **Switch via click**: click chip → dropdown lists all workspaces with `⌘ 1..N` hints → click another row → workspace switches; chip updates; left sidebar tree highlights new active workspace.
3. **Switch via shortcut**: press `⌘1` through `⌘N` (N = workspace count) → workspace switches each time. `⌘N+1` (out of range) → no-op.
4. **CreateDialog default path**: open New Workspace → type "My Project" → preview shows `~/Documents/workground/my-project` → Create → folder appears on disk.
5. **CreateDialog override**: type "test" → 选择其他位置 → pick `~/Desktop/somewhere` → preview shows that → 清除 → preview reverts. Pick again, Create → folder is at chosen path.
6. **CJK-only name**: type "测试" → preview placeholder → Create → backend creates `workspace-<id-prefix>/`.
7. **Rename reactivity**: rename a workspace via sidebar → chip text updates.

## 7. PR Shape (bisectable commits)

| # | Commit | LOC est |
|---|---|---|
| 1 | `feat(shortcuts): ⌘ 1..9 / Ctrl+1..9 for workspace switching + tests` | ~120 |
| 2 | `feat(workspace): folder picker in CreateDialog + slug preview + tests` | ~180 |
| 3 | `feat(tabs): TabBarWorkspaceChip + dropdown switcher + tests` | ~280 |

Total: ~580 LOC. Build green at every commit. Each commit ships a working feature in isolation; commit 3 depends on commit 2 only for the "+ 新建工作区" wiring (uses the slightly-extended `WorkspaceCreateDialog`).

## 8. Open Questions / Risks

- **`Cmd+1..9` vs browser convention**: in web browsers, this switches tabs. uClaw uses `Cmd+[/]` for tab nav. We're repurposing the digit chord for workspace switching; users familiar with multi-tab browsers may briefly fumble. Acceptable given the small workspace count typical in this app.
- **Long workspace names + many workspaces**: with 15+ workspaces the dropdown could grow tall. Radix `DropdownMenuContent` defaults to fitting viewport; if it overflows, the user gets internal scrolling. Acceptable for Phase 4a.
- **Slug preview vs backend reality**: the client-side preview may differ from the backend's actual chosen path if a collision occurs (backend appends `(2)`, `(3)`, …). The preview shows what *would* be created in the happy path; the actual path is returned from `createWorkspace` and used for subsequent UI updates. Phase 4a accepts this minor mismatch.
- **Settings panel duplication**: each of the 9 new shortcut entries shows up as its own row in Shortcut Settings. With 9 entries it's slightly long but matches Apple Mail's per-mailbox shortcut listing pattern. Could be collapsed into a "工作区切换 (Cmd+1..9)" composite row in Phase 4b if it bothers users.
