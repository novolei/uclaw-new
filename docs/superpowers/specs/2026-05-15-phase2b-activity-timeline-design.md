# Phase 2b: Activity Timeline & Rich Report Display

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Upgrade the automation 动态 tab from a truncated plain-text list to a vertical timeline with rich Markdown report cards, and fix the file-preview dead-click bug in Kaleidoscope.

**Architecture:** Two parallel tracks — (A) ActivityHistoryView/ActivityListItem get a timeline shell and rich card rendering; (B) RunSessionSubView gains a collapsible report card pinned above the existing AgentMessages. One small Rust change adds `workingDir` to `AutomationActivity` so file artifact chips can open files. A one-line fix mounts `PreviewPanel` in `KaleidoscopeShell` so `FilePathChip` clicks work everywhere.

**Tech Stack:** React 18 + TypeScript, Jotai, react-markdown + remark-gfm (already in project), `openFile` / `openExternal` from `@tauri-apps/plugin-shell` (already wired in tauri-bridge), Rust `rusqlite`.

---

## Scope

### Part A — 动态 Tab Timeline

**`ActivityHistoryView`** wraps the activity list in a vertical timeline shell: a continuous left-side connector line with per-item dot nodes, newest run on top.

**`ActivityListItem`** becomes a full timeline card:

| Element | Detail |
|---|---|
| Node dot | Blue=running/queued · Green=completed(useful) · Grey=completed(noop/skipped) · Red=failed |
| Header row | Timestamp · status label · duration · "查看进程 ›" link (right-aligned) |
| `reportOutcome` badge | 有效 / 无操作 / 跳过 / 错误 — shown only when outcome is non-null |
| Body | `reportText` rendered as Markdown via new `ActivityMarkdown` component (always visible, not truncated) |
| Artifact chips | Parsed from `reportArtifactsJson`; `file` → `openFile`, `url` → `openExternal`, `text` → display-only (no click) |
| Running placeholder | When status is `running` or `queued` and `reportText` is null: italic "运行中，暂无报告…" |
| Archived / escalation | Existing behaviours preserved |

**New component `ActivityMarkdown`** (`ui/src/components/automation/ActivityMarkdown.tsx`): compact inline Markdown renderer wrapping `react-markdown + remark-gfm` without the full-page container from `preview/renderers/MarkdownRenderer`. Uses `prose-sm prose-zinc dark:prose-invert` with tight spacing suitable for a card body.

### Part B — RunSessionSubView Report Card

A collapsible "运行报告" card is pinned at the top of `RunSessionSubView`, above the existing `AgentMessages`.

| State | Card content |
|---|---|
| Running / queued, no reportText yet | Placeholder row: "运行中，暂无报告…" (italic, muted) |
| Completed / failed, reportText present | Markdown body + artifact chips + outcome badge |
| Completed / failed, no reportText | Card hidden entirely |

The card is **collapsible** (default: expanded). A `▾ / ▸` chevron button in the card header toggles it. Collapse state is local React state (not persisted).

A thin divider labelled "对话过程" separates the report card from `AgentMessages`.

The existing `AgentMessages` component is **unchanged** — it continues to render the full conversation transcript below.

### Bug Fix — FilePathChip in Kaleidoscope

`FilePathChip` dispatches `openPreviewAction` which sets `previewPanelOpenAtom`. `PreviewPanel` subscribes to that atom, but it is only mounted in `WorkspaceShell`. In Kaleidoscope (where `RunSessionSubView` lives), no panel responds.

**Fix:** import and render `<PreviewPanel />` once inside `KaleidoscopeShell.tsx`. Since only one shell is visible at a time, there is no double-mount conflict.

### Rust Backend — `workingDir` on `AutomationActivity`

File artifact `path` values are relative to the spec's run working directory. The frontend needs the absolute base path to call `openFile`.

**Change:** Add `working_dir: String` to `AutomationActivity` in `src-tauri/src/automation/activity.rs`. Populate it by extending the SQL query inside `get_activity()` with a LEFT JOIN on `spaces`:

```sql
SELECT a.*, COALESCE(s.path, '') AS working_dir
FROM automation_activities a
LEFT JOIN automation_specs sp ON sp.id = a.spec_id
LEFT JOIN spaces s ON s.id = sp.space_id
WHERE a.spec_id = ?1
ORDER BY a.queued_at DESC
LIMIT ?2
```

