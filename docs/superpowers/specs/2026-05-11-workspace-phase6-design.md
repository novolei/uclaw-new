# Phase 6 — Workspace Maturity (Umbrella)

> **Phase 6 of the workspace remediation series.** Bundles three
> independent features that complete the workspace UX. Each ships as its
> own PR in **A → B → C order** — sequentially, not in parallel.

## 1. Goal

After Phase 1-5 (the architectural rebuild + the cross-workspace tab
fixes), workspaces are correct but flat. Phase 6 adds three quality-of-
life features that make heavy-use workspaces actually pleasant:

- **6-A: Pinned sessions** — let users elevate high-value sessions
  above the implicit "recently updated" ordering.
- **6-B: Cross-workspace search palette** — Cmd+K already exists but
  scopes to the active workspace. Default to cross-workspace, group
  results by workspace.
- **6-C: Cost dashboard + budget alerts** — surface per-workspace and
  per-model spend, plus a configurable monthly budget with toast
  warnings at 80% / 100%.

## 2. Non-Goals (Phase 6 as a whole)

- Per-workspace budgets (only a global monthly budget in 6-C; revisit
  if asked).
- Hard-blocking budget enforcement (toast warnings only — user's call).
- Cross-workspace tab views (each workspace still owns its own tab
  list — that's the Phase 5 contract).
- Pinned sessions count caps (visual grouping is enough; no policy
  ceiling).
- Drag-to-reorder pinned sessions (pinned-by-recency is enough; can
  revisit).

## 3. Ordering & Cadence

**Sequential, A → B → C**, one PR at a time:

1. **6-A first** — smallest scope, claims V18 migration. ~400 LOC.
2. **6-B next** — pure frontend + Tauri-search-command tweak, no
   schema. ~600 LOC.
3. **6-C last** — touches `emit_turn_cost` (Rust) + new dashboard page +
   `Settings` shape change. ~500 LOC.

Rationale for sequential:
- A claims the next free migration number (V18). Locking that down
  before B/C means no V-number collision.
- B's cross-workspace UX builds on the **session.workspaceId pattern**
  shipped in PR #83. That pattern's also used by 6-A (the pin state
  on the session). Sequential lets us refine the helper once if
  needed.
- C is the most schema-adjacent (cost rollups depend on `cost_records
  ⋈ agent_sessions`). Doing it last means we can build on any
  stabilization 6-A or 6-B introduces.

Each sub-feature has its own brainstorming-derived sub-spec; this
umbrella covers cross-cutting concerns only.

## 4. Migration Registry Update

After Phase 6 lands:

| V | What | Status |
|---|---|---|
| V11 | trigram messages_fts | **PR #33 still open** — superseded by V12, close as such |
| V12–V17 | merged | |
| **V18** | `agent_sessions.pinned_at INTEGER NULL` | **claimed by Phase 6-A** |
| V19+ | free | |

If a future Phase 6.5 adds per-workspace budgets, V19 would carry
`workspace_budgets`. Out of scope for this Phase.

## 5. Shared Concerns

**No new shared atoms / utilities across A/B/C.** Each touches different
domain atoms (agent-atoms, search-atoms, settings + a new cost atom).
The only cross-cutting reuse is the established **session.workspaceId
pattern** from PR #83 — both 6-A (pin state follows the session) and
6-B (search hits open in their session's workspace) rely on it. No
adaptation needed; it already works.

**Tab tagging on workspace switch**: 6-B opens search-hit sessions via
the existing `useOpenSession` + `AppShell.handleSearchResultSelect`
plumbing that already auto-switches workspace. No new logic required.

## 6. Sibling Fix — `titlebar-drag-region` user-select

A pre-Phase-6 fix lands alongside this work in **PR #89**: the
`.titlebar-drag-region` CSS rule was missing `user-select: none` and
`cursor: default`. Symptom: clicking the empty area between/after
TabBar tabs showed an I-beam cursor and the window drag never started
because WKWebView's text-selection handler raced the OS drag handler
on mousedown. Adding these two declarations makes the established
"drag this region" affordance actually work everywhere it's used
(titlebar, sidebar header, RightSidePanel header, TabBar row, etc.).

This is **not** part of any sub-spec — it's a pre-existing bug fixed
in passing because PR #89 was already touching the TabBar's drag
plumbing. Mentioned here so the umbrella record is complete.

## 7. Testing Strategy

Each sub-spec defines its own tests. At the umbrella level:

- **6-A**: migration runs idempotent; Tauri toggle command flips state;
  WorkspaceRail renders the pinned segment with sort-order correctness.
- **6-B**: search atom returns grouped shape; SearchPalette renders
  per-workspace sections in workspace-bar order; click opens session
  in its own workspace's tab list.
- **6-C**: budget settings round-trip; threshold logic (80% fires at
  exactly 80, 100% fires at exactly 100); dashboard renders 0 / some /
  many cost-record states.

No cross-feature integration tests — A/B/C are functionally
independent.

## 8. Sub-Spec Files

| Feature | Spec | Plan (will be authored after spec approval) |
|---|---|---|
| 6-A Pinned | `docs/superpowers/specs/2026-05-11-workspace-phase6a-pinned-design.md` | `docs/superpowers/plans/2026-05-11-workspace-phase6a.md` |
| 6-B Search | `docs/superpowers/specs/2026-05-11-workspace-phase6b-search-design.md` | `docs/superpowers/plans/2026-05-11-workspace-phase6b.md` |
| 6-C Cost | `docs/superpowers/specs/2026-05-11-workspace-phase6c-cost-design.md` | `docs/superpowers/plans/2026-05-11-workspace-phase6c.md` |

Plans are written one at a time, in shipping order. After 6-A lands,
`writing-plans` produces the 6-B plan; same for C. This keeps
implementation context narrow on one feature at a time.
