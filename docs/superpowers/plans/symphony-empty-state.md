# Symphony Empty State — Hero × Template Gallery

One PR, one branch (`feat/symphony-empty-state`), bisectable commits. Closes the UI gap: clicking the Symphony mode segment with no workflows currently leaves the user staring at "Loading workflow…" forever.

## Goal

When the user has zero Symphony workflows (fresh install, first time in mode), present an inviting hero + 3 starter templates + quick actions so the path from "what is this?" to "I have a workflow running" is one click long.

Picked over the minimal blank-slate (Option A) and the list+empty hybrid (Option C) because templates surface the DAG concept visually — which is the differentiator between Symphony and the other two runtimes.

## Architecture

The current code path: `ModeSwitcher → open tab with workflowId → SymphonyCanvas(workflowId) → symphonyGetWorkflow(workflowId)`. When `workflowId` matches no row, the load fails and the canvas sticks on the "Loading…" placeholder.

Fix at two surfaces:

1. **ModeSwitcher** — when target mode is `symphony` and there's no current workflow id AND `symphonyWorkflows.length === 0`, open a tab with a sentinel `sessionId = '__symphony_new__'` instead of falling through to bare `setMode('symphony')` (today's behavior leaves no tab open at all).
2. **SymphonyCanvas** — treat the sentinel and any workflow id whose `symphonyGetWorkflow` call comes back not-found as the empty state, render `<SymphonyEmptyState />` instead of the canvas chrome.

`SymphonyEmptyState` owns the hero copy, the 3 template cards (with mini-DAG previews), and the quick-action row (Blank workflow / Import .md / View docs). Clicking a template calls `symphonySaveWorkflow(template.def, template.md)`; on success it dispatches an action that swaps the current tab's `sessionId` to the new workflow id and refreshes the workflow list atom, so the next render mounts the real `<SymphonyCanvas>`.

## Tasks

### T1. docs: plan

This file.

### T2. ui: templates data module

- **File (new):** `ui/src/components/symphony/templates.ts`
- Exports `SYMPHONY_TEMPLATES: readonly StarterTemplate[]` where `StarterTemplate = { id, name, description, def: SymphonyWorkflowDef, definitionMd: string, miniDag: MiniDag }`.
- 3 templates:
  - `linear-chain` — fetch → process → report (3 nodes)
  - `diamond-fan-out` — a → (b1, b2) → c (4 nodes)
  - `research-draft-review` — research → draft → review (3 nodes, content-pipeline framing)
- `MiniDag = { width, height, nodes: {x,y}[], edges: {fromIndex, toIndex}[] }` — pure data, rendered by `MiniDagPreview` inside the card.
- **Verify:** `npx tsc --noEmit` clean.

### T3. ui: `SymphonyEmptyState` component

- **File (new):** `ui/src/components/symphony/EmptyState.tsx`
- Sub-component `MiniDagPreview({mini}: {mini: MiniDag})` — pure SVG: circles for nodes, curved bezier paths for edges. `stroke="currentColor"` so it inherits the parent's text color (theme-tokens FTW). Faint pulse animation on hover via Tailwind `animate-pulse` only when the parent card is `:hover`.
- Layout:
  - Vertical centered flex inside `flex-1` parent
  - Hero zone: `Network` lucide icon (matches ModeSwitcher), `text-foreground` headline, `text-muted-foreground` subhead
  - Template gallery: `grid-cols-3 gap-4` of `<TemplateCard>` (rounded-lg border bg-card hover:bg-accent/30 hover:border-accent transition)
  - Quick action row: 3 buttons — "Blank workflow" (primary), "Import .md" (secondary), "View docs" (ghost + external-link icon)
- Click handlers:
  - Template card → `onCreate(template)` prop → parent calls `symphonySaveWorkflow(def, md)` and on success swaps tab + refreshes list
  - Blank → `onCreate(BLANK_TEMPLATE)` with `def = { id: crypto.randomUUID(), name: 'Untitled', nodes: [], edges: [], ... }`
  - Import .md → opens a `<dialog>` with a single textarea + Cancel/Import buttons → `symphonyImportWorkflowMd(source)`
  - View docs → external link to the spec path inside the app (or just opens GH if/when published)
- **Theming:** All colors via tokens (`bg-card`, `border-border`, `text-muted-foreground`, `bg-accent/30`, `text-primary`). NO hardcoded zinc/gray.
- **Verify:** `npx tsc --noEmit` clean.

### T4. ui: wire-up — SymphonyCanvas + ModeSwitcher + index.ts

- **Files:**
  - `ui/src/components/symphony/index.ts` — add `export { SymphonyEmptyState }`.
  - `ui/src/components/symphony/SymphonyCanvas.tsx`:
    - Add `const SENTINEL_NEW = '__symphony_new__'`.
    - At top of render, if `workflowId === SENTINEL_NEW || (loadFailed && !detail)`, render `<SymphonyEmptyState onCreated={handleCreated} />` and return.
    - `handleCreated(newWorkflowId)` updates the tab's `sessionId` via `openSession('symphony', newWorkflowId, workflow.name)` from `session-tabs` atom, then `setCurrentWorkflow(newWorkflowId)`.
    - Distinguish "loading" from "not-found" by setting a `loadFailed` boolean state in the `symphonyGetWorkflow` catch block.
  - `ui/src/components/app-shell/ModeSwitcher.tsx`:
    - In the `else if (targetMode === 'symphony')` branch, after `recent` is undefined, instead of falling through to `setMode(targetMode)` call `openSession('symphony', SENTINEL_NEW, 'New workflow')` then `setMode('symphony')`.
- **Verify:** `npx tsc --noEmit` clean. Manual: `cargo tauri dev` → click Symphony segment (with empty db) → see hero.

### T5. ui: tests

- **File (new):** `ui/src/components/symphony/EmptyState.test.tsx`
  - Renders hero + 3 template cards + 3 quick-action buttons
  - Click template card fires `onCreate` with the right template id
  - Click "Blank workflow" fires `onCreate` with a `BLANK_TEMPLATE` whose def.nodes is empty
  - Click "Import .md" opens the import dialog (textarea present)
- **Verify:** `npm test -- --run EmptyState`.

## Commit table

| # | Title                                                          | Files                               | Verified by                          |
| - | -------------------------------------------------------------- | ----------------------------------- | ------------------------------------ |
| 1 | docs: symphony empty-state plan                                | docs/superpowers/plans/...md        | `git diff` review                    |
| 2 | ui(symphony): starter template defs + mini-DAG metadata        | symphony/templates.ts               | `tsc --noEmit`                       |
| 3 | ui(symphony): hero × template gallery empty state              | symphony/EmptyState.tsx, index.ts   | `tsc --noEmit`                       |
| 4 | ui(symphony): wire empty state into canvas + mode switcher     | SymphonyCanvas.tsx, ModeSwitcher.tsx | `tsc --noEmit` + manual smoke       |
| 5 | ui(symphony): EmptyState component tests                       | EmptyState.test.tsx                 | `npm test -- --run EmptyState`       |

## Out of scope

- Real WORKFLOW.md template browser with imports from disk
- Onboarding tour / coachmarks
- Workflow library marketplace integration
- Empty-state translations (i18n is not on in this repo)