`row_to_activity()` reads the new column by index. If `spaces.path` is empty or the spec has no space, fall back to `<home>/Documents/workground/automations/<spec_id>` (same logic as the runtime's `workspace_root` calculation in `service.rs:561-570`).

No new Tauri command. No new DB migration. The new field is appended to the existing `get_automation_activity` response.

---

## File Map

| File | Action | Notes |
|---|---|---|
| `src-tauri/src/automation/activity.rs` | Modify | Add `working_dir: String` to struct + populate in `row_to_activity()` |
| `ui/src/lib/tauri-bridge.ts` | Modify | Add `workingDir: string` to `AutomationActivity` interface |
| `ui/src/components/automation/ActivityMarkdown.tsx` | **Create** | Compact inline Markdown renderer |
| `ui/src/components/automation/ActivityListItem.tsx` | Modify | Timeline card: node dot, outcome badge, Markdown body, artifact chips |
| `ui/src/components/automation/ActivityHistoryView.tsx` | Modify | Wrap list in timeline shell (connector line + dots) |
| `ui/src/components/automation/RunSessionSubView.tsx` | Modify | Insert collapsible report card above AgentMessages |
| `ui/src/views/Kaleidoscope/KaleidoscopeShell.tsx` | Modify | Mount `<PreviewPanel />` so FilePathChips work |

---

## Data Flow

```
get_automation_activity(specId, 50)
  → Vec<AutomationActivity>  [incl. workingDir]
  → automationActivitiesAtom[specId]
  → ActivityHistoryView (timeline shell)
    → ActivityListItem × N (timeline cards)
        reportText → ActivityMarkdown
        reportArtifactsJson → JSON.parse → artifact chips
          file chip click → openFile(workingDir + "/" + artifact.path)
          url chip click  → openExternal(artifact.path ?? artifact.title)
          // url artifacts store the URL in path when present, else title

RunSessionSubView
  → reportText / reportArtifactsJson / reportOutcome from activities atom
  → collapsible report card (top)
  → AgentMessages (unchanged, below)

KaleidoscopeShell
  → <PreviewPanel /> (new)
    ← previewPanelOpenAtom / selectedPreviewFileAtom
    ← FilePathChip clicks in AgentMessages
```

---

## Colour / Badge Reference

| `status` | `reportOutcome` | Dot colour | Badge |
|---|---|---|---|
| running / queued | — | `bg-primary` (blue, pulsing) | — |
| completed | useful | `bg-green-500` | 有效 |
| completed | noop | `bg-gray-500` | 无操作 |
| completed | skipped / filtered_out | `bg-gray-500` | 跳过 |
| completed | error | `bg-red-500` | 错误 |
| failed | — | `bg-red-500` | — |
| cancelled | — | `bg-gray-400` | — |
| waiting_user | — | `bg-yellow-500` (existing ring) | — |

Colours use Tailwind utility classes that map to theme tokens (`text-green-500`, `bg-primary`, etc.) so they survive all 11 uClaw themes.

---

## Out of Scope

- **`text` artifact kind** — shown as a chip label but not clickable. No inline expansion in Phase 2b.
- **`report_to_user` multi-call chapters** — requires a new DB structure. Deferred to Phase 2c.
- **Timeline within a run** — restructuring `AgentMessages` into a structured step timeline. Deferred to Phase 2c.
- **Sticky / pinned report card** — report card scrolls with content, not fixed. Can revisit in Phase 2c.

---

## Verification

1. Run a smoke-2a spec → 动态 tab shows a new timeline node; clicking "查看进程 ›" opens RunSessionSubView with report card on top.
2. After run completes → report card shows `reportText` rendered as Markdown (headings, lists, bold).
3. If `reportArtifactsJson` has a `file` entry → chip appears; clicking opens the file in system default app.
4. If `reportArtifactsJson` has a `url` entry → chip appears; clicking opens in browser.
5. While run is still `running` → report card shows italic placeholder.
6. In RunSessionSubView header, click ▾ → card collapses; click ▸ → expands.
7. In Kaleidoscope, open a run session → click a `FilePathChip` in AgentMessages → file preview panel opens (no longer dead).
8. `cd ui && npx tsc --noEmit` passes.
9. `npm test -- --run` — all existing tests pass (668+ tests, 0 new failures).
