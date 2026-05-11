# Phase 6-B — Cross-Workspace Search Palette

> **Sub-feature of [Phase 6](./2026-05-11-workspace-phase6-design.md).**
> Second to ship, after 6-A merges. Default the search palette to
> cross-workspace mode with results grouped by workspace.

## 1. Problem

The SearchPalette (Cmd+K) currently scopes to the active workspace.
With multiple workspaces in heavy use, finding "that conversation about
the Tauri build flow last week" requires either:
- Manually switching workspaces and searching each one, or
- Remembering exactly where the conversation was filed.

Both are friction the user explicitly doesn't want. The backend can
already search across workspaces (V12 trigram FTS doesn't enforce
workspace scope) — it's just a UI gate.

## 2. Goal

Default the Cmd+K palette to **cross-workspace** mode. Results group
by workspace, with the active workspace's section pinned to the top.
Clicking a hit opens the session in **its own workspace's tab list**
(reusing the `session.workspaceId` pattern from PR #83) and
auto-switches the active workspace if needed.

## 3. Non-Goals

- A toggle for "active workspace only" — the cross-workspace default
  covers the use case; if a user wants to narrow, they can prefix
  the query with the workspace name (existing fuzzy match already
  does this for the workspaces section).
- Result pagination — results capped at 50 total, distributed by
  relevance.
- Per-workspace sort order within results — within a workspace,
  FTS rank order is fine.
- New keyboard shortcuts. Existing Cmd+K + arrow + Enter flow stays.

## 4. Backend Changes

### Tauri command tweak

The search command (`search_conversations` or equivalent — check
`src-tauri/src/tauri_commands.rs`) already accepts an optional
`workspace_id` filter. The frontend currently always passes the
active workspace's id. The fix is purely frontend: don't pass the
workspace_id parameter.

If the Rust side requires `workspace_id` as non-optional, change its
type to `Option<String>` and skip the WHERE clause when None. Audit
needed during implementation.

### Result shape

Existing `SearchResult` already carries enough info per hit. The
frontend groups client-side by `workspaceId` (already returned in the
result). No new IPC fields.

## 5. Frontend Changes

### Atom

`ui/src/atoms/search-atoms.ts` — the search-results atom keeps its
existing shape. Add a derived **grouped** view:

```ts
export interface SearchResultGroup {
  workspaceId: string
  workspaceName: string
  workspaceIcon: string  // resolved via getWorkspaceIcon
  hits: SearchResult[]
}

export const searchResultsGroupedAtom = atom<SearchResultGroup[]>((get) => {
  const results = get(searchResultsAtom)
  const workspaces = get(workspacesAtom)
  const activeId = get(activeWorkspaceIdAtom)

  // Group hits by workspaceId
  const byWs = new Map<string, SearchResult[]>()
  for (const r of results) {
    const ws = r.workspaceId ?? 'default'
    if (!byWs.has(ws)) byWs.set(ws, [])
    byWs.get(ws)!.push(r)
  }

  // Sort groups: active workspace first, then by workspace-bar order
  // (workspaces atom is already sorted by sortOrder).
  const sortedWsIds = workspaces.map((w) => w.id)
  const groupOrder = [
    activeId,
    ...sortedWsIds.filter((id) => id !== activeId),
  ].filter(Boolean) as string[]

  return groupOrder
    .filter((wsId) => byWs.has(wsId))
    .map((wsId) => {
      const ws = workspaces.find((w) => w.id === wsId)
      return {
        workspaceId: wsId,
        workspaceName: ws?.name ?? '默认工作区',
        workspaceIcon: ws?.icon ?? 'Folder',
        hits: byWs.get(wsId) ?? [],
      }
    })
})
```

### UI: SearchPalette grouped render

`ui/src/components/search/SearchPalette.tsx` — split the existing
"search hits" section into a per-workspace grouped render:

```tsx
const groups = useAtomValue(searchResultsGroupedAtom)

{groups.map((group) => {
  const Icon = getWorkspaceIcon(group.workspaceIcon)
  return (
    <div key={group.workspaceId} className="space-y-0.5">
      <div className="flex items-center gap-1.5 px-2 py-1">
        <span className="inline-flex items-center justify-center
                          size-4 rounded bg-primary/15 text-primary">
          <Icon className="size-3" />
        </span>
        <span className="text-[11px] font-semibold uppercase
                          tracking-wide text-muted-foreground">
          {group.workspaceName} · {group.hits.length}
        </span>
      </div>
      {group.hits.map((hit) => (
        <SearchHitRow key={hit.id} hit={hit} ... />
      ))}
    </div>
  )
})}
```

### Result cap + truncation

Total cap: **50 hits across all workspaces** (sent to backend). Within
each group, render at most **5 visible**; if a group has more, show a
"在该工作区内还有 N 条" link that closes the palette and opens the
search view scoped to that workspace (existing per-workspace search
flow). Implementation: pass `limit: 50` to the backend; client-side
slice to 5 per group; render the "more" link conditionally.

### Hit click → open session in its workspace

The existing `handleSearchResultSelect` in AppShell already opens
results via `useOpenSession`. The `useOpenSession` from PR #83 already
tags the new tab with `session.workspaceId` (not active workspace).
The active workspace doesn't auto-switch today; add that:

```ts
// In handleSearchResultSelect's 'search_hit' case, after opening:
if (session?.workspaceId && session.workspaceId !== activeWorkspaceId) {
  void selectWorkspace(session.workspaceId)
}
```

Now clicking a cross-workspace hit:
1. Opens the session's tab in its own workspace's tab list (PR #83)
2. Auto-switches to that workspace so the tab is immediately visible
3. The right panel + body update via TabSessionSyncer (Phase 5)

## 6. Search-Empty Behavior

When the search input is empty, the palette shows "browse" mode
(recent threads, settings shortcuts, workspaces). That stays unchanged
— it's already cross-workspace.

## 7. Edge Cases

- **No hits in a workspace** → that workspace's group is omitted (don't
  render empty headers).
- **Search hits in a deleted workspace** → the session re-homed to
  'default' (Phase 1), so the hit appears under "默认工作区". No special
  handling.
- **Long workspace names** → group header truncates with ellipsis;
  full name in `title` attribute.
- **Workspace icon missing** → `getWorkspaceIcon` already falls back
  to Folder.
- **Slow FTS query** (50+ workspaces, complex query): set a 500ms
  debounce on input (probably already exists). Show a loading
  indicator above the groups list.
- **Cross-workspace hit + currently in the destination workspace**:
  selectWorkspace is a no-op when already on that workspace. ✓

## 8. Tests

Vitest:

- `searchResultsGroupedAtom` groups by workspaceId.
- Group order: active workspace first, then workspacesAtom order.
- Empty workspaces are omitted from the output.
- SearchPalette renders the per-workspace section headers with
  workspace name + count + icon.
- Clicking a cross-workspace hit calls `selectWorkspace` and
  `useOpenSession` with the session's workspaceId.
- 5-hits-per-group cap renders correctly when a group has 10 hits.

Rust unit (if Tauri command shape changed):
- `search_*` command without `workspace_id` param returns hits from
  all workspaces.

## 9. Commit Shape (4 commits)

1. `refactor(search): allow Tauri command to accept null workspace_id`
2. `feat(search): searchResultsGroupedAtom derives per-workspace groups`
3. `feat(search): SearchPalette renders cross-workspace results grouped by workspace`
4. `feat(search): auto-switch active workspace when opening a cross-workspace hit`

Bisectable: after 1, backend supports the query; after 2, atom is
queryable but UI doesn't use it; after 3, palette shows grouped results;
after 4, full feature works including workspace switch.

## 10. Risks

- **Backend change**: if the search Tauri command currently requires
  `workspace_id` as non-optional, changing the signature is a breaking
  API change. Mitigation: introduce `workspace_id: Option<String>` (Rust
  side) — backward-compatible since None is allowed but the existing
  frontend always passes Some(activeWorkspaceId). After the frontend
  switches to passing None, the old behavior is unreachable.
- **Group order surprises**: if a user moves a workspace in the
  switcher's order, search groups reorder too. That's actually desired
  (UI consistency).
- **Result-count cap surprise**: 50 hits across many workspaces could
  mean some workspaces show 0 hits while others show 5. Acceptable;
  users wanting more can click "在该工作区内还有 N 条" or run a
  workspace-scoped search.
